import { describe, expect, it } from 'vitest';
import { buildSitemap, sitemapPages, SITEMAP_HINTS, withSitemapDirective } from './sitemap';
import type { SitemapPage } from './sitemap';

const pages: SitemapPage[] = [
    { path: '/', lastmod: '2026-06-16', changefreq: 'weekly', priority: 1 },
    { path: '/features', lastmod: '2025-12-07', changefreq: 'monthly', priority: 0.8 },
];

const locs = (xml: string) => [...xml.matchAll(/<loc>([^<]+)<\/loc>/g)].map((m) => m[1]);

describe('buildSitemap', () => {
    it('lists each page with its lastmod, changefreq and priority', () => {
        expect(buildSitemap('https://opn.onl', pages)).toBe(
            [
                '<?xml version="1.0" encoding="UTF-8"?>',
                '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">',
                '  <url>',
                '    <loc>https://opn.onl/</loc>',
                '    <lastmod>2026-06-16</lastmod>',
                '    <changefreq>weekly</changefreq>',
                '    <priority>1.0</priority>',
                '  </url>',
                '  <url>',
                '    <loc>https://opn.onl/features</loc>',
                '    <lastmod>2025-12-07</lastmod>',
                '    <changefreq>monthly</changefreq>',
                '    <priority>0.8</priority>',
                '  </url>',
                '</urlset>',
                '',
            ].join('\n'),
        );
    });

    it('takes every loc from the site it is given', () => {
        expect(locs(buildSitemap('https://s.example.com', pages))).toEqual([
            'https://s.example.com/',
            'https://s.example.com/features',
        ]);
        expect(locs(buildSitemap('http://localhost:5173', pages))).toEqual([
            'http://localhost:5173/',
            'http://localhost:5173/features',
        ]);
    });

    it('escapes characters that are special in XML', () => {
        const xml = buildSitemap('https://example.com', [{ ...pages[0], path: '/a&b' }]);
        expect(xml).toContain('<loc>https://example.com/a&amp;b</loc>');
    });

    it.each(['opn.onl', 'ftp://opn.onl', 'https://opn.onl/app', 'https://', ''])(
        'refuses %j as a site URL',
        (siteUrl) => {
            expect(() => buildSitemap(siteUrl, pages)).toThrow(/VITE_FRONTEND_URL/);
        },
    );
});

describe('sitemapPages', () => {
    it('pairs every route with its hints, in route order', () => {
        const hints = {
            '/': { lastmod: '2026-06-16', changefreq: 'weekly', priority: 1 },
            '/features': { lastmod: '2025-12-07', changefreq: 'monthly', priority: 0.8 },
        } as const;
        expect(sitemapPages(['/features', '/'], hints)).toEqual([
            { path: '/features', ...hints['/features'] },
            { path: '/', ...hints['/'] },
        ]);
    });

    it('refuses a route that has no hints', () => {
        expect(() => sitemapPages(['/', '/changelog'], { '/': SITEMAP_HINTS['/'] })).toThrow(
            /no hints for: \/changelog/,
        );
    });

    it('refuses hints for a route that is not prerendered', () => {
        expect(() => sitemapPages(['/'], { '/': SITEMAP_HINTS['/'], '/gone': SITEMAP_HINTS['/'] })).toThrow(
            /not prerendered: \/gone/,
        );
    });

    it('has hints for the home page, which is the only route whose loc ends in a slash', () => {
        expect(Object.keys(SITEMAP_HINTS)).toContain('/');
    });
});

describe('withSitemapDirective', () => {
    const robots = 'User-agent: *\nAllow: /\n\nDisallow: /dashboard\n';

    it('appends a Sitemap line for the site', () => {
        expect(withSitemapDirective(robots, 'https://s.example.com')).toBe(
            'User-agent: *\nAllow: /\n\nDisallow: /dashboard\n\n# Sitemap\nSitemap: https://s.example.com/sitemap.xml\n',
        );
    });

    it('replaces an existing Sitemap line instead of adding a second one', () => {
        const out = withSitemapDirective(`${robots}\nSitemap: https://opn.onl/sitemap.xml\n\n\n`, 'https://s.example.com');
        expect(out.match(/^sitemap:/gim)).toHaveLength(1);
        expect(out).toContain('Sitemap: https://s.example.com/sitemap.xml');
        expect(out).not.toContain('opn.onl');
        expect(out).toContain('Disallow: /dashboard');
    });

    it('writes a Sitemap line even when there was no robots.txt', () => {
        expect(withSitemapDirective('', 'https://s.example.com')).toBe(
            '# Sitemap\nSitemap: https://s.example.com/sitemap.xml\n',
        );
    });
});
