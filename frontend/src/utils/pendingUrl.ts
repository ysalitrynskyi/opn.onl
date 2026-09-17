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

/** Homepage shorten while logged out. sessionStorage survives the auth
 *  flow's full-page reloads; router location state does not. */
export function savePendingUrl(url: string): void {
    const trimmed = url.trim();
    if (!isUsablePendingUrl(trimmed)) return;
    sessionStorage.setItem(PENDING_URL_KEY, trimmed);
}

/** Read-and-forget. Invalid values are dropped so they cannot resurface
 *  later in the tab, and the dashboard never shows an error for them. */
export function takePendingUrl(): string | null {
    const raw = sessionStorage.getItem(PENDING_URL_KEY);
    sessionStorage.removeItem(PENDING_URL_KEY);
    if (!raw) return null;
    const trimmed = raw.trim();
    return isUsablePendingUrl(trimmed) ? trimmed : null;
}
