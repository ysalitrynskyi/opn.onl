import { describe, expect, it } from 'vitest';
import { rulesPayload, rulesSignature, sharesConditions, sortByPriority } from './routingRules';
import type { RoutingRule } from './types';

const rule = (destination_url: string, extra: Partial<RoutingRule> = {}): RoutingRule => ({
    destination_url,
    weight: 1,
    ...extra,
});

describe('rulesPayload', () => {
    it('gives every rule its position as the priority', () => {
        const payload = rulesPayload([
            rule('https://app.example', { match_device: 'Mobile' }),
            rule('https://de.example', { match_country: 'DE' }),
            rule('https://web.example'),
        ]);

        expect(payload.map(r => r.priority)).toEqual([0, 1, 2]);
    });

    it('ignores the priorities the rules were loaded with', () => {
        const payload = rulesPayload([
            rule('https://a.example', { match_device: 'Mobile', priority: 0 }),
            rule('https://b.example', { priority: 0 }),
        ]);

        expect(payload.map(r => r.priority)).toEqual([0, 1]);
    });

    it('lets neighbours with identical conditions share a priority', () => {
        const payload = rulesPayload([
            rule('https://de.example', { match_country: 'DE' }),
            rule('https://a.example', { weight: 3 }),
            rule('https://b.example'),
            rule('https://c.example', { match_os: 'iOS' }),
        ]);

        expect(payload.map(r => r.priority)).toEqual([0, 1, 1, 2]);
        expect(payload.map(r => r.weight)).toEqual([1, 3, 1, 1]);
    });

    it('does not group rules that are not neighbours', () => {
        const payload = rulesPayload([
            rule('https://a.example'),
            rule('https://de.example', { match_country: 'DE' }),
            rule('https://b.example'),
        ]);

        expect(payload.map(r => r.priority)).toEqual([0, 1, 2]);
    });

    it('sends blank conditions as null and only the fields the API takes', () => {
        const [sent] = rulesPayload([
            { ...rule('https://a.example', { match_device: '', match_os: undefined }), id: 7 } as RoutingRule,
        ]);

        expect(sent).toEqual({
            priority: 0,
            match_device: null,
            match_os: null,
            match_country: null,
            match_lang: null,
            destination_url: 'https://a.example',
            weight: 1,
        });
    });
});

describe('sharesConditions', () => {
    it('treats blank, missing and differently cased conditions as the same', () => {
        expect(sharesConditions(rule('a'), rule('b', { match_device: '', match_os: null }))).toBe(true);
        expect(
            sharesConditions(
                rule('a', { match_country: 'DE', match_device: 'Mobile' }),
                rule('b', { match_country: 'de', match_device: ' mobile ' }),
            ),
        ).toBe(true);
    });

    it('tells apart rules that differ in any one condition', () => {
        expect(sharesConditions(rule('a', { match_lang: 'en' }), rule('b', { match_lang: 'de' }))).toBe(false);
        expect(sharesConditions(rule('a'), rule('b', { match_os: 'iOS' }))).toBe(false);
    });
});

describe('sortByPriority', () => {
    it('orders by priority and keeps the API order for ties', () => {
        const sorted = sortByPriority([
            rule('c', { priority: 2 }),
            rule('a1', { priority: 0 }),
            rule('b', { priority: 1 }),
            rule('a2', { priority: 0 }),
        ]);

        expect(sorted.map(r => r.destination_url)).toEqual(['a1', 'a2', 'b', 'c']);
    });

    it('does not change the array it is given', () => {
        const rules = [rule('b', { priority: 1 }), rule('a', { priority: 0 })];
        sortByPriority(rules);
        expect(rules.map(r => r.destination_url)).toEqual(['b', 'a']);
    });
});

describe('rulesSignature', () => {
    it('is the same for the same rules whatever priorities they carry', () => {
        const loaded = [rule('https://a.example', { priority: 4 }), rule('https://b.example', { priority: 9 })];
        const again = [rule('https://a.example', { priority: 0 }), rule('https://b.example', { priority: 1 })];
        expect(rulesSignature(loaded)).toBe(rulesSignature(again));
    });

    it('changes with the order, a destination, a condition or a weight', () => {
        const base = [rule('https://a.example'), rule('https://b.example', { match_device: 'Mobile' })];
        const signature = rulesSignature(base);

        expect(rulesSignature([base[1], base[0]])).not.toBe(signature);
        expect(rulesSignature([rule('https://a.example/x'), base[1]])).not.toBe(signature);
        expect(rulesSignature([base[0], rule('https://b.example', { match_device: 'Tablet' })])).not.toBe(signature);
        expect(rulesSignature([base[0], { ...base[1], weight: 2 }])).not.toBe(signature);
    });

    it('ignores rules that have no destination yet', () => {
        const saved = [rule('https://a.example')];
        expect(rulesSignature([...saved, rule('  ')])).toBe(rulesSignature(saved));
    });
});
