import { Search, SortAsc, SortDesc } from 'lucide-react';
import type { LinkSortBy, LinkSortOrder } from './types';

interface LinkFiltersProps {
    searchQuery: string;
    onSearchQueryChange: (value: string) => void;
    sortBy: LinkSortBy;
    onSortByChange: (value: LinkSortBy) => void;
    sortOrder: LinkSortOrder;
    onSortOrderChange: (value: LinkSortOrder) => void;
}

export default function LinkFilters({
    searchQuery,
    onSearchQueryChange,
    sortBy,
    onSortByChange,
    sortOrder,
    onSortOrderChange,
}: LinkFiltersProps) {
    return (
        <div className="mb-6 flex flex-col sm:flex-row gap-3">
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
                    className="rounded-lg border border-line2 bg-surface px-3 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500"
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
    );
}
