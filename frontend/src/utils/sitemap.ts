// sitemap.xml and the Sitemap line of robots.txt are generated at build time
// (see vite.config.ts) from VITE_FRONTEND_URL and PRERENDER_ROUTES. A copy
// committed under public/ pinned every build to one domain: a self-hosted
// instance either advertised opn.onl in its sitemap or failed verify-prerender.

export type ChangeFreq = 'always' | 'hourly' | 'daily' | 'weekly' | 'monthly' | 'yearly' | 'never';

export interface SitemapPage {
    /** Route as listed in PRERENDER_ROUTES, e.g. "/features". */
    path: string;
    /** YYYY-MM-DD */
    lastmod: string;
    changefreq: ChangeFreq;
    priority: number;
}

// One entry per prerendered route. lastmod is maintained by hand: bump it when
// the page's content changes.
export const SITEMAP_HINTS: Record<string, Omit<SitemapPage, 'path'>> = {
    '/': { lastmod: '2026-06-16', changefreq: 'weekly', priority: 1.0 },
    '/features': { lastmod: '2026-10-16', changefreq: 'monthly', priority: 0.8 },
    '/pricing': { lastmod: '2026-10-16', changefreq: 'monthly', priority: 0.8 },
    '/about': { lastmod: '2025-12-07', changefreq: 'monthly', priority: 0.6 },
    '/privacy': { lastmod: '2026-10-16', changefreq: 'yearly', priority: 0.3 },
    '/terms': { lastmod: '2026-10-16', changefreq: 'yearly', priority: 0.3 },
    '/contact': { lastmod: '2025-12-07', changefreq: 'monthly', priority: 0.5 },
    '/faq': { lastmod: '2026-10-16', changefreq: 'monthly', priority: 0.6 },
    '/docs': { lastmod: '2026-10-16', changefreq: 'monthly', priority: 0.7 },
    '/developers': { lastmod: '2026-06-16', changefreq: 'monthly', priority: 0.8 },
    '/login': { lastmod: '2025-12-07', changefreq: 'yearly', priority: 0.5 },
    '/register': { lastmod: '2025-12-07', changefreq: 'yearly', priority: 0.5 },
};

/**
 * Pair every prerendered route with its sitemap hints, in route order. The two
 * lists must name the same pages: a route without hints would ship a sitemap
 * entry nobody chose, a hint without a route is a page that no longer exists.
 */
export function sitemapPages(
    routes: readonly string[],
    hints: Record<string, Omit<SitemapPage, 'path'>> = SITEMAP_HINTS,
): SitemapPage[] {
    const missing = routes.filter((route) => !(route in hints));
    const unknown = Object.keys(hints).filter((path) => !routes.includes(path));
    if (missing.length > 0 || unknown.length > 0) {
        throw new Error(
            'sitemap hints (src/utils/sitemap.ts) and PRERENDER_ROUTES (vite.config.ts) disagree' +
                (missing.length > 0 ? `; no hints for: ${missing.join(', ')}` : '') +
                (unknown.length > 0 ? `; hints for routes that are not prerendered: ${unknown.join(', ')}` : ''),
        );
    }
    return routes.map((path) => ({ path, ...hints[path] }));
}

function assertSiteOrigin(siteUrl: string): void {
    if (!/^https?:\/\/[^/?#\s]+$/.test(siteUrl)) {
        throw new Error(
            `VITE_FRONTEND_URL must be the site's origin, such as https://opn.onl ` +
                `(an absolute http(s) URL with no path), got "${siteUrl}"`,
        );
    }
}

function escapeXml(text: string): string {
    return text
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&apos;');
}

/** The sitemap.xml document for `siteUrl` (see normalizeSiteUrl): one url per page. */
export function buildSitemap(siteUrl: string, pages: readonly SitemapPage[]): string {
    assertSiteOrigin(siteUrl);
    const urls = pages.map(
        (page) =>
            '  <url>\n' +
            `    <loc>${escapeXml(`${siteUrl}${page.path}`)}</loc>\n` +
            `    <lastmod>${page.lastmod}</lastmod>\n` +
            `    <changefreq>${page.changefreq}</changefreq>\n` +
            `    <priority>${page.priority.toFixed(1)}</priority>\n` +
            '  </url>\n',
    );
    return (
        '<?xml version="1.0" encoding="UTF-8"?>\n' +
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' +
        urls.join('') +
        '</urlset>\n'
    );
}

/** `robots` with its Sitemap line replaced by one that points at `siteUrl`. */
export function withSitemapDirective(robots: string, siteUrl: string): string {
    assertSiteOrigin(siteUrl);
    const rules = robots.replace(/^[ \t]*sitemap:.*(?:\r?\n|$)/gim, '').trimEnd();
    return `${rules ? `${rules}\n\n` : ''}# Sitemap\nSitemap: ${siteUrl}/sitemap.xml\n`;
}
