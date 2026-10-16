import { useEffect, useState, useMemo, useCallback, useRef } from 'react';
import { useNavigate } from 'react-router-dom';
import {
    Plus, Download, X, ChevronLeft, ChevronRight,
    MousePointer,
    Zap, Link2, Upload, ArrowRight,
} from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { API_ENDPOINTS, authFetch, failureMessage, responseError, shortLinkBase, totalCount } from '../config/api';
import SEO from '../components/SEO';
import logger from '../utils/logger';
import { toast } from '../components/Toast';
import ShareModal from '../components/ShareModal';
import { useKeyboardShortcuts } from '../hooks/useKeyboardShortcuts';
import LiveClickIndicator from '../components/dashboard/LiveClickIndicator';
import { useLiveClicks } from '../hooks/useLiveClicks';
import EditModal from '../components/dashboard/EditModal';
import QRModal from '../components/dashboard/QRModal';
import PreviewModal from '../components/dashboard/PreviewModal';
import BulkImportModal, { type BulkReport } from '../components/dashboard/BulkImportModal';
import { parseBulkUrls, readBulkErrors } from '../components/dashboard/bulkImport';
import Skeleton from '../components/dashboard/Skeleton';
import CreateLinkForm from '../components/dashboard/CreateLinkForm';
import LinkFilters from '../components/dashboard/LinkFilters';
import LinkRow from '../components/dashboard/LinkRow';
import BulkActionBar, { SelectAllCheckbox } from '../components/dashboard/BulkActionBar';
import { BULK_IDS_PER_REQUEST, bulkAffectedCount, bulkResultMessage, bulkStoppedMessage, runBulkChunks } from '../components/dashboard/bulkActions';
import type { AppSettings, LinkData, LinkSortBy, LinkSortOrder, LinkUpdatePayload, SparklineSeries } from '../components/dashboard/types';
import { takePendingUrl } from '../utils/pendingUrl';
import { safeLocalStorage } from '../utils/storage';
import { pluralize } from '../utils/plural';

const LINKS_PER_PAGE = 20;
const SPARKLINE_BATCH_SIZE = 80;
// Links are loaded from the server a page at a time; LINKS_PER_PAGE is how many
// of the loaded ones show at once. MAX_PAGE_SIZE is the server's own cap.
const PAGE_SIZE = 100;
const MAX_PAGE_SIZE = 1000;
const SEARCH_DEBOUNCE_MS = 300;

function linksUrl(limit: number, offset: number, search: string) {
    const params = new URLSearchParams({ limit: String(limit) });
    if (offset > 0) params.set('offset', String(offset));
    if (search) params.set('search', search);
    return `${API_ENDPOINTS.links}?${params}`;
}

// A link that was deleted elsewhere is refused with the same 403 as one the
// user may not edit, so link ids cannot be probed. Either way the link is no
// longer theirs to edit, and that is what the dialog says. The other 403s on
// an update (a blocked destination, a folder out of reach, the bio page) keep
// their own messages.
const EDIT_DENIED_MESSAGE = "You don't have permission to modify this link";
const LINK_GONE_MESSAGE = 'This link no longer exists or you no longer have access to it.';

function updateFailureMessage(status: number, message: string | undefined) {
    if (status === 403 && message === EDIT_DENIED_MESSAGE) return LINK_GONE_MESSAGE;
    return message || 'Failed to update link';
}

export default function Dashboard() {
    const [links, setLinks] = useState<LinkData[]>([]);
    // How many links match what `links` was fetched with (all of the account's,
    // or those matching `activeSearch`). More than `links.length` means the
    // server holds the rest.
    const [total, setTotal] = useState(0);
    const [activeSearch, setActiveSearch] = useState('');
    // How many links the account has, searched or not.
    const [accountTotal, setAccountTotal] = useState(0);
    const [accountClicks, setAccountClicks] = useState<number | null>(null);
    const [loadingMore, setLoadingMore] = useState(false);
    const [loading, setLoading] = useState(true);
    // Homepage shorten hands the URL over before the create form mounts.
    const [createInitialUrl, setCreateInitialUrl] = useState('');
    const [searchQuery, setSearchQuery] = useState('');
    const [editingLink, setEditingLink] = useState<LinkData | null>(null);
    const [qrLink, setQrLink] = useState<LinkData | null>(null);
    const [shareLink, setShareLink] = useState<LinkData | null>(null);
    const [copiedId, setCopiedId] = useState<number | null>(null);
    const [copiedSourceId, setCopiedSourceId] = useState<number | null>(null);
    const [error, setError] = useState('');
    const [currentPage, setCurrentPage] = useState(1);
    const [sortBy, setSortBy] = useState<LinkSortBy>('date');
    const [sortOrder, setSortOrder] = useState<LinkSortOrder>('desc');
    const [appSettings, setAppSettings] = useState<AppSettings>({
        custom_aliases_enabled: true,
        min_alias_length: 5,
        max_alias_length: 50,
        qr_branding_enabled: true,
        burn_after_reading_enabled: false,
        safe_link_interstitial_enabled: false,
        conditional_routing_enabled: false,
        link_in_bio_enabled: false
    });
    const [sparklineData, setSparklineData] = useState<Record<number, SparklineSeries>>({});
    const [previewLink, setPreviewLink] = useState<LinkData | null>(null);
    const [showBulkImport, setShowBulkImport] = useState(false);
    const [bulkUrls, setBulkUrls] = useState('');
    const [bulkImporting, setBulkImporting] = useState(false);
    const [cloningIds, setCloningIds] = useState<ReadonlySet<number>>(new Set());
    const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
    const [bulkBusy, setBulkBusy] = useState(false);
    // Shown inside the modal: the page's own error banner sits behind its backdrop.
    const [bulkReport, setBulkReport] = useState<BulkReport | null>(null);
    const navigate = useNavigate();
    const linksFetchId = useRef(0);
    const sparklineFetchId = useRef(0);
    const bulkInFlight = useRef(false);
    const accountClicksRequested = useRef(false);
    // The list as it is now, for requests that finish after a later render.
    const listRef = useRef({ loaded: 0, search: '' });
    // IDs removed locally while a fetchLinks was in flight. Stale responses
    // must not fold these back in when they merge newly created rows.
    const droppedLinkIds = useRef(new Set<number>());
    // Links whose clone request has not been answered yet.
    const cloningInFlight = useRef(new Set<number>());

    useEffect(() => {
        const token = safeLocalStorage.getItem('token');
        if (!token) {
            navigate('/login');
            return;
        }
        const pending = takePendingUrl();
        if (pending) setCreateInitialUrl(pending);
        fetchLinks();
        fetchSettings();
    }, [navigate]);

    const fetchSettings = async () => {
        try {
            const res = await fetch(API_ENDPOINTS.appSettings);
            if (res.ok) {
                const data = await res.json();
                setAppSettings(data);
            }
        } catch (err) {
            logger.error('Failed to fetch settings', err);
        }
    };

    // Keyboard shortcuts
    useKeyboardShortcuts([
        {
            key: 'n',
            handler: () => {
                const input = document.querySelector('input[placeholder*="example.com"]') as HTMLInputElement;
                if (input) input.focus();
            },
            description: 'Focus new link input',
        },
        {
            key: '/',
            handler: () => {
                const input = document.querySelector('input[placeholder*="Search"]') as HTMLInputElement;
                if (input) input.focus();
            },
            description: 'Focus search',
        },
        {
            key: 'Escape',
            handler: () => {
                // One modal per press, topmost first: the bulk-import modal
                // renders after the rest, so it paints over any of them.
                if (showBulkImport) {
                    setShowBulkImport(false);
                } else if (previewLink) {
                    setPreviewLink(null);
                } else if (shareLink) {
                    setShareLink(null);
                } else if (qrLink) {
                    setQrLink(null);
                } else if (editingLink) {
                    setEditingLink(null);
                } else {
                    setSearchQuery('');
                }
            },
            description: 'Close modal / Clear search',
        },
    ]);

    // Bulk import handler
    const handleBulkImport = async () => {
        const urls = parseBulkUrls(bulkUrls);

        if (urls.length === 0) {
            setBulkReport({
                message: 'No valid URLs found. Each line should contain a valid URL starting with http:// or https://',
                failures: [],
            });
            return;
        }

        setBulkImporting(true);
        setBulkReport(null);

        try {
            const res = await authFetch(API_ENDPOINTS.bulkLinks, {
                method: 'POST',
                body: JSON.stringify({ urls }),
            });

            if (res.ok) {
                const data = await res.json();
                fetchLinks();

                const errors: string[] = Array.isArray(data.errors) ? data.errors : [];
                if (errors.length === 0) {
                    setShowBulkImport(false);
                    setBulkUrls('');
                    return;
                }

                // Partly done: keep the modal open with only what failed in the
                // box, so nothing has to be pasted again.
                const created = Array.isArray(data.links) ? data.links.length : 0;
                const { failures, remaining } = readBulkErrors(
                    urls,
                    errors,
                    Array.isArray(data.failed) ? data.failed : undefined,
                );
                setBulkUrls(remaining.join('\n'));
                setBulkReport({
                    message: `Created ${created} ${pluralize(created, 'link')}. ${failures.length} failed.`,
                    failures,
                });
            } else {
                const data = await res.json();
                const fromList = Array.isArray(data.errors) && data.errors.length > 0
                    ? data.errors.join(', ')
                    : null;
                setBulkReport({ message: fromList || data.error || 'Failed to import links', failures: [] });
            }
        } catch {
            setBulkReport({ message: 'Network error during import', failures: [] });
        } finally {
            setBulkImporting(false);
        }
    };

    // The term the server is asked for; it ignores surrounding spaces, so the
    // filter below must too or the two would disagree on what matches.
    const searchTerm = searchQuery.trim();

    // Filter and sort links
    const filteredLinks = useMemo(() => {
        let result = [...links];

        // Search filter
        if (searchTerm) {
            const query = searchTerm.toLowerCase();
            result = result.filter(link =>
                link.code.toLowerCase().includes(query) ||
                link.original_url.toLowerCase().includes(query) ||
                link.title?.toLowerCase().includes(query) ||
                link.notes?.toLowerCase().includes(query) ||
                link.tags.some(t => t.name.toLowerCase().includes(query))
            );
        }

        // Sort - pinned items first, then by selected sort
        result.sort((a, b) => {
            // Pinned items always first
            if (a.is_pinned && !b.is_pinned) return -1;
            if (!a.is_pinned && b.is_pinned) return 1;

            let comparison = 0;
            if (sortBy === 'date') {
                comparison = new Date(a.created_at).getTime() - new Date(b.created_at).getTime();
            } else {
                comparison = a.click_count - b.click_count;
            }
            return sortOrder === 'asc' ? comparison : -comparison;
        });

        return result;
    }, [links, searchTerm, sortBy, sortOrder]);

    // Pagination
    const totalPages = Math.ceil(filteredLinks.length / LINKS_PER_PAGE);
    const paginatedLinks = useMemo(() => {
        const start = (currentPage - 1) * LINKS_PER_PAGE;
        return filteredLinks.slice(start, start + LINKS_PER_PAGE);
    }, [filteredLinks, currentPage]);

    // Reset to page 1 when search changes
    useEffect(() => {
        setCurrentPage(1);
    }, [searchQuery]);

    // A selection is the rows on the page the user is looking at. A new page
    // or a new search shows different rows, so the old ticks would lie.
    useEffect(() => {
        setSelectedIds(prev => (prev.size === 0 ? prev : new Set()));
    }, [currentPage, searchQuery]);

    // Clamp the current page when the list shrinks (e.g. deleting the last link
    // on the final page) so the user is never stranded on an empty page.
    useEffect(() => {
        if (totalPages > 0 && currentPage > totalPages) {
            setCurrentPage(totalPages);
        }
    }, [totalPages, currentPage]);

    // Reload the list, keeping the search it was fetched with unless `search`
    // names another one. A reload asks for as many links as are showing, so
    // saving an edit does not fold away pages the user loaded; a new search
    // starts again from the first page.
    const fetchLinks = async (search = listRef.current.search) => {
        const requestId = ++linksFetchId.current;
        const showing = search === listRef.current.search ? listRef.current.loaded : 0;
        const limit = Math.min(MAX_PAGE_SIZE, Math.max(PAGE_SIZE, showing));
        try {
            const res = await authFetch(linksUrl(limit, 0, search));
            if (res.ok) {
                const data: LinkData[] = await res.json();
                if (requestId !== linksFetchId.current) {
                    // Delete/pin bumped the generation so this snapshot must
                    // not replace the optimistic list (that would resurrect a
                    // deleted row). Still fold in ids we do not have, or a
                    // create that raced the mutation never appears.
                    setLinks(prev => {
                        const have = new Set(prev.map(l => l.id));
                        const extras = data.filter(
                            l => !have.has(l.id) && !droppedLinkIds.current.has(l.id),
                        );
                        return extras.length === 0 ? prev : [...extras, ...prev];
                    });
                    return;
                }
                const count = totalCount(res) ?? data.length;
                setLinks(data);
                setTotal(count);
                setActiveSearch(search);
                if (!search) setAccountTotal(count);
            } else {
                const data = await res.json().catch(() => null) as { error?: string } | null;
                if (requestId !== linksFetchId.current) return;
                setError(data?.error || 'Failed to load links. Please try again.');
            }
        } catch (error) {
            if (requestId !== linksFetchId.current) return;
            logger.error('Failed to fetch links', error);
            setError('Failed to load links. Please try again.');
        } finally {
            if (requestId === linksFetchId.current) {
                setLoading(false);
            }
        }
    };

    // Append the next page of the list being shown.
    const loadMore = async () => {
        if (loadingMore) return;
        setLoadingMore(true);
        const requestId = linksFetchId.current;
        const wasOnLastPage = currentPage === totalPages;
        try {
            const res = await authFetch(linksUrl(PAGE_SIZE, links.length, activeSearch));
            if (!res.ok) {
                setError('Failed to load more links. Please try again.');
                return;
            }
            const data: LinkData[] = await res.json();
            // A reload, delete or pin that ran meanwhile changed the list this
            // page was cut for; appending it could repeat or skip links.
            if (requestId !== linksFetchId.current) return;
            setLinks(prev => {
                // Links created since the first page push the later ones down,
                // so the page can start with rows that are already here.
                const have = new Set(prev.map(l => l.id));
                const fresh = data.filter(l => !have.has(l.id) && !droppedLinkIds.current.has(l.id));
                return fresh.length === 0 ? prev : [...prev, ...fresh];
            });
            const count = totalCount(res) ?? links.length + data.length;
            setTotal(count);
            if (!activeSearch) setAccountTotal(count);
            // Land on the first of the new links instead of looking unchanged.
            if (wasOnLastPage) setCurrentPage(p => p + 1);
        } catch (error) {
            logger.error('Failed to load more links', error);
            setError('Failed to load more links. Please try again.');
        } finally {
            setLoadingMore(false);
        }
    };

    useEffect(() => {
        listRef.current = { loaded: links.length, search: activeSearch };
    }, [links.length, activeSearch]);

    // With every link loaded the search box filters them as the user types.
    // With more on the server it asks the server (after a pause, not on each
    // key), or the links that are not loaded could never be found.
    useEffect(() => {
        if (loading || searchTerm === activeSearch) return;
        if (searchTerm && !activeSearch && links.length >= total) return;
        const timer = setTimeout(() => fetchLinks(searchTerm), SEARCH_DEBOUNCE_MS);
        return () => clearTimeout(timer);
    }, [loading, searchTerm, activeSearch, links.length, total]);

    // Fetch sparkline data for the given links. Batch ids so a large account
    // cannot blow typical request-line limits (~8KB) with one query string.
    const fetchSparklines = useCallback(async (linkIds: number[]) => {
        if (linkIds.length === 0) return;
        const requestId = ++sparklineFetchId.current;
        const merged: Record<number, SparklineSeries> = {};
        try {
            for (let i = 0; i < linkIds.length; i += SPARKLINE_BATCH_SIZE) {
                if (requestId !== sparklineFetchId.current) return;
                const batch = linkIds.slice(i, i + SPARKLINE_BATCH_SIZE);
                const res = await authFetch(`${API_ENDPOINTS.sparklines}?ids=${batch.join(',')}`);
                if (requestId !== sparklineFetchId.current) return;
                if (res.ok) {
                    const data = await res.json();
                    for (const item of data.sparklines) {
                        merged[item.link_id] = { data: item.data, labels: item.labels };
                    }
                }
            }
            if (requestId !== sparklineFetchId.current) return;
            // A failed batch is skipped above, so `merged` can be a subset.
            // Fold into the previous cache instead of replacing it, or a
            // later batch failure wipes sparklines we already had.
            setSparklineData(prev => ({ ...prev, ...merged }));
        } catch (error) {
            if (requestId !== sparklineFetchId.current) return;
            logger.error('Failed to fetch sparklines', error);
        }
    }, []);

    // The ids of the links on the page as one stable string. Pinning, editing
    // or refreshing into the same links gives a new `links` array but leaves
    // this unchanged, so none of those asks the server for charts again.
    const linkIdsKey = useMemo(
        () => links.map(l => l.id).sort((a, b) => a - b).join(','),
        [links],
    );
    // The charts held so far, readable from the effect below without every
    // arriving batch re-running it.
    const sparklineDataRef = useRef(sparklineData);
    useEffect(() => {
        sparklineDataRef.current = sparklineData;
    }, [sparklineData]);

    // When the set of links changes, fetch the ones without a chart and drop
    // the charts of links that are gone (a delete needs no request at all).
    useEffect(() => {
        const ids = linkIdsKey ? linkIdsKey.split(',').map(Number) : [];
        const shown = new Set(ids);
        setSparklineData(prev => {
            const kept = Object.entries(prev).filter(([id]) => shown.has(Number(id)));
            return kept.length === Object.keys(prev).length ? prev : Object.fromEntries(kept);
        });
        fetchSparklines(ids.filter(id => !(id in sparklineDataRef.current)));
    }, [linkIdsKey, fetchSparklines]);

    // The rows on hand are the whole account only when the server has nothing
    // more to give. Then their totals are exact. Otherwise they say nothing
    // about the account: the link count is the server's, the click count is the
    // one Settings shows, and "active" is unknown, so it is left out.
    const shownTotal = Math.max(total, links.length);
    const hasMore = links.length < shownTotal;
    const unloaded = shownTotal - links.length;
    const accountLoaded = !activeSearch && !hasMore;
    const linkCount = accountLoaded ? links.length : accountTotal;
    const totalClicks = accountLoaded ? links.reduce((sum, link) => sum + link.click_count, 0) : accountClicks;
    const activeLinks = accountLoaded ? links.filter(l => l.is_active).length : null;

    const needsAccountClicks = !loading && !accountLoaded;
    useEffect(() => {
        if (!needsAccountClicks || accountClicksRequested.current) return;
        accountClicksRequested.current = true;
        (async () => {
            try {
                const res = await authFetch(API_ENDPOINTS.userProfile);
                if (!res.ok) return;
                const profile = await res.json();
                if (typeof profile?.total_clicks === 'number') setAccountClicks(profile.total_clicks);
            } catch (error) {
                logger.error('Failed to fetch account totals', error);
            }
        })();
    }, [needsAccountClicks]);

    const pageIds = paginatedLinks.map(link => link.id);
    // A click on this page moves the row. The header total is the sum of the
    // loaded rows when they are the whole account, and the profile total
    // otherwise, so that one is moved too. Clicks on other pages are ignored.
    const onLiveClick = useCallback((linkId: number) => {
        setLinks(prev => prev.map(link => (
            link.id === linkId ? { ...link, click_count: link.click_count + 1 } : link
        )));
        setAccountClicks(prev => (prev == null ? prev : prev + 1));
    }, []);
    const live = useLiveClicks(pageIds, onLiveClick);
    const selectedOnPage = pageIds.filter(id => selectedIds.has(id)).length;
    const allOnPage = pageIds.length > 0 && selectedOnPage === pageIds.length;

    const toggleSelected = (id: number) => {
        setSelectedIds(prev => {
            const next = new Set(prev);
            if (next.has(id)) next.delete(id);
            else next.add(id);
            return next;
        });
    };

    const toggleSelectPage = () => {
        setSelectedIds(prev => {
            const next = new Set(prev);
            if (allOnPage) {
                for (const id of pageIds) next.delete(id);
            } else {
                for (const id of pageIds) next.add(id);
            }
            return next;
        });
    };

    const clearSelection = () => {
        setSelectedIds(new Set());
        // The bar unmounts with the selection, so the trigger it would return
        // focus to is gone. The select-all box is the control that remains.
        requestAnimationFrame(() => {
            document.getElementById('link-select-all')?.focus();
        });
    };

    // Bulk routes answer with a count, not a list of failures. Ids the caller
    // may not change are skipped, and a shortfall against `ids` is that fact.
    // Click caps are not on POST /links/bulk/update, so they are not offered.
    const applyBulk = async (
        action: 'delete' | 'update',
        extra: Record<string, unknown>,
    ): Promise<boolean> => {
        const ids = [...selectedIds];
        if (ids.length === 0 || bulkInFlight.current) return false;
        bulkInFlight.current = true;
        setBulkBusy(true);
        const field = action === 'delete' ? 'deleted' : 'updated';
        const endpoint = action === 'delete' ? API_ENDPOINTS.bulkDeleteLinks : API_ENDPOINTS.bulkUpdateLinks;
        const fallback = action === 'delete' ? 'Failed to delete links' : 'Failed to update links';
        try {
            const run = await runBulkChunks(ids, async (chunk) => {
                try {
                    const res = await authFetch(endpoint, {
                        method: 'POST',
                        body: JSON.stringify({ ids: [...chunk], ...extra }),
                    });
                    if (!res.ok) {
                        return { ok: false as const, error: await responseError(res, fallback) };
                    }
                    const affected = bulkAffectedCount(await res.json().catch(() => null), field);
                    if (affected === null) {
                        return { ok: false as const, error: 'The server response could not be read.' };
                    }
                    return { ok: true as const, affected };
                } catch (error) {
                    return { ok: false as const, error: failureMessage(error, 'Network error. Please try again.') };
                }
            }, BULK_IDS_PER_REQUEST);

            if (run.error && run.sent === 0) {
                setError(run.error);
                return false;
            }
            if (run.error) {
                setError(bulkStoppedMessage(action, run.affected, run.error));
            } else {
                const result = bulkResultMessage(action, run.affected, run.sent);
                if (result.complete) {
                    setError('');
                    toast(result.message, 'success');
                } else {
                    setError(result.message);
                }
            }

            // A delete that the server accepted must not be written back by a
            // list request that left before the delete. The same set is what
            // a single-link delete records.
            if (action === 'delete') {
                for (const id of ids) droppedLinkIds.current.add(id);
            }
            await fetchLinks();
            clearSelection();
            return true;
        } finally {
            bulkInFlight.current = false;
            setBulkBusy(false);
        }
    };

    const handleDelete = async (id: number) => {
        if (!confirm('Are you sure you want to delete this link?')) return;

        try {
            const res = await authFetch(API_ENDPOINTS.linkDelete(id), {
                method: 'DELETE',
            });
            if (res.ok) {
                // Drop any in-flight fetchLinks so it cannot write a snapshot
                // that still contains this id over the optimistic list.
                droppedLinkIds.current.add(id);
                linksFetchId.current += 1;
                setLinks(prev => prev.filter(l => l.id !== id));
                setTotal(t => Math.max(0, t - 1));
                setAccountTotal(t => Math.max(0, t - 1));
            } else {
                setError('Failed to delete link');
            }
        } catch (error) {
            logger.error('Failed to delete link', error);
            setError('Network error. Please try again.');
        }
    };

    const handleUpdate = async (id: number, data: LinkUpdatePayload) => {
        try {
            const res = await authFetch(API_ENDPOINTS.linkUpdate(id), {
                method: 'PUT',
                body: JSON.stringify(data),
            });
            if (!res.ok) {
                const response = await res.json().catch(() => null) as { error?: string } | null;
                throw new Error(updateFailureMessage(res.status, response?.error));
            }

            await fetchLinks();
        } catch (error) {
            logger.error('Failed to update link', error);
            const updateError = error instanceof Error
                ? error
                : new Error('Network error. Please try again.');
            setError(updateError.message);
            throw updateError;
        }
    };

    const handleCopy = async (link: LinkData) => {
        try {
            const mainUrl = `${shortLinkBase()}/${link.code}`;
            await navigator.clipboard.writeText(mainUrl);
            setCopiedId(link.id);
            toast('Short link copied!', 'success');
            setTimeout(() => setCopiedId(null), 2000);
        } catch {
            toast('Failed to copy link', 'error');
        }
    };

    const handleCopySource = async (link: LinkData) => {
        try {
            await navigator.clipboard.writeText(link.original_url);
            setCopiedSourceId(link.id);
            toast('Source URL copied!', 'success');
            setTimeout(() => setCopiedSourceId(null), 2000);
        } catch {
            toast('Failed to copy source URL', 'error');
        }
    };

    const handlePin = async (link: LinkData) => {
        try {
            const res = await authFetch(API_ENDPOINTS.linkPin(link.id), {
                method: 'POST',
            });
            if (res.ok) {
                const data = await res.json();
                linksFetchId.current += 1;
                setLinks(prev => prev.map(l => l.id === link.id ? { ...l, is_pinned: data.is_pinned } : l));
                toast(data.message, 'success');
            } else {
                toast('Failed to update pin status', 'error');
            }
        } catch {
            toast('Network error', 'error');
        }
    };

    const handleClone = async (link: LinkData) => {
        // A second click before the first answer would make a second copy (and
        // spend a second creation token). The ref answers at once; the state
        // is what disables the button.
        if (cloningInFlight.current.has(link.id)) return;
        cloningInFlight.current.add(link.id);
        setCloningIds(new Set(cloningInFlight.current));
        try {
            const res = await authFetch(API_ENDPOINTS.linkClone(link.id), {
                method: 'POST',
            });
            if (res.ok) {
                const data = await res.json();
                toast(`Link cloned! New code: ${data.code}`, 'success');
                fetchLinks();
            } else {
                const data = await res.json();
                toast(data.error || 'Failed to clone link', 'error');
            }
        } catch {
            toast('Network error', 'error');
        } finally {
            cloningInFlight.current.delete(link.id);
            setCloningIds(new Set(cloningInFlight.current));
        }
    };

    const handleExport = async () => {
        try {
            const response = await authFetch(API_ENDPOINTS.exportLinks);

            if (!response.ok) throw new Error('Failed to export links');

            const blob = await response.blob();
            const url = window.URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url;
            a.download = 'opn_onl_links.csv';
            document.body.appendChild(a);
            a.click();
            a.remove();
            window.URL.revokeObjectURL(url);
        } catch (err) {
            setError(err instanceof Error ? err.message : 'Failed to export links');
        }
    };

    const handleShare = (link: LinkData) => {
        // On mobile, try native share first
        if (navigator.share && /Android|iPhone|iPad|iPod/i.test(navigator.userAgent)) {
            const mainUrl = `${shortLinkBase()}/${link.code}`;
            navigator.share({
                title: link.title || 'Check out this link',
                url: mainUrl,
            }).catch(() => {
                // If native share fails, show modal
                setShareLink(link);
            });
        } else {
            // On desktop, show share modal
            setShareLink(link);
        }
    };

    if (loading) {
        return (
            <div className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8 py-10">
                <SEO title="Dashboard" noIndex />
                <div className="mb-8">
                    <div className="h-9 w-48 rounded bg-line2/70 animate-pulse" />
                </div>
                <Skeleton />
            </div>
        );
    }

    return (
        <div className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8 py-10">
            <SEO title="Dashboard" noIndex />

            <AnimatePresence>
                {editingLink && (
                    <EditModal
                        link={editingLink}
                        onClose={() => setEditingLink(null)}
                        onSave={handleUpdate}
                        burnEnabled={appSettings.burn_after_reading_enabled}
                        interstitialEnabled={appSettings.safe_link_interstitial_enabled}
                        routingEnabled={appSettings.conditional_routing_enabled}
                        bioEnabled={appSettings.link_in_bio_enabled}
                    />
                )}
                {qrLink && (
                    <QRModal link={qrLink} onClose={() => setQrLink(null)} brandingEnabled={appSettings.qr_branding_enabled} />
                )}
                {shareLink && (
                    <ShareModal
                        url={`${shortLinkBase()}/${shareLink.code}`}
                        title={shareLink.title || `Check out this link`}
                        onClose={() => setShareLink(null)}
                    />
                )}
                {previewLink && (
                    <PreviewModal
                        link={previewLink}
                        sparkline={sparklineData[previewLink.id]}
                        onClose={() => setPreviewLink(null)}
                    />
                )}
            </AnimatePresence>

            {/* Header with Stats */}
            <div className="flex flex-col sm:flex-row sm:items-end sm:justify-between gap-4 mb-8">
                <div>
                    <h1 className="font-display text-3xl sm:text-4xl font-extrabold text-ink tracking-tight">Dashboard</h1>
                    <div className="flex items-center gap-4 mt-3 text-sm text-muted">
                        <span className="inline-flex items-center gap-1.5">
                            <Link2 className="h-4 w-4 text-faint" />
                            {linkCount.toLocaleString()} {pluralize(linkCount, 'link')}
                        </span>
                        {activeLinks !== null && (
                            <span className="inline-flex items-center gap-1.5">
                                <Zap className="h-4 w-4 text-success" />
                                {activeLinks} active
                            </span>
                        )}
                        {totalClicks !== null && (
                            <span className="inline-flex items-center gap-1.5">
                                <MousePointer className="h-4 w-4 text-faint" />
                                {totalClicks.toLocaleString()} {pluralize(totalClicks, 'click')}
                            </span>
                        )}
                        <LiveClickIndicator status={live.status} />
                    </div>
                </div>
                <div className="flex gap-2">
                    <button
                        onClick={() => {
                            setBulkReport(null);
                            setShowBulkImport(true);
                        }}
                        className="inline-flex items-center gap-2 rounded-lg border border-line2 bg-surface px-4 py-2 text-sm font-medium text-ink transition-colors hover:border-ink/30"
                    >
                        <Upload className="h-4 w-4 text-muted" />
                        Bulk Import
                    </button>
                    <button
                        onClick={handleExport}
                        className="inline-flex items-center gap-2 rounded-lg border border-line2 bg-surface px-4 py-2 text-sm font-medium text-ink transition-colors hover:border-ink/30"
                    >
                        <Download className="h-4 w-4 text-muted" />
                        Export CSV
                    </button>
                </div>
            </div>

            {/* Bulk Import Modal */}
            <AnimatePresence>
                {showBulkImport && (
                    <BulkImportModal
                        urls={bulkUrls}
                        onUrlsChange={setBulkUrls}
                        importing={bulkImporting}
                        report={bulkReport}
                        onImport={handleBulkImport}
                        onClose={() => setShowBulkImport(false)}
                    />
                )}
            </AnimatePresence>

            {error && (
                <motion.div
                    initial={{ opacity: 0, y: -10 }}
                    animate={{ opacity: 1, y: 0 }}
                    role="alert"
                    className="mb-6 flex items-center justify-between gap-3 rounded-xl border border-danger/30 bg-danger/5 px-4 py-3 text-sm text-danger"
                >
                    {error}
                    <button onClick={() => setError('')} aria-label="Dismiss error" className="text-danger/70 transition-colors hover:text-danger">
                        <X className="h-4 w-4" />
                    </button>
                </motion.div>
            )}

            {/* Create Link Form */}
            <CreateLinkForm
                appSettings={appSettings}
                initialUrl={createInitialUrl}
                onCreated={() => { fetchLinks(); }}
                onError={setError}
            />

            {/* Search and Filter */}
            {(links.length > 0 || accountTotal > 0) && (
                <LinkFilters
                    searchQuery={searchQuery}
                    onSearchQueryChange={setSearchQuery}
                    sortBy={sortBy}
                    onSortByChange={setSortBy}
                    sortOrder={sortOrder}
                    onSortOrderChange={setSortOrder}
                />
            )}

            {/* Links List */}
            <div className={selectedIds.size > 0 ? 'pb-24 sm:pb-0' : undefined}>
                {paginatedLinks.length > 0 && (
                    <div className="flex items-center gap-2 py-3">
                        <SelectAllCheckbox
                            checked={allOnPage}
                            indeterminate={selectedOnPage > 0 && !allOnPage}
                            onChange={toggleSelectPage}
                        />
                        <span className="text-sm text-muted">Select all on this page</span>
                    </div>
                )}
                {selectedIds.size > 0 && (
                    <BulkActionBar
                        count={selectedIds.size}
                        busy={bulkBusy}
                        onDelete={() => applyBulk('delete', {})}
                        onSetExpiry={(expiresAt) => applyBulk('update', { expires_at: expiresAt })}
                        onRemoveExpiry={() => applyBulk('update', { remove_expiration: true })}
                    />
                )}
                <div className="divide-y divide-line border-y border-line">
                    <AnimatePresence mode="popLayout">
                        {paginatedLinks.map((link, index) => {
                            // Calculate link number (bottom to top, 1 = oldest)
                            const linkNumber = filteredLinks.length + unloaded - ((currentPage - 1) * LINKS_PER_PAGE + index);
                            return (
                                <LinkRow
                                    key={link.id}
                                    link={link}
                                    linkNumber={linkNumber}
                                    sparkline={sparklineData[link.id]}
                                    copied={copiedId === link.id}
                                    sourceCopied={copiedSourceId === link.id}
                                    cloning={cloningIds.has(link.id)}
                                    selected={selectedIds.has(link.id)}
                                    clickFlash={live.flashNonce(link.id)}
                                    onToggleSelect={toggleSelected}
                                    onCopy={handleCopy}
                                    onCopySource={handleCopySource}
                                    onPreview={setPreviewLink}
                                    onPin={handlePin}
                                    onClone={handleClone}
                                    onShare={handleShare}
                                    onQr={setQrLink}
                                    onEdit={setEditingLink}
                                    onDelete={handleDelete}
                                />
                            );
                        })}
                    </AnimatePresence>
                </div>

                {/* Pagination */}
                {(totalPages > 1 || hasMore) && (
                    <div className="flex flex-wrap items-center justify-between gap-3 pt-5 mt-1">
                        <p className="text-sm text-faint" aria-live="polite">
                            {hasMore ? (
                                <>
                                    Showing {links.length.toLocaleString()} of {shownTotal.toLocaleString()} links
                                    {(sortBy !== 'date' || sortOrder !== 'desc') && '. Sorting covers the links loaded so far.'}
                                </>
                            ) : (
                                <>
                                    Showing {((currentPage - 1) * LINKS_PER_PAGE) + 1}-{Math.min(currentPage * LINKS_PER_PAGE, filteredLinks.length)} of {filteredLinks.length}
                                </>
                            )}
                        </p>
                        <div className="flex items-center gap-2">
                            {totalPages > 1 && (
                                <>
                                    <button
                                        onClick={() => setCurrentPage(p => Math.max(1, p - 1))}
                                        disabled={currentPage === 1}
                                        aria-label="Previous page"
                                        className="rounded-lg border border-line2 bg-surface p-2 text-muted transition-colors hover:text-ink hover:border-ink/30 disabled:opacity-50 disabled:cursor-not-allowed"
                                    >
                                        <ChevronLeft className="h-4 w-4" />
                                    </button>
                                    <span className="px-3 py-1 font-mono text-sm font-medium text-ink tabular-nums">
                                        {currentPage} / {totalPages}
                                    </span>
                                    <button
                                        onClick={() => setCurrentPage(p => Math.min(totalPages, p + 1))}
                                        disabled={currentPage === totalPages}
                                        aria-label="Next page"
                                        className="rounded-lg border border-line2 bg-surface p-2 text-muted transition-colors hover:text-ink hover:border-ink/30 disabled:opacity-50 disabled:cursor-not-allowed"
                                    >
                                        <ChevronRight className="h-4 w-4" />
                                    </button>
                                </>
                            )}
                            {hasMore && (
                                <button
                                    onClick={loadMore}
                                    disabled={loadingMore}
                                    className="rounded-lg border border-line2 bg-surface px-4 py-2 text-sm font-medium text-ink transition-colors hover:border-ink/30 disabled:opacity-50 disabled:cursor-not-allowed"
                                >
                                    {loadingMore ? 'Loading…' : 'Load more'}
                                </button>
                            )}
                        </div>
                    </div>
                )}

                {filteredLinks.length === 0 && searchQuery && (
                    <div className="py-12 text-center text-muted">
                        No links found matching "{searchQuery}"
                    </div>
                )}

                {links.length === 0 && accountTotal === 0 && !error && (
                    <motion.div
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        className="rounded-2xl border border-dashed border-line2 bg-paper py-16 text-center"
                    >
                        <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-full border border-line bg-surface">
                            <Plus className="h-6 w-6 text-faint" />
                        </div>
                        <h3 className="font-display text-lg font-bold text-ink">No links yet</h3>
                        <p className="mt-1 text-muted">Create your first shortened link above.</p>
                        <button
                            type="button"
                            onClick={() => {
                                const input = document.querySelector('input[placeholder*="example.com"]') as HTMLInputElement;
                                if (input) input.focus();
                            }}
                            className="mt-5 inline-flex items-center gap-1.5 text-sm font-medium text-primary-600 transition-colors hover:text-primary-700"
                        >
                            Start shortening <ArrowRight className="h-4 w-4" />
                        </button>
                    </motion.div>
                )}
            </div>
        </div>
    );
}
