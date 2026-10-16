import { pluralize } from '../../utils/plural';
import type { CheckCodeResponse, UrlHealthResponse } from '../../config/api';

/** Matches `check_link_text` in the links handler. */
export const MAX_TITLE_CHARS = 500;
export const MAX_NOTES_CHARS = 5000;

/** Counter appears once this many characters remain, so a short note stays quiet. */
export const NEAR_LIMIT_CHARS = 500;

/** Postgres `integer`. A larger cap is rejected by the API. */
const MAX_CLICKS = 2_147_483_647;

export interface AliasHint {
    text: string;
    ok: boolean;
}

const TAKEN_MESSAGE = 'This alias is already taken';

/** Live check is a hint. "Available" / "Taken" are the two answers; anything else is the server's own wording. */
export function aliasHintFrom(body: Pick<CheckCodeResponse, 'available' | 'message'>): AliasHint {
    if (body.available) return { text: 'Available', ok: true };
    if (body.message === TAKEN_MESSAGE) return { text: 'Taken', ok: false };
    return { text: body.message || 'Taken', ok: false };
}

export function parseClickLimit(raw: string): { ok: true; value: number | null } | { ok: false } {
    const trimmed = raw.trim();
    if (!trimmed) return { ok: true, value: null };
    if (!/^[1-9]\d*$/.test(trimmed)) return { ok: false };
    const value = Number(trimmed);
    if (!Number.isSafeInteger(value) || value <= 0 || value > MAX_CLICKS) return { ok: false };
    return { ok: true, value };
}

export const CLICK_LIMIT_ERROR = 'Click limit must be a positive whole number';
export const START_BEFORE_EXPIRY = 'Start date must be before the expiration';
export const CLICK_LIMIT_HINT = 'Link stops working after this many clicks';

/**
 * Cap a burn link keeps when the request does not name one: the cap it already
 * has, otherwise one click. Sending a second number would override that.
 */
export function burnCap(stored: number | null | undefined): number {
    return stored != null && stored > 0 ? stored : 1;
}

export function burnCapHint(cap: number): string {
    return `Burn after reading stops the link after ${cap.toLocaleString()} ${pluralize(cap, 'click')}.`;
}

export function initialClickLimit(value: number | null | undefined): string {
    if (value == null || value <= 0) return '';
    return String(value);
}

export type ClickLimitUpdate =
    | { error: string }
    | { max_clicks?: number; remove_max_clicks?: true };

/**
 * Omit the cap unless it changed. An empty field keeps the stored cap (the
 * remove checkbox is what clears it). While burn is on, omit both: echoing a
 * cap or a remove next to `burn_after_reading: true` does not do what it says.
 */
export function clickLimitUpdate(args: {
    raw: string;
    stored: number | null;
    remove: boolean;
    burnOn: boolean;
}): ClickLimitUpdate {
    if (args.burnOn) return {};
    if (args.remove) return { remove_max_clicks: true };
    const parsed = parseClickLimit(args.raw);
    if (!parsed.ok) return { error: CLICK_LIMIT_ERROR };
    if (parsed.value == null || parsed.value === args.stored) return {};
    return { max_clicks: parsed.value };
}

/** `YYYY-MM-DDTHH:mm` in the viewer's zone, for `<input type="datetime-local">`. */
export function datetimeLocalValue(iso: string | null | undefined): string {
    if (!iso) return '';
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return '';
    const pad = (n: number) => String(n).padStart(2, '0');
    return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** RFC 3339 UTC, the same conversion the expiry fields use (`Date#toISOString`). */
export function datetimeLocalToUtc(value: string): string | null {
    if (!value) return null;
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return null;
    return date.toISOString();
}

export interface DestinationStatus {
    ok: boolean;
    text: string;
}

/**
 * Health-check answers with the same body on success and on a refused URL.
 * Redirects are followed by the server, so a 3xx here is one it handed back
 * rather than a target we can name.
 */
export function statusFromHealth(
    httpOk: boolean,
    data: Partial<UrlHealthResponse> | { error?: string | null } | null,
): DestinationStatus {
    if (!data) return { ok: false, text: 'Could not check this destination' };
    const body = data as Partial<UrlHealthResponse>;
    if (body.reachable) {
        const code = body.status_code;
        if (code != null && code >= 300 && code < 400) return { ok: true, text: `Redirect · HTTP ${code}` };
        if (code != null) return { ok: true, text: `Reachable · HTTP ${code}` };
        return { ok: true, text: 'Reachable' };
    }
    if (body.error) return { ok: false, text: body.error };
    if (!httpOk) return { ok: false, text: 'Could not check this destination' };
    return { ok: false, text: 'Unreachable' };
}
