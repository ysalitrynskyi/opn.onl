const PENDING_URL_KEY = 'opn.pendingUrl';
const MAX_PENDING_URL_LENGTH = 2048;

function isUsablePendingUrl(url: string): boolean {
    if (!url || url.length > MAX_PENDING_URL_LENGTH) return false;
    try {
        const parsed = new URL(url);
        return parsed.protocol === 'http:' || parsed.protocol === 'https:';
    } catch {
        return false;
    }
}

// Strict Mode remounts with fresh state and re-runs the dashboard effect.
// Keep the last take in memory until the next microtask so the second
// effect still sees it after sessionStorage is cleared.
let leftover: string | null = null;

/** Homepage shorten while logged out. sessionStorage survives the auth
 *  flow's full-page reloads; router location state does not. */
export function savePendingUrl(url: string): void {
    const trimmed = url.trim();
    if (!isUsablePendingUrl(trimmed)) return;
    leftover = null;
    sessionStorage.setItem(PENDING_URL_KEY, trimmed);
}

/** Read-and-forget. Invalid values are dropped so they cannot resurface
 *  later in the tab, and the dashboard never shows an error for them. */
export function takePendingUrl(): string | null {
    const raw = sessionStorage.getItem(PENDING_URL_KEY);
    if (raw !== null) {
        sessionStorage.removeItem(PENDING_URL_KEY);
        const trimmed = raw.trim();
        leftover = isUsablePendingUrl(trimmed) ? trimmed : null;
        queueMicrotask(() => {
            leftover = null;
        });
        return leftover;
    }
    return leftover;
}
