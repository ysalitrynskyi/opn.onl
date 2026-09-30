import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, render } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { HelmetProvider } from 'react-helmet-async';
import SEO from './SEO';
import App from '../App';
import { dropPrerenderedHeadTags, SEO_TAG_ATTRIBUTE } from '../utils/prerenderedHead';

// React 19 hoists <title>, <meta> and <link> into document.head, so these tests
// read the real head the browser would get.

const head = () => document.head;
const all = (selector: string) => [...head().querySelectorAll(selector)];
const canonicals = () => all('link[rel="canonical"]').map((l) => l.getAttribute('href'));
const robots = () => all('meta[name="robots"]').map((m) => m.getAttribute('content'));
const jsonLdTypes = () =>
    [...document.querySelectorAll('script[type="application/ld+json"]')].map(
        (s) => JSON.parse(s.textContent || '{}')['@type'] as string,
    );

function renderSeo(ui: React.ReactElement) {
    return render(<HelmetProvider>{ui}</HelmetProvider>);
}

afterEach(() => {
    // Unmount first: React removes the head tags it owns, and expects them to
    // still be attached when it does.
    cleanup();
    head().innerHTML = '';
});

describe('SEO head tags', () => {
    it('gives an indexable page one description and one canonical, and no robots tag', () => {
        renderSeo(<SEO title="Pricing" description="Free." url="/pricing" />);

        expect(document.title).toBe('Pricing | opn.onl');
        expect(all('meta[name="description"]')).toHaveLength(1);
        expect(canonicals()).toEqual(['https://opn.onl/pricing']);
        expect(robots()).toEqual([]);
    });

    it('canonicalises the home page with the trailing slash its sitemap entry has', () => {
        renderSeo(<SEO />);

        expect(canonicals()).toEqual(['https://opn.onl/']);
        expect(all('meta[property="og:url"]').map((m) => m.getAttribute('content'))).toEqual(['https://opn.onl/']);
    });

    it('gives a noindex page noindex and no canonical pointing elsewhere', () => {
        renderSeo(<SEO title="Dashboard" noIndex />);

        expect(robots()).toEqual(['noindex, nofollow']);
        expect(canonicals()).toEqual([]);
        expect(all('meta[property="og:url"]')).toHaveLength(0);
    });

    it('lets a noindex page that names its own URL canonicalise to itself', () => {
        renderSeo(<SEO title="Alice" url="/@alice" noIndex />);

        expect(robots()).toEqual(['noindex, nofollow']);
        expect(canonicals()).toEqual(['https://opn.onl/@alice']);
    });

    it('emits a single FAQPage block that carries the questions', () => {
        renderSeo(
            <SEO
                title="FAQ"
                url="/faq"
                schemaType="FAQPage"
                faqItems={[
                    { question: 'Is it free?', answer: 'Yes.' },
                    { question: 'Can I self-host?', answer: 'Yes.' },
                ]}
            />,
        );

        const faqBlocks = [...document.querySelectorAll('script[type="application/ld+json"]')]
            .map((s) => JSON.parse(s.textContent || '{}'))
            .filter((schema) => schema['@type'] === 'FAQPage');
        expect(faqBlocks).toHaveLength(1);
        expect(faqBlocks[0].mainEntity.map((q: { name: string }) => q.name)).toEqual([
            'Is it free?',
            'Can I self-host?',
        ]);
    });

    it('marks every head tag it renders so a prerendered copy can be dropped', () => {
        renderSeo(<SEO title="Pricing" url="/pricing" />);

        const tags = all('title, meta, link');
        expect(tags.length).toBeGreaterThan(10);
        for (const tag of tags) {
            expect(tag.hasAttribute(SEO_TAG_ATTRIBUTE)).toBe(true);
        }
    });
});

describe('prerendered head on an app route', () => {
    // nginx serves the prerendered home page as the SPA shell for app routes, so
    // /dashboard arrives with the home page's head tags already in it.
    function loadPrerenderedHome() {
        head().innerHTML = [
            `<title ${SEO_TAG_ATTRIBUTE}="">opn.onl - Open Source URL Shortener</title>`,
            '<meta name="theme-color" content="#2f37d8">',
            `<meta ${SEO_TAG_ATTRIBUTE}="" name="description" content="Home page description">`,
            `<link ${SEO_TAG_ATTRIBUTE}="" rel="canonical" href="https://opn.onl/">`,
            `<meta ${SEO_TAG_ATTRIBUTE}="" property="og:url" content="https://opn.onl/">`,
        ].join('');
    }

    it('keeps only the tags of the page that is open', () => {
        loadPrerenderedHome();

        dropPrerenderedHeadTags();
        renderSeo(<SEO title="Dashboard" noIndex />);

        expect(document.title).toBe('Dashboard | opn.onl');
        expect(all('title')).toHaveLength(1);
        expect(all('meta[name="description"]')).toHaveLength(1);
        expect(canonicals()).toEqual([]);
        expect(all('meta[property="og:url"]')).toHaveLength(0);
        expect(robots()).toEqual(['noindex, nofollow']);
        // Tags index.html itself ships are not the SEO component's to remove.
        expect(all('meta[name="theme-color"]')).toHaveLength(1);
    });
});

describe('index.html', () => {
    // Every prerendered page is index.html plus the tags SEO renders, so any
    // page-level tag here would be a second, contradicting copy on every page.
    it('carries no page-level SEO tags of its own', () => {
        const html = readFileSync(resolve(process.cwd(), 'index.html'), 'utf8');
        const headHtml = html.slice(0, html.indexOf('</head>'));
        for (const pattern of [
            /<title\b/,
            /<meta\s+name="(description|keywords|author|robots)"/,
            /<meta\s+property="og:/,
            /<meta\s+name="twitter:/,
            /<link\s+rel="canonical"/,
        ]) {
            expect(headHtml).not.toMatch(pattern);
        }
    });
});

describe('prerendered routes', () => {
    const sitemap = readFileSync(resolve(process.cwd(), 'public/sitemap.xml'), 'utf8');
    const sitemapUrls = [...sitemap.matchAll(/<loc>([^<]+)<\/loc>/g)].map((m) => m[1]);
    const config = readFileSync(resolve(process.cwd(), 'vite.config.ts'), 'utf8');
    const prerenderRoutes = [
        ...config.match(/const PRERENDER_ROUTES\s*=\s*\[([\s\S]*?)\]/)![1].matchAll(/['"]([^'"]+)['"]/g),
    ].map((m) => m[1]);

    it.each(prerenderRoutes)('%s has one canonical, equal to its sitemap entry', (route) => {
        render(
            <MemoryRouter initialEntries={[route]}>
                <App />
            </MemoryRouter>,
        );

        const expected = sitemapUrls.find((u) => new URL(u).pathname === route);
        expect(expected).toBeDefined();
        expect(canonicals()).toEqual([expected]);
        expect(all('meta[name="description"]')).toHaveLength(1);
        expect(robots()).toEqual([]);
        expect(jsonLdTypes().filter((t) => t === 'FAQPage').length).toBeLessThanOrEqual(1);
    });

    it('gives /login and /register titles of their own', () => {
        const titles = ['/login', '/register', '/'].map((route) => {
            const { unmount } = render(
                <MemoryRouter initialEntries={[route]}>
                    <App />
                </MemoryRouter>,
            );
            const title = document.title;
            unmount();
            return title;
        });

        expect(new Set(titles).size).toBe(3);
        expect(titles[0]).toBe('Log in | opn.onl');
        expect(titles[1]).toBe('Sign up | opn.onl');
    });
});
