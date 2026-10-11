/** The hosted instance, used when VITE_FRONTEND_URL is not set. */
export const DEFAULT_SITE_URL = 'https://opn.onl';

/**
 * The site's public origin without a trailing slash, from VITE_FRONTEND_URL.
 * Canonical URLs (SEO.tsx) and the sitemap the build generates both take the
 * host from here, so a page's canonical can only ever equal its sitemap entry.
 */
export function normalizeSiteUrl(configured: string | undefined): string {
    return (configured || DEFAULT_SITE_URL).replace(/\/+$/, '');
}
