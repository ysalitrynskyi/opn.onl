import { describe, expect, it } from 'vitest';
import { waitFor } from '@testing-library/react';

// The entry point, loaded the way the browser loads a prerendered page: the
// HTML already holds the head tags of the route it was rendered for.
describe('main.tsx on a prerendered page', () => {
    it('leaves one set of head tags, for the route that is open', async () => {
        // An app-route request is served the prerendered home page.
        document.head.innerHTML = [
            '<meta name="theme-color" content="#2f37d8">',
            '<title data-seo="">opn.onl - Open Source URL Shortener</title>',
            '<meta data-seo="" name="description" content="Home page description">',
            '<link data-seo="" rel="canonical" href="https://opn.onl/">',
        ].join('');
        document.body.innerHTML = '<div id="root" data-prerendered="true"><h1>Prerendered home</h1></div>';
        window.history.pushState({}, '', '/pricing');

        await import('./main');

        await waitFor(() => expect(document.title).toBe('Pricing | opn.onl'));
        const head = document.head;
        expect(head.querySelectorAll('title')).toHaveLength(1);
        expect(head.querySelectorAll('meta[name="description"]')).toHaveLength(1);
        expect([...head.querySelectorAll('link[rel="canonical"]')].map((l) => l.getAttribute('href'))).toEqual([
            'https://opn.onl/pricing',
        ]);
        expect(head.querySelectorAll('meta[name="theme-color"]')).toHaveLength(1);
    });
});
