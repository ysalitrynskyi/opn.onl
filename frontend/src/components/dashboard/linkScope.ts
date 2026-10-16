/** What `GET /links` is asked for. Folder and tag are one id each, matching the handler. */
export interface LinkListScope {
    search: string;
    orgId: number | null;
    folderId: number | null;
    tagId: number | null;
}

export const EMPTY_SCOPE: LinkListScope = {
    search: '',
    orgId: null,
    folderId: null,
    tagId: null,
};

/** A folder or tag row the dashboard can list. `link_count` is 0 when the payload omitted it. */
export interface CatalogItem {
    id: number;
    name: string;
    color: string | null;
    link_count: number;
}

export interface OrgOption {
    id: number;
    name: string;
}

const HEX_COLOR = /^#(?:[0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/;

/** `#rgb` or `#rrggbb`, the only colours the API stores. Anything else is dropped. */
export function swatchColor(color: string | null | undefined): string | undefined {
    return color && HEX_COLOR.test(color) ? color : undefined;
}

/** A positive 32-bit id from a query string. Anything else is "not set". */
export function parsePositiveId(raw: string | null): number | null {
    if (raw == null || !/^[1-9]\d*$/.test(raw)) return null;
    const value = Number(raw);
    if (!Number.isSafeInteger(value) || value > 2_147_483_647) return null;
    return value;
}

export function sameScope(a: LinkListScope, b: LinkListScope): boolean {
    return a.search === b.search
        && a.orgId === b.orgId
        && a.folderId === b.folderId
        && a.tagId === b.tagId;
}

/** Personal links, with no folder or tag filter: the account-wide totals apply. */
export function isUnfilteredAccount(scope: LinkListScope): boolean {
    return scope.search === '' && scope.orgId == null && scope.folderId == null && scope.tagId == null;
}

/**
 * Query string for `GET /links`. `folder_id` is an equality match, so there is
 * no value that means "not in a folder"; leaving it out is every folder.
 */
export function linksQuery(scope: LinkListScope, limit: number, offset: number): string {
    const params = new URLSearchParams({ limit: String(limit) });
    if (offset > 0) params.set('offset', String(offset));
    if (scope.search) params.set('search', scope.search);
    if (scope.orgId != null) params.set('org_id', String(scope.orgId));
    if (scope.folderId != null) params.set('folder_id', String(scope.folderId));
    if (scope.tagId != null) params.set('tag_id', String(scope.tagId));
    return params.toString();
}

function record(item: unknown): Record<string, unknown> | null {
    if (!item || typeof item !== 'object') return null;
    return item as Record<string, unknown>;
}

/** Folder or tag rows. A link list (no `name`) does not count, so a shared mock cannot fill the menu. */
export function catalogItems(data: unknown): CatalogItem[] {
    if (!Array.isArray(data)) return [];
    const items: CatalogItem[] = [];
    for (const item of data) {
        const row = record(item);
        if (!row || typeof row.id !== 'number' || typeof row.name !== 'string') continue;
        items.push({
            id: row.id,
            name: row.name,
            color: typeof row.color === 'string' ? row.color : null,
            link_count: typeof row.link_count === 'number' ? row.link_count : 0,
        });
    }
    return items;
}

/** Organizations. `slug` is required so a link payload cannot be read as a team. */
export function orgOptions(data: unknown): OrgOption[] {
    if (!Array.isArray(data)) return [];
    const items: OrgOption[] = [];
    for (const item of data) {
        const row = record(item);
        if (!row || typeof row.id !== 'number' || typeof row.name !== 'string' || typeof row.slug !== 'string') continue;
        items.push({ id: row.id, name: row.name });
    }
    return items;
}
