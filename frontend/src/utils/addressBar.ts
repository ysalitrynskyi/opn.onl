/**
 * Replace the current history entry's URL with its path alone, dropping the
 * query string and fragment. Used after reading a one-time secret (a password
 * reset or email verification token) out of the query.
 *
 * history.state is passed through unchanged: React Router keeps its entry key
 * and index there, and back/forward navigation depends on them.
 */
export function stripQueryFromAddressBar(): void {
    const { pathname, search, hash } = window.location;
    if (!search && !hash) return;
    window.history.replaceState(window.history.state, '', pathname);
}
