import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

type AnalyticsHit = {
    page_path: string;
    page_location: string;
    page_referrer?: string;
    anonymize_ip: boolean;
};

describe('analytics hit', () => {
    it('keeps consent mode and strips query tokens from the hit', () => {
        // jsdom serves this module over http, so import.meta.url is not a file URL.
        const html = readFileSync(resolve(process.cwd(), 'index.html'), 'utf8');
        expect(html).toContain("window.GA_CONSENT_MODE === 'opt-out'");
        expect(html).toContain("stored === 'granted'");
        expect(html).toContain("gtag('config', gaId, analyticsHit());");

        const start = html.indexOf('/* analytics-hit:start */');
        const end = html.indexOf('/* analytics-hit:end */');
        expect(start).toBeGreaterThan(-1);
        expect(end).toBeGreaterThan(start);
        const source = html.slice(start, end);
        const analyticsHit = new Function(`${source}; return analyticsHit;`)() as () => AnalyticsHit;

        window.history.pushState({}, '', '/reset-password?token=SECRET#frag');
        let referrerSet = false;
        try {
            Object.defineProperty(document, 'referrer', {
                configurable: true,
                get: () => 'https://evil.example/reset-password?token=SECRET#frag',
            });
            referrerSet = true;
        } catch {
            referrerSet = false;
        }

        const hit = analyticsHit();
        expect(hit.anonymize_ip).toBe(true);
        expect(hit.page_path).toBe('/reset-password');
        expect(hit.page_location).toBe(`${window.location.origin}/reset-password`);
        expect(hit.page_location).not.toContain('SECRET');
        expect(hit.page_location).not.toContain('?');
        if (referrerSet) {
            expect(hit.page_referrer).toBe('https://evil.example/reset-password');
            expect(hit.page_referrer).not.toContain('SECRET');
        }
    });
});
