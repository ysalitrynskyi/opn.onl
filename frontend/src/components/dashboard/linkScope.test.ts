import { describe, expect, it } from 'vitest';
import { catalogItems, linksQuery, orgOptions, parsePositiveId, readOnlyIn, type LinkListScope } from './linkScope';

const scope = (overrides: Partial<LinkListScope> = {}): LinkListScope => ({
    search: '',
    orgId: null,
    folderId: null,
    tagId: null,
    ...overrides,
});

describe('parsePositiveId', () => {
    it('accepts a positive 32-bit id and rejects everything else', () => {
        expect(parsePositiveId('4')).toBe(4);
        expect(parsePositiveId('2147483647')).toBe(2147483647);
        expect(parsePositiveId(null)).toBeNull();
        expect(parsePositiveId('')).toBeNull();
        expect(parsePositiveId('0')).toBeNull();
        expect(parsePositiveId('-3')).toBeNull();
        expect(parsePositiveId('1.5')).toBeNull();
        expect(parsePositiveId('no')).toBeNull();
        expect(parsePositiveId('2147483648')).toBeNull();
    });
});

describe('linksQuery', () => {
    it('sends folder, tag and team only when set, and never an unfiled flag', () => {
        expect(linksQuery(scope(), 100, 0)).toBe('limit=100');
        expect(linksQuery(scope({ search: 'needle', orgId: 2, folderId: 3, tagId: 4 }), 100, 20)).toBe(
            'limit=100&offset=20&search=needle&org_id=2&folder_id=3&tag_id=4',
        );
        expect(linksQuery(scope({ folderId: 3 }), 100, 0)).not.toContain('null');
    });
});

describe('catalog parsers', () => {
    it('ignores a link list and a non-array', () => {
        const link = { id: 1, code: 'abc', original_url: 'https://example.com' };
        expect(catalogItems([link])).toEqual([]);
        expect(catalogItems({})).toEqual([]);
        expect(catalogItems([{ id: 3, name: 'Campaigns', color: '#2563eb', link_count: 2 }])).toEqual([
            { id: 3, name: 'Campaigns', color: '#2563eb', link_count: 2 },
        ]);
    });

    it('requires a slug before treating a row as a team', () => {
        expect(orgOptions([{ id: 2, name: 'Acme' }])).toEqual([]);
        expect(orgOptions([{ id: 2, name: 'Acme', slug: 'acme' }])).toEqual([{ id: 2, name: 'Acme', role: null }]);
    });

    it('keeps the caller role that GET /orgs reports', () => {
        expect(orgOptions([{ id: 2, name: 'Acme', slug: 'acme', role: 'viewer' }])).toEqual([
            { id: 2, name: 'Acme', role: 'viewer' },
        ]);
        expect(orgOptions([{ id: 2, name: 'Acme', slug: 'acme', role: 7 }])).toEqual([
            { id: 2, name: 'Acme', role: null },
        ]);
    });
});

describe('readOnlyIn', () => {
    it('is read-only for a viewer or a role the client does not know, and not for an unknown answer', () => {
        expect(readOnlyIn({ id: 1, name: 'A', role: 'viewer' })).toBe(true);
        expect(readOnlyIn({ id: 1, name: 'A', role: 'auditor' })).toBe(true);
        for (const role of ['owner', 'admin', 'editor']) {
            expect(readOnlyIn({ id: 1, name: 'A', role })).toBe(false);
        }
        // An older server that sends no role, or a team not in the list yet:
        // the controls stay and the server decides.
        expect(readOnlyIn({ id: 1, name: 'A', role: null })).toBe(false);
        expect(readOnlyIn(null)).toBe(false);
    });
});
