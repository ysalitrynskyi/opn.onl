/**
 * Web Storage access that never throws.
 *
 * When a browser blocks site data for this origin (cookies and site data turned
 * off, a privacy extension, a sandboxed frame), merely reading
 * `window.localStorage` or `window.sessionStorage` throws a SecurityError, and a
 * write can also throw when the store is full. Reached during render, that one
 * exception takes the whole app down to the error boundary, public pages
 * included. Every storage access in the app goes through these wrappers instead:
 *
 * - `getItem` returns null when storage is unreachable, so a blocked token read
 *   means "signed out" rather than a crash;
 * - `setItem` reports whether the value was actually stored;
 * - `removeItem` does nothing when there is no storage to remove from.
 *
 * With storage available they behave exactly like the Storage methods they wrap.
 */

type StorageArea = 'localStorage' | 'sessionStorage';

function guarded(area: StorageArea) {
    return {
        getItem(key: string): string | null {
            try {
                return window[area].getItem(key);
            } catch {
                return null;
            }
        },

        setItem(key: string, value: string): boolean {
            try {
                window[area].setItem(key, value);
                return true;
            } catch {
                return false;
            }
        },

        removeItem(key: string): void {
            try {
                window[area].removeItem(key);
            } catch {
                /* storage unreachable: nothing was kept, so nothing to remove */
            }
        },
    };
}

export const safeLocalStorage = guarded('localStorage');
export const safeSessionStorage = guarded('sessionStorage');

/**
 * Shown when a sign-in succeeded on the server but the session token could not
 * be stored. Without it every later page treats the visitor as signed out, so
 * say why now instead of silently landing them back on the login page.
 */
export const SIGN_IN_NEEDS_STORAGE =
    "Your browser is blocking storage for this site, so you can't stay signed in. " +
    'Allow cookies and site data for this site, then log in.';
