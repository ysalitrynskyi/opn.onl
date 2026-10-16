import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { API_ENDPOINTS, authFetch, SessionExpiredError } from '../config/api';
import { safeLocalStorage } from '../utils/storage';

/** How long the row count keeps its highlight after a click. */
const FLASH_MS = 700;

/** A frame with no delimiter larger than this is not a click event. */
const MAX_SSE_BUFFER = 64 * 1024;

const BACKOFF_BASE_MS = 1_000;
const BACKOFF_CAP_MS = 60_000;

export type LiveStatus = 'connected' | 'reconnecting' | 'off';

export interface LiveClicks {
    status: LiveStatus;
    /** Non-zero while `linkId`'s count should play the click flash. */
    flashNonce: (linkId: number) => number;
}

/**
 * Equal jitter: half to all of the exponential delay, never above 60s and
 * never zero, so a failure cannot spin. `attempt` 0 is the first failure.
 */
export function liveBackoffMs(attempt: number, random: () => number = Math.random): number {
    const exponent = Math.min(Math.max(attempt, 0), 16);
    const exp = Math.min(BACKOFF_CAP_MS, BACKOFF_BASE_MS * 2 ** exponent);
    return Math.floor(exp / 2 + random() * (exp / 2));
}

/** Complete SSE `data` payloads in `buffer`, and the trailing partial frame. */
export function takeSseData(buffer: string): { data: string[]; rest: string } {
    const data: string[] = [];
    let rest = buffer;
    for (;;) {
        const split = rest.indexOf('\n\n');
        if (split === -1) break;
        const raw = rest.slice(0, split);
        rest = rest.slice(split + 2);
        const payload = raw
            .split('\n')
            .filter((line) => line.startsWith('data:'))
            .map((line) => (line.startsWith('data: ') ? line.slice(6) : line.slice(5)))
            .join('\n');
        if (payload !== '') data.push(payload);
    }
    return { data, rest };
}

function linkIdFromPayload(payload: string): number | null {
    try {
        const event = JSON.parse(payload) as { link_id?: unknown };
        const id = event.link_id;
        return typeof id === 'number' && Number.isInteger(id) ? id : null;
    } catch {
        return null;
    }
}

function isEventStream(response: Response): boolean {
    const type = response.headers?.get('content-type') ?? '';
    return type.toLowerCase().includes('text/event-stream');
}

/** `Retry-After` as milliseconds, or 0 when the header is absent or not a delay. */
function retryAfterMs(response: Response): number {
    const raw = response.headers?.get('retry-after');
    if (!raw) return 0;
    const seconds = Number(raw);
    if (!Number.isFinite(seconds) || seconds < 0) return 0;
    return seconds * 1000;
}

function isAbortError(error: unknown): boolean {
    return error instanceof Error && error.name === 'AbortError';
}

async function readEventStream(
    body: ReadableStream<Uint8Array>,
    signal: AbortSignal,
    onPayload: (payload: string) => void,
): Promise<boolean> {
    const reader = body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';
    let sawChunk = false;
    const abort = () => {
        reader.cancel().catch(() => {});
    };
    signal.addEventListener('abort', abort);
    try {
        for (;;) {
            const { done, value } = await reader.read();
            if (done) return sawChunk;
            sawChunk = true;
            buffer += decoder.decode(value, { stream: true }).replace(/\r\n/g, '\n').replace(/\r/g, '\n');
            if (buffer.length > MAX_SSE_BUFFER) buffer = buffer.slice(-MAX_SSE_BUFFER);
            const taken = takeSseData(buffer);
            buffer = taken.rest;
            for (const payload of taken.data) onPayload(payload);
        }
    } finally {
        signal.removeEventListener('abort', abort);
        try {
            reader.releaseLock();
        } catch {
            // cancel() already released the reader.
        }
    }
}

function initialStatus(): LiveStatus {
    if (typeof document === 'undefined') return 'off';
    if (document.visibilityState === 'hidden') return 'off';
    if (!safeLocalStorage.getItem('token')) return 'off';
    return 'reconnecting';
}

/**
 * One live click stream for the Dashboard tab.
 *
 * EventSource and the browser WebSocket API cannot send `Authorization`.
 * Both `/sse` and `/ws` require a credential before the stream starts, and
 * `/ws` does not authenticate a first message (client text is ignored). `/sse`
 * accepts the same Bearer header as the rest of the API, so this uses `fetch`
 * and reads the event stream. The session JWT stays out of the URL.
 *
 * The tab holds the connection only while it is visible. Hiding it closes the
 * stream, which frees a slot in `WS_MAX_CONNECTIONS_PER_USER` (default 5).
 * A 429, a dropped stream or a network error waits with exponential backoff
 * capped at 60s. A 401 stops: `authFetch` has already ended the session.
 *
 * Events for links outside `linkIds` are ignored. Each event counts as one
 * click: an uncapped link's `click_count` is the stored total plus one at
 * request time, so two clicks before the buffer flushes can carry the same
 * number.
 */
export function useLiveClicks(linkIds: readonly number[], onClick: (linkId: number) => void): LiveClicks {
    const [status, setStatus] = useState<LiveStatus>(initialStatus);
    const [flash, setFlash] = useState<{ id: number; nonce: number } | null>(null);
    const idsRef = useRef(new Set(linkIds));
    const onClickRef = useRef(onClick);

    // The stream stays up while the page of ids changes. Layout, not a passive
    // effect: the new rows are committed before the next click can be read.
    useLayoutEffect(() => {
        idsRef.current = new Set(linkIds);
        onClickRef.current = onClick;
    }, [linkIds, onClick]);

    useEffect(() => {
        let cancelled = false;
        let sessionOver = false;
        let attempt = 0;
        let timer: ReturnType<typeof setTimeout> | undefined;
        let flashTimer: ReturnType<typeof setTimeout> | undefined;
        let current: AbortController | null = null;

        const clearRetry = () => {
            if (timer !== undefined) {
                clearTimeout(timer);
                timer = undefined;
            }
        };

        const stillMine = (mine: AbortController) =>
            !cancelled && !sessionOver && !mine.signal.aborted && current === mine;

        const schedule = (minimumMs: number, mine: AbortController) => {
            if (!stillMine(mine)) return;
            const delay = Math.min(BACKOFF_CAP_MS, Math.max(liveBackoffMs(attempt), minimumMs));
            attempt += 1;
            setStatus('reconnecting');
            timer = setTimeout(() => {
                timer = undefined;
                void connect();
            }, delay);
        };

        const note = (id: number) => {
            if (cancelled || !idsRef.current.has(id)) return;
            onClickRef.current(id);
            setFlash((prev) => ({ id, nonce: (prev?.nonce ?? 0) + 1 }));
            if (flashTimer !== undefined) clearTimeout(flashTimer);
            flashTimer = setTimeout(() => {
                if (!cancelled) setFlash(null);
            }, FLASH_MS);
        };

        async function connect() {
            clearRetry();
            if (cancelled || sessionOver) return;
            if (document.visibilityState === 'hidden' || !safeLocalStorage.getItem('token')) {
                setStatus('off');
                return;
            }
            current?.abort();
            const mine = new AbortController();
            current = mine;
            setStatus('reconnecting');
            try {
                const response = await authFetch(API_ENDPOINTS.sse, {
                    signal: mine.signal,
                    cache: 'no-store',
                    headers: { Accept: 'text/event-stream' },
                });
                if (!stillMine(mine)) return;
                if (response.status === 429 || !response.ok || !response.body || !isEventStream(response)) {
                    response.body?.cancel().catch(() => {});
                    schedule(response.status === 429 ? retryAfterMs(response) : 0, mine);
                    return;
                }
                setStatus('connected');
                const sawChunk = await readEventStream(response.body, mine.signal, (payload) => {
                    const id = linkIdFromPayload(payload);
                    if (id !== null) note(id);
                });
                if (!stillMine(mine)) return;
                // A stream that carried something was healthy. The next drop
                // starts its backoff from the beginning. An empty close did not.
                if (sawChunk) attempt = 0;
                schedule(0, mine);
            } catch (error) {
                if (!stillMine(mine) || isAbortError(error)) return;
                if (error instanceof SessionExpiredError) {
                    sessionOver = true;
                    clearRetry();
                    current?.abort();
                    setStatus('off');
                    return;
                }
                schedule(0, mine);
            }
        }

        const onVisibility = () => {
            if (sessionOver || cancelled) return;
            if (document.visibilityState === 'hidden') {
                clearRetry();
                current?.abort();
                current = null;
                attempt = 0;
                setStatus('off');
                return;
            }
            // Visible again with a stream open, or a retry already waiting its
            // backoff: nothing to do. Only a hide leaves `current` empty.
            if (current !== null) return;
            void connect();
        };

        document.addEventListener('visibilitychange', onVisibility);
        void connect();

        return () => {
            cancelled = true;
            clearRetry();
            if (flashTimer !== undefined) clearTimeout(flashTimer);
            current?.abort();
            document.removeEventListener('visibilitychange', onVisibility);
        };
    }, []);

    const flashNonce = (linkId: number) => (flash?.id === linkId ? flash.nonce : 0);
    return { status, flashNonce };
}
