import { describe, expect, it } from 'vitest';
import { MAX_BIO_ORDER, bioOrderFits } from './bioOrder';

describe('bioOrderFits', () => {
    it('allows the server cap and refuses one more', () => {
        expect(MAX_BIO_ORDER).toBe(500);
        expect(bioOrderFits(0)).toBe(true);
        expect(bioOrderFits(500)).toBe(true);
        expect(bioOrderFits(501)).toBe(false);
    });
});
