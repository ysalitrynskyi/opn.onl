import { describe, expect, it } from 'vitest';
import { DEFAULT_SITE_URL, normalizeSiteUrl } from './siteUrl';

describe('normalizeSiteUrl', () => {
    it('falls back to the hosted instance when nothing is configured', () => {
        expect(normalizeSiteUrl(undefined)).toBe(DEFAULT_SITE_URL);
        expect(normalizeSiteUrl('')).toBe(DEFAULT_SITE_URL);
    });

    it('keeps a configured origin and drops trailing slashes', () => {
        expect(normalizeSiteUrl('https://s.example.com')).toBe('https://s.example.com');
        expect(normalizeSiteUrl('https://s.example.com/')).toBe('https://s.example.com');
        expect(normalizeSiteUrl('http://localhost:5173//')).toBe('http://localhost:5173');
    });
});
