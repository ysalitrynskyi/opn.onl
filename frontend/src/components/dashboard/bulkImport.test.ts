import { describe, expect, it } from 'vitest';
import { parseBulkUrls, readBulkErrors } from './bulkImport';

describe('parseBulkUrls', () => {
    it('keeps the trimmed http(s) lines and drops blanks and other text', () => {
        const text = '  https://a.example/1  \n\nnot a url\nftp://b.example\nhttp://c.example/2\n';
        expect(parseBulkUrls(text)).toEqual(['https://a.example/1', 'http://c.example/2']);
    });
});

describe('readBulkErrors', () => {
    const sent = ['https://a.example/1', 'https://b.example/2', 'https://c.example/3', 'https://d.example/4'];

    it('splits both message shapes into the URL and the reason', () => {
        const { failures } = readBulkErrors(sent, [
            'https://b.example/2: Invalid URL',
            'Failed to shorten https://c.example/3: connection reset: by peer',
        ]);

        expect(failures).toEqual([
            { url: 'https://b.example/2', reason: 'Invalid URL' },
            { url: 'https://c.example/3', reason: 'connection reset: by peer' },
        ]);
    });

    it('leaves only the failed URLs, in the order they were sent', () => {
        const { remaining } = readBulkErrors(sent, [
            'https://d.example/4: blocked',
            'https://b.example/2: Invalid URL',
        ]);

        expect(remaining).toEqual(['https://b.example/2', 'https://d.example/4']);
    });

    it('leaves nothing when there are no failures', () => {
        expect(readBulkErrors(sent, [])).toEqual({ failures: [], remaining: [] });
    });

    it('keeps one line per failure when a URL was pasted twice', () => {
        const twice = ['https://a.example/1', 'https://b.example/2', 'https://a.example/1'];

        const oneFailed = readBulkErrors(twice, ['https://a.example/1: blocked']);
        const bothFailed = readBulkErrors(twice, ['https://a.example/1: blocked', 'https://a.example/1: blocked']);

        expect(oneFailed.remaining).toEqual(['https://a.example/1']);
        expect(bothFailed.remaining).toEqual(['https://a.example/1', 'https://a.example/1']);
    });

    it('shows a message that names no sent URL as it came, and keeps no line for it', () => {
        const { failures, remaining } = readBulkErrors(sent, ['Too many URLs in one request (max 500)']);

        expect(failures).toEqual([{ url: null, reason: 'Too many URLs in one request (max 500)' }]);
        expect(remaining).toEqual([]);
    });
});
