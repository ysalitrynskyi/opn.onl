import { describe, expect, it } from 'vitest';
import { pluralize } from './plural';

describe('pluralize', () => {
    it('uses the singular for exactly one', () => {
        expect(pluralize(1, 'link')).toBe('link');
    });

    it('uses the plural for zero and for more than one', () => {
        expect(pluralize(0, 'click')).toBe('clicks');
        expect(pluralize(2, 'click')).toBe('clicks');
        expect(pluralize(1234, 'link')).toBe('links');
    });

    it('takes an explicit plural form', () => {
        expect(pluralize(1, 'API key')).toBe('API key');
        expect(pluralize(3, 'person', 'people')).toBe('people');
    });
});
