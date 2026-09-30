import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { safeLocalStorage, safeSessionStorage } from './storage';
import { blockSiteStorage } from '../test/helpers';

describe('safe storage wrappers', () => {
    beforeEach(() => {
        localStorage.clear();
        sessionStorage.clear();
    });

    describe('with storage available', () => {
        it('reads, writes and removes local storage like the Storage API', () => {
            expect(safeLocalStorage.setItem('token', 'abc')).toBe(true);
            expect(localStorage.getItem('token')).toBe('abc');
            expect(safeLocalStorage.getItem('token')).toBe('abc');

            safeLocalStorage.removeItem('token');
            expect(localStorage.getItem('token')).toBeNull();
            expect(safeLocalStorage.getItem('token')).toBeNull();
        });

        it('reads, writes and removes session storage like the Storage API', () => {
            expect(safeSessionStorage.setItem('opn.pendingUrl', 'https://example.com')).toBe(true);
            expect(sessionStorage.getItem('opn.pendingUrl')).toBe('https://example.com');
            expect(safeSessionStorage.getItem('opn.pendingUrl')).toBe('https://example.com');

            safeSessionStorage.removeItem('opn.pendingUrl');
            expect(sessionStorage.getItem('opn.pendingUrl')).toBeNull();
        });
    });

    describe('with site storage blocked by the browser', () => {
        let unblock: () => void;
        beforeEach(() => {
            unblock = blockSiteStorage();
        });
        afterEach(() => unblock());

        it('the raw Storage API throws, which is what these wrappers exist for', () => {
            expect(() => window.localStorage.getItem('token')).toThrow(DOMException);
            expect(() => window.sessionStorage.getItem('x')).toThrow(DOMException);
        });

        it('reads come back empty instead of throwing', () => {
            expect(safeLocalStorage.getItem('token')).toBeNull();
            expect(safeSessionStorage.getItem('opn.pendingUrl')).toBeNull();
        });

        it('writes report that nothing was stored instead of throwing', () => {
            expect(safeLocalStorage.setItem('token', 'abc')).toBe(false);
            expect(safeSessionStorage.setItem('opn.pendingUrl', 'https://example.com')).toBe(false);
        });

        it('removals are a no-op instead of throwing', () => {
            expect(() => safeLocalStorage.removeItem('token')).not.toThrow();
            expect(() => safeSessionStorage.removeItem('opn.pendingUrl')).not.toThrow();
        });
    });

    it('reports a failed write when the store itself refuses it (quota exceeded)', () => {
        const setItem = vi.mocked(localStorage.setItem);
        setItem.mockImplementationOnce(() => {
            throw new DOMException('The quota has been exceeded.', 'QuotaExceededError');
        });
        expect(safeLocalStorage.setItem('token', 'abc')).toBe(false);
    });
});
