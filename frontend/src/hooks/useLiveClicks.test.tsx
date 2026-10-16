import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { API_ENDPOINTS } from '../config/api';
import { liveBackoffMs, takeSseData, useLiveClicks } from './useLiveClicks';

const TOKEN = 'live-token';

type Push = (text: string) => void;

function sseResponse(signal?: AbortSignal): { response: Response; push: Push } {
    const encoder = new TextEncoder();
    let controller!: ReadableStreamDefaultController<Uint8Array>;
    const stream = new ReadableStream<Uint8Array>({
        start(control) {
            controller = control;
            signal?.addEventListener('abort', () => {
                try {
                    controller.close();
                } catch {
                    // Already closed.
                }
            });
        },
    });
    return {
        response: new Response(stream, {
            status: 200,
            headers: { 'Content-Type': 'text/event-stream' },
        }),
        push: (text) => controller.enqueue(encoder.encode(text)),
    };
}

function event(linkId: number, clickCount = 1): string {
    return `data: ${JSON.stringify({ link_id: linkId, click_count: clickCount })}\n\n`;
}

function setVisibility(state: DocumentVisibilityState) {
    Object.defineProperty(document, 'visibilityState', {
        configurable: true,
        get: () => state,
    });
    document.dispatchEvent(new Event('visibilitychange'));
}

describe('live click backoff', () => {
    it('waits half to all of an exponential delay and never more than 60s', () => {
        expect(liveBackoffMs(0, () => 0)).toBe(500);
        expect(liveBackoffMs(0, () => 1)).toBe(1_000);
        expect(liveBackoffMs(1, () => 0)).toBe(1_000);
        expect(liveBackoffMs(2, () => 1)).toBe(4_000);
        expect(liveBackoffMs(6, () => 1)).toBe(60_000);
        expect(liveBackoffMs(20, () => 0)).toBe(30_000);
        expect(liveBackoffMs(20, () => 1)).toBe(60_000);
    });

    it('keeps a partial frame until the delimiter arrives', () => {
        const split = takeSseData('data: {"link_id":');
        expect(split.data).toEqual([]);
        const done = takeSseData(`${split.rest}1}\n\n: ping\n\n`);
        expect(done.data).toEqual(['{"link_id":1}']);
        expect(done.rest).toBe('');
    });
});

describe('useLiveClicks', () => {
    beforeEach(() => {
        vi.mocked(localStorage.getItem).mockImplementation((key) => (key === 'token' ? TOKEN : null));
        setVisibility('visible');
    });

    afterEach(() => {
        vi.useRealTimers();
        vi.mocked(global.fetch).mockReset();
        if (vi.isMockFunction(Math.random)) vi.mocked(Math.random).mockRestore();
        delete (document as { visibilityState?: string }).visibilityState;
    });

    it('counts a click for a link on the page and ignores the rest', async () => {
        const onClick = vi.fn();
        let push: Push = () => {};
        vi.mocked(global.fetch).mockImplementation(async (input, init) => {
            const opened = sseResponse(init?.signal ?? undefined);
            push = opened.push;
            return opened.response;
        });

        const { result } = renderHook(() => useLiveClicks([7], onClick));
        await waitFor(() => expect(result.current.status).toBe('connected'));

        const [url, init] = vi.mocked(global.fetch).mock.calls[0];
        expect(String(url)).toBe(API_ENDPOINTS.sse);
        expect(new URL(String(url)).search).toBe('');
        const headers = new Headers(init?.headers);
        expect(headers.get('Authorization')).toBe(`Bearer ${TOKEN}`);
        expect(init?.method ?? 'GET').toBe('GET');

        act(() => {
            push(': ping\n\n');
            push('data: not-json\n\n');
            push(event(8, 5));
            push('data: {"link_');
            push('id":7,"click_count":1}\n\n');
            push(event(7, 1));
        });
        await waitFor(() => {
            expect(onClick).toHaveBeenCalledTimes(2);
            expect(result.current.flashNonce(7)).toBeGreaterThan(0);
        });
        expect(onClick).toHaveBeenNthCalledWith(1, 7);
        expect(onClick).toHaveBeenNthCalledWith(2, 7);
        expect(result.current.flashNonce(8)).toBe(0);
    });

    it('closes the stream while the tab is hidden and opens it again when shown', async () => {
        const signals: AbortSignal[] = [];
        vi.mocked(global.fetch).mockImplementation(async (_input, init) => {
            if (init?.signal) signals.push(init.signal);
            return sseResponse(init?.signal ?? undefined).response;
        });

        const { result } = renderHook(() => useLiveClicks([1], vi.fn()));
        await waitFor(() => expect(result.current.status).toBe('connected'));
        expect(signals).toHaveLength(1);

        act(() => setVisibility('hidden'));
        await waitFor(() => expect(result.current.status).toBe('off'));
        expect(signals[0].aborted).toBe(true);

        act(() => setVisibility('visible'));
        await waitFor(() => expect(vi.mocked(global.fetch).mock.calls.length).toBe(2));
        await waitFor(() => expect(result.current.status).toBe('connected'));
        expect(signals[1].aborted).toBe(false);
    });

    it('keeps the one stream it has when visibility fires without a hide', async () => {
        vi.mocked(global.fetch).mockImplementation(async (_input, init) => sseResponse(init?.signal ?? undefined).response);

        const { result } = renderHook(() => useLiveClicks([1], vi.fn()));
        await waitFor(() => expect(result.current.status).toBe('connected'));

        act(() => setVisibility('visible'));
        await act(async () => {
            await Promise.resolve();
        });
        expect(global.fetch).toHaveBeenCalledTimes(1);
        expect(result.current.status).toBe('connected');
    });

    it('does not open a stream while the tab is hidden or there is no session', async () => {
        setVisibility('hidden');
        const hidden = renderHook(() => useLiveClicks([1], vi.fn()));
        await act(async () => {
            await Promise.resolve();
        });
        expect(global.fetch).not.toHaveBeenCalled();
        expect(hidden.result.current.status).toBe('off');
        hidden.unmount();

        vi.mocked(localStorage.getItem).mockReturnValue(null);
        setVisibility('visible');
        const signedOut = renderHook(() => useLiveClicks([1], vi.fn()));
        await act(async () => {
            await Promise.resolve();
        });
        expect(global.fetch).not.toHaveBeenCalled();
        expect(signedOut.result.current.status).toBe('off');
    });

    it('stops on 401 and does not try again', async () => {
        vi.useFakeTimers();
        vi.mocked(global.fetch).mockResolvedValue(new Response('unauthorized', { status: 401 }));

        const { result } = renderHook(() => useLiveClicks([1], vi.fn()));
        await act(async () => {
            await vi.advanceTimersByTimeAsync(0);
        });
        expect(result.current.status).toBe('off');
        expect(localStorage.removeItem).toHaveBeenCalledWith('token');
        expect(global.fetch).toHaveBeenCalledTimes(1);

        await act(async () => {
            await vi.advanceTimersByTimeAsync(60_000);
        });
        expect(global.fetch).toHaveBeenCalledTimes(1);
    });

    it('backs off after an error, honours Retry-After, and caps the wait at 60s', async () => {
        vi.spyOn(Math, 'random').mockReturnValue(0);
        vi.useFakeTimers();
        let calls = 0;
        vi.mocked(global.fetch).mockImplementation(async () => {
            calls += 1;
            if (calls === 1) {
                return new Response('busy', { status: 503 });
            }
            if (calls === 2) {
                return new Response('capped', {
                    status: 429,
                    headers: { 'Retry-After': '120' },
                });
            }
            return new Response('still', { status: 503 });
        });

        renderHook(() => useLiveClicks([1], vi.fn()));
        await act(async () => {
            await vi.advanceTimersByTimeAsync(0);
        });
        expect(calls).toBe(1);

        await act(async () => {
            await vi.advanceTimersByTimeAsync(499);
        });
        expect(calls).toBe(1);
        await act(async () => {
            await vi.advanceTimersByTimeAsync(1);
        });
        expect(calls).toBe(2);

        // Retry-After is 120s and the cap is 60s, so the next try is at 60s.
        await act(async () => {
            await vi.advanceTimersByTimeAsync(59_999);
        });
        expect(calls).toBe(2);
        await act(async () => {
            await vi.advanceTimersByTimeAsync(1);
        });
        expect(calls).toBe(3);
    });

    it('reconnects after a stream that carried data ends, starting its backoff over', async () => {
        vi.spyOn(Math, 'random').mockReturnValue(0);
        vi.useFakeTimers();
        const encoder = new TextEncoder();
        let calls = 0;
        vi.mocked(global.fetch).mockImplementation(async (_input, init) => {
            calls += 1;
            // Calls 1 and 2 are healthy streams that the server then closes.
            // Each carried data, so each wait is the first step, never the second.
            if (calls <= 2) {
                return new Response(
                    new ReadableStream<Uint8Array>({
                        start(control) {
                            control.enqueue(encoder.encode(': ping\n\n'));
                            control.close();
                        },
                    }),
                    { status: 200, headers: { 'Content-Type': 'text/event-stream' } },
                );
            }
            return sseResponse(init?.signal ?? undefined).response;
        });

        const { result } = renderHook(() => useLiveClicks([1], vi.fn()));
        await act(async () => {
            await vi.advanceTimersByTimeAsync(0);
        });
        expect(calls).toBe(1);
        expect(result.current.status).toBe('reconnecting');

        await act(async () => {
            await vi.advanceTimersByTimeAsync(500);
        });
        expect(calls).toBe(2);

        await act(async () => {
            await vi.advanceTimersByTimeAsync(500);
        });
        expect(calls).toBe(3);
        expect(result.current.status).toBe('connected');
    });

    it('aborts the stream when the dashboard unmounts', async () => {
        let signal: AbortSignal | undefined;
        vi.mocked(global.fetch).mockImplementation((_input, init) => {
            signal = init?.signal ?? undefined;
            return new Promise(() => {});
        });

        const { unmount } = renderHook(() => useLiveClicks([1], vi.fn()));
        await waitFor(() => expect(signal).toBeDefined());
        unmount();
        expect(signal?.aborted).toBe(true);
    });
});
