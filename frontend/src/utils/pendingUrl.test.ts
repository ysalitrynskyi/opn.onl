import { beforeEach, describe, expect, it } from 'vitest';
import { savePendingUrl, takePendingUrl } from './pendingUrl';

describe('takePendingUrl', () => {
    beforeEach(() => {
        sessionStorage.clear();
        // Drain any leftover from a previous take in this module.
        takePendingUrl();
        takePendingUrl();
    });

    it('returns the same URL on a second take after storage is cleared', () => {
        savePendingUrl('https://example.com/long');
        expect(takePendingUrl()).toBe('https://example.com/long');
        expect(sessionStorage.getItem('opn.pendingUrl')).toBeNull();
        expect(takePendingUrl()).toBe('https://example.com/long');
    });
});
