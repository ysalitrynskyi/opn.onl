import { Search, SortAsc, SortDesc } from 'lucide-react';
import OrganizeMenu from './OrganizeMenu';
import { swatchColor, type CatalogItem, type OrgOption } from './linkScope';
import type { LinkSortBy, LinkSortOrder } from './types';

interface LinkFiltersProps {
    searchQuery: string;
    onSearchQueryChange: (value: string) => void;
    sortBy: LinkSortBy;
    onSortByChange: (value: LinkSortBy) => void;
    sortOrder: LinkSortOrder;
    onSortOrderChange: (value: LinkSortOrder) => void;
    orgs: OrgOption[];
    orgId: number | null;
    onOrgChange: (id: number | null) => void;
    folders: CatalogItem[];
    folderId: number | null;
    onFolderChange: (id: number | null) => void;
    tags: CatalogItem[];
    tagId: number | null;
    onTagChange: (id: number | null) => void;
    onOrganizeChanged: () => void;
    onFolderDeleted: (id: number) => void;
    onTagDeleted: (id: number) => void;
}

const selectClass = 'rounded-lg border border-line2 bg-surface px-3 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500';

function chipClass(pressed: boolean): string {
    return `inline-flex items-center gap-1 rounded-full border px-2.5 py-1 text-xs font-medium transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary-500 ${
        pressed
            ? 'border-primary-500 bg-primary-50 text-ink'
            : 'border-line bg-surface text-muted hover:text-ink'
    }`;
}

export default function LinkFilters({
    searchQuery,
    onSearchQueryChange,
    sortBy,
    onSortByChange,
    sortOrder,
    onSortOrderChange,
    orgs,
    orgId,
    onOrgChange,
    folders,
    folderId,
    onFolderChange,
    tags,
    tagId,
    onTagChange,
    onOrganizeChanged,
    onFolderDeleted,
    onTagDeleted,
}: LinkFiltersProps) {
    const showContext = orgs.length > 0 || orgId != null;
    const knownOrg = orgId != null && orgs.some(org => org.id === orgId);
    const knownFolder = folderId != null && folders.some(folder => folder.id === folderId);
    const knownTag = tagId != null && tags.some(tag => tag.id === tagId);
    const showTags = tags.length > 0 || (tagId != null && !knownTag);

    return (
        <div className="mb-6 space-y-3">
            <div className="flex flex-col sm:flex-row gap-3">
                <div className="relative flex-1">
                    <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-faint" />
                    <input
                        type="search"
                        aria-label="Search links"
                        placeholder="Search links, titles, notes, tags..."
                        className="w-full rounded-lg border border-line2 bg-surface pl-10 pr-4 py-2.5 text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint"
                        value={searchQuery}
                        onChange={e => onSearchQueryChange(e.target.value)}
                    />
                </div>
                <div className="flex gap-2">
                    <select
                        value={sortBy}
                        onChange={(e) => onSortByChange(e.target.value as LinkSortBy)}
                        aria-label="Sort links by"
                        className={selectClass}
                    >
                        <option value="date">Sort by Date</option>
                        <option value="clicks">Sort by Clicks</option>
                    </select>
                    <button
                        onClick={() => onSortOrderChange(sortOrder === 'asc' ? 'desc' : 'asc')}
                        className="rounded-lg border border-line2 bg-surface p-2.5 text-muted transition-colors hover:text-ink hover:border-ink/30"
                        title={sortOrder === 'asc' ? 'Ascending' : 'Descending'}
                        aria-label={sortOrder === 'asc' ? 'Ascending' : 'Descending'}
                    >
                        {sortOrder === 'asc' ? <SortAsc className="h-4 w-4" /> : <SortDesc className="h-4 w-4" />}
                    </button>
                </div>
            </div>
            <div className="flex flex-col gap-3 sm:flex-row sm:flex-wrap sm:items-center">
                {showContext && (
                    <select
                        aria-label="Link context"
                        value={orgId ?? ''}
                        onChange={(event) => onOrgChange(event.target.value ? Number(event.target.value) : null)}
                        className={selectClass}
                    >
                        <option value="">Personal</option>
                        {orgs.map(org => (
                            <option key={org.id} value={org.id}>{org.name}</option>
                        ))}
                        {orgId != null && !knownOrg && <option value={orgId}>Organization</option>}
                    </select>
                )}
                <select
                    aria-label="Filter by folder"
                    value={folderId ?? ''}
                    onChange={(event) => onFolderChange(event.target.value ? Number(event.target.value) : null)}
                    className={selectClass}
                >
                    <option value="">All folders</option>
                    {folders.map(folder => (
                        <option key={folder.id} value={folder.id}>{folder.name}</option>
                    ))}
                    {folderId != null && !knownFolder && <option value={folderId}>Folder</option>}
                </select>
                {showTags && (
                    <div role="group" aria-label="Filter by tag" className="flex flex-wrap items-center gap-1.5">
                        <button
                            type="button"
                            aria-pressed={tagId == null}
                            onClick={() => onTagChange(null)}
                            className={chipClass(tagId == null)}
                        >
                            All
                        </button>
                        {tags.map(tag => {
                            const pressed = tagId === tag.id;
                            const color = swatchColor(tag.color);
                            return (
                                <button
                                    key={tag.id}
                                    type="button"
                                    aria-pressed={pressed}
                                    onClick={() => onTagChange(pressed ? null : tag.id)}
                                    className={chipClass(pressed)}
                                >
                                    {color && (
                                        <span className="h-2 w-2 rounded-full" style={{ backgroundColor: color }} aria-hidden="true" />
                                    )}
                                    {tag.name}
                                </button>
                            );
                        })}
                        {tagId != null && !knownTag && (
                            <button
                                type="button"
                                aria-pressed
                                onClick={() => onTagChange(null)}
                                className={chipClass(true)}
                            >
                                Tag
                            </button>
                        )}
                    </div>
                )}
                <div className="sm:ml-auto">
                    <OrganizeMenu
                        orgId={orgId}
                        folders={folders}
                        tags={tags}
                        onChanged={onOrganizeChanged}
                        onFolderDeleted={onFolderDeleted}
                        onTagDeleted={onTagDeleted}
                    />
                </div>
            </div>
        </div>
    );
}
