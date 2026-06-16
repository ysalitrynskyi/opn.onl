import { useEffect, useState, useMemo, useCallback } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import {
    Copy, Plus, Trash2, BarChart2,
    QrCode, Download, Lock, Clock, Edit2, X, Check,
    Search, ChevronDown, Calendar, ChevronLeft, ChevronRight,
    MousePointer, SortAsc, SortDesc,
    Zap, Link2, Share2, Upload, Clipboard, Pin, CopyPlus,
    Eye, ArrowRight, Flame, ShieldCheck
} from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { API_ENDPOINTS, authFetch } from '../config/api';
import SEO from '../components/SEO';
import logger from '../utils/logger';
import { toast } from '../components/Toast';
import ShareModal from '../components/ShareModal';
import { useKeyboardShortcuts } from '../hooks/useKeyboardShortcuts';
import Sparkline from '../components/Sparkline';
import LinkPreviewCard from '../components/LinkPreviewCard';
import EditModal from '../components/dashboard/EditModal';
import QRModal from '../components/dashboard/QRModal';
import Skeleton from '../components/dashboard/Skeleton';
import MiniStats from '../components/dashboard/MiniStats';
import type { LinkData, LinkUpdatePayload } from '../components/dashboard/types';

interface AppSettings {
    custom_aliases_enabled: boolean;
    min_alias_length: number;
    max_alias_length: number;
    qr_branding_enabled: boolean;
    burn_after_reading_enabled: boolean;
    safe_link_interstitial_enabled: boolean;
}

const LINKS_PER_PAGE = 20;

const ease = [0.16, 1, 0.3, 1] as const;

export default function Dashboard() {
    const [links, setLinks] = useState<LinkData[]>([]);
    const [loading, setLoading] = useState(true);
    const [newUrl, setNewUrl] = useState('');
    const [alias, setAlias] = useState('');
    const [title, setTitle] = useState('');
    const [password, setPassword] = useState('');
    const [expiresAt, setExpiresAt] = useState('');
    const [expiresTime, setExpiresTime] = useState('23:59');
    const [creating, setCreating] = useState(false);
    const [showAdvanced, setShowAdvanced] = useState(false);
    const [searchQuery, setSearchQuery] = useState('');
    const [editingLink, setEditingLink] = useState<LinkData | null>(null);
    const [qrLink, setQrLink] = useState<LinkData | null>(null);
    const [shareLink, setShareLink] = useState<LinkData | null>(null);
    const [copiedId, setCopiedId] = useState<number | null>(null);
    const [copiedSourceId, setCopiedSourceId] = useState<number | null>(null);
    const [error, setError] = useState('');
    const [currentPage, setCurrentPage] = useState(1);
    const [sortBy, setSortBy] = useState<'date' | 'clicks'>('date');
    const [sortOrder, setSortOrder] = useState<'asc' | 'desc'>('desc');
    const [appSettings, setAppSettings] = useState<AppSettings>({
        custom_aliases_enabled: true,
        min_alias_length: 5,
        max_alias_length: 50,
        qr_branding_enabled: true,
        burn_after_reading_enabled: false,
        safe_link_interstitial_enabled: false
    });
    const [burnAfterReading, setBurnAfterReading] = useState(false);
    const [safeLinkInterstitial, setSafeLinkInterstitial] = useState(false);
    const [sparklineData, setSparklineData] = useState<Record<number, { data: number[]; labels: string[] }>>({});
    const [previewLink, setPreviewLink] = useState<LinkData | null>(null);
    const [showBulkImport, setShowBulkImport] = useState(false);
    const [bulkUrls, setBulkUrls] = useState('');
    const [bulkImporting, setBulkImporting] = useState(false);
    const [clipboardUrl, setClipboardUrl] = useState<string | null>(null);
    const navigate = useNavigate();

    useEffect(() => {
        const token = localStorage.getItem('token');
        if (!token) {
            navigate('/login');
            return;
        }
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

    // Check clipboard for URL on focus
    useEffect(() => {
        let isMounted = true;

        const checkClipboard = async () => {
            try {
                const text = await navigator.clipboard.readText();
                // Only update state if component is still mounted
                if (!isMounted) return;

                if (text && /^https?:\/\/.+/.test(text.trim())) {
                    setClipboardUrl(text.trim());
                } else {
                    setClipboardUrl(null);
                }
            } catch {
                // Clipboard access denied or not available
                if (isMounted) {
                    setClipboardUrl(null);
                }
            }
        };

        window.addEventListener('focus', checkClipboard);
        checkClipboard();

        return () => {
            isMounted = false;
            window.removeEventListener('focus', checkClipboard);
        };
    }, []);

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
                setEditingLink(null);
                setQrLink(null);
                setShareLink(null);
                setSearchQuery('');
            },
            description: 'Close modal / Clear search',
        },
    ]);

    // Bulk import handler
    const handleBulkImport = async () => {
        const urls = bulkUrls.split('\n').map(u => u.trim()).filter(u => u && /^https?:\/\/.+/.test(u));

        if (urls.length === 0) {
            setError('No valid URLs found. Each line should contain a valid URL starting with http:// or https://');
            return;
        }

        setBulkImporting(true);
        setError('');

        try {
            const res = await authFetch(API_ENDPOINTS.bulkLinks, {
                method: 'POST',
                body: JSON.stringify({ urls }),
            });

            if (res.ok) {
                const data = await res.json();
                setShowBulkImport(false);
                setBulkUrls('');
                fetchLinks();

                if (data.errors && data.errors.length > 0) {
                    setError(`Created ${data.links.length} links. ${data.errors.length} failed: ${data.errors.slice(0, 3).join(', ')}${data.errors.length > 3 ? '...' : ''}`);
                }
            } else {
                const data = await res.json();
                setError(data.error || 'Failed to import links');
            }
        } catch {
            setError('Network error during import');
        } finally {
            setBulkImporting(false);
        }
    };

    // Quick create from clipboard
    const handleClipboardCreate = () => {
        if (clipboardUrl) {
            setNewUrl(clipboardUrl);
            setClipboardUrl(null);
        }
    };

    // Filter and sort links
    const filteredLinks = useMemo(() => {
        let result = [...links];

        // Search filter
        if (searchQuery) {
            const query = searchQuery.toLowerCase();
            result = result.filter(link =>
                link.code.toLowerCase().includes(query) ||
                link.original_url.toLowerCase().includes(query) ||
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
    }, [links, searchQuery, sortBy, sortOrder]);

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

    const fetchLinks = async () => {
        try {
            const res = await authFetch(API_ENDPOINTS.links);
            if (res.ok) {
                const data = await res.json();
                setLinks(data);
            }
        } catch (error) {
            logger.error('Failed to fetch links', error);
            setError('Failed to load links. Please try again.');
        } finally {
            setLoading(false);
        }
    };

    // Fetch sparkline data for all links
    const fetchSparklines = useCallback(async (linkIds: number[]) => {
        if (linkIds.length === 0) return;
        try {
            const res = await authFetch(`${API_ENDPOINTS.sparklines}?ids=${linkIds.join(',')}`);
            if (res.ok) {
                const data = await res.json();
                const sparklines: Record<number, { data: number[]; labels: string[] }> = {};
                for (const item of data.sparklines) {
                    sparklines[item.link_id] = { data: item.data, labels: item.labels };
                }
                setSparklineData(sparklines);
            }
        } catch (error) {
            logger.error('Failed to fetch sparklines', error);
        }
    }, []);

    // Fetch sparklines when links change
    useEffect(() => {
        if (links.length > 0) {
            const linkIds = links.map(l => l.id);
            fetchSparklines(linkIds);
        }
    }, [links, fetchSparklines]);

    const handleCreate = async (e: React.FormEvent) => {
        e.preventDefault();
        setCreating(true);
        setError('');

        // Validate alias length if provided
        if (alias) {
            if (alias.length < appSettings.min_alias_length) {
                setError(`Alias must be at least ${appSettings.min_alias_length} characters`);
                setCreating(false);
                return;
            }
            if (alias.length > appSettings.max_alias_length) {
                setError(`Alias must be at most ${appSettings.max_alias_length} characters`);
                setCreating(false);
                return;
            }
            // Check for valid characters
            if (!/^[a-zA-Z0-9_-]+$/.test(alias)) {
                setError('Alias can only contain letters, numbers, hyphens, and underscores');
                setCreating(false);
                return;
            }
            // Check for leading/trailing special chars
            if (/^[-_]|[-_]$/.test(alias)) {
                setError('Alias cannot start or end with hyphen or underscore');
                setCreating(false);
                return;
            }
        }

        try {
            // Build expiration datetime with timezone
            let expirationDate: string | undefined;
            if (expiresAt) {
                const dateTime = `${expiresAt}T${expiresTime || '23:59'}:00`;
                expirationDate = new Date(dateTime).toISOString();
            }

            const res = await authFetch(API_ENDPOINTS.links, {
                method: 'POST',
                body: JSON.stringify({
                    original_url: newUrl,
                    custom_alias: alias || undefined,
                    title: title || undefined,
                    password: password || undefined,
                    expires_at: expirationDate,
                    burn_after_reading: burnAfterReading || undefined,
                    safe_link_interstitial: safeLinkInterstitial || undefined,
                }),
            });

            if (res.ok) {
                setNewUrl('');
                setAlias('');
                setTitle('');
                setPassword('');
                setExpiresAt('');
                setExpiresTime('23:59');
                setBurnAfterReading(false);
                setSafeLinkInterstitial(false);
                setShowAdvanced(false);
                fetchLinks();
            } else {
                const data = await res.json();
                setError(data.error || 'Failed to create link');
            }
        } catch (error) {
            logger.error('Failed to create link', error);
            setError('Network error. Please try again.');
        } finally {
            setCreating(false);
        }
    };

    const handleDelete = async (id: number) => {
        if (!confirm('Are you sure you want to delete this link?')) return;

        try {
            const res = await authFetch(API_ENDPOINTS.linkDelete(id), {
                method: 'DELETE',
            });
            if (res.ok) {
                setLinks(links.filter(l => l.id !== id));
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
            if (res.ok) {
                fetchLinks();
            } else {
                setError('Failed to update link');
            }
        } catch (error) {
            logger.error('Failed to update link', error);
            setError('Network error. Please try again.');
        }
    };

    const handleCopy = async (link: LinkData) => {
        try {
            const mainUrl = `${import.meta.env.VITE_FRONTEND_URL || window.location.origin}/${link.code}`;
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
                setLinks(links.map(l => l.id === link.id ? { ...l, is_pinned: data.is_pinned } : l));
                toast(data.message, 'success');
            } else {
                toast('Failed to update pin status', 'error');
            }
        } catch {
            toast('Network error', 'error');
        }
    };

    const handleClone = async (link: LinkData) => {
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
            const mainUrl = `${import.meta.env.VITE_FRONTEND_URL || window.location.origin}/${link.code}`;
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

    const totalClicks = links.reduce((sum, link) => sum + link.click_count, 0);
    const activeLinks = links.filter(l => l.is_active).length;

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
                    />
                )}
                {qrLink && (
                    <QRModal link={qrLink} onClose={() => setQrLink(null)} brandingEnabled={appSettings.qr_branding_enabled} />
                )}
                {shareLink && (
                    <ShareModal
                        url={`${import.meta.env.VITE_FRONTEND_URL || window.location.origin}/${shareLink.code}`}
                        title={shareLink.title || `Check out this link`}
                        onClose={() => setShareLink(null)}
                    />
                )}
                {previewLink && (
                    <motion.div
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                        className="fixed inset-0 bg-ink/40 backdrop-blur-sm flex items-center justify-center z-50 p-4"
                        onClick={() => setPreviewLink(null)}
                    >
                        <motion.div
                            initial={{ scale: 0.97, opacity: 0, y: 8 }}
                            animate={{ scale: 1, opacity: 1, y: 0 }}
                            exit={{ scale: 0.97, opacity: 0, y: 8 }}
                            transition={{ duration: 0.18, ease }}
                            className="bg-surface rounded-2xl border border-line2 shadow-lift max-w-md w-full p-6"
                            onClick={e => e.stopPropagation()}
                        >
                            <div className="flex items-center justify-between mb-5">
                                <h3 className="font-display text-lg font-bold text-ink tracking-tight">Link preview</h3>
                                <button onClick={() => setPreviewLink(null)} aria-label="Close" className="text-faint transition-colors hover:text-ink">
                                    <X className="h-5 w-5" />
                                </button>
                            </div>
                            <div className="mb-4">
                                <p className="font-mono text-xs uppercase tracking-[0.14em] text-faint mb-2">Short URL</p>
                                <code className="rounded-md bg-paper border border-line px-2 py-1 font-mono text-sm text-primary-600">
                                    {previewLink.short_url}
                                </code>
                            </div>
                            <div className="mb-4">
                                <p className="font-mono text-xs uppercase tracking-[0.14em] text-faint mb-2">Destination preview</p>
                                <LinkPreviewCard url={previewLink.original_url} />
                            </div>
                            {sparklineData[previewLink.id] && (
                                <div className="border-t border-line pt-4">
                                    <p className="font-mono text-xs uppercase tracking-[0.14em] text-faint mb-2">Clicks (last 7 days)</p>
                                    <div className="rounded-lg border border-line bg-paper p-3">
                                        <Sparkline
                                            data={sparklineData[previewLink.id].data}
                                            labels={sparklineData[previewLink.id].labels}
                                            width={320}
                                            height={60}
                                            strokeWidth={2}
                                        />
                                    </div>
                                </div>
                            )}
                        </motion.div>
                    </motion.div>
                )}
            </AnimatePresence>

            {/* Header with Stats */}
            <div className="flex flex-col sm:flex-row sm:items-end sm:justify-between gap-4 mb-8">
                <div>
                    <h1 className="font-display text-3xl sm:text-4xl font-extrabold text-ink tracking-tight">Dashboard</h1>
                    <div className="flex items-center gap-4 mt-3 text-sm text-muted">
                        <span className="inline-flex items-center gap-1.5">
                            <Link2 className="h-4 w-4 text-faint" />
                            {links.length} links
                        </span>
                        <span className="inline-flex items-center gap-1.5">
                            <Zap className="h-4 w-4 text-success" />
                            {activeLinks} active
                        </span>
                        <span className="inline-flex items-center gap-1.5">
                            <MousePointer className="h-4 w-4 text-faint" />
                            {totalClicks.toLocaleString()} clicks
                        </span>
                    </div>
                </div>
                <div className="flex gap-2">
                    <button
                        onClick={() => setShowBulkImport(true)}
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

            {/* Clipboard URL Suggestion */}
            {clipboardUrl && !newUrl && (
                <motion.div
                    initial={{ opacity: 0, y: -10 }}
                    animate={{ opacity: 1, y: 0 }}
                    exit={{ opacity: 0, y: -10 }}
                    className="mb-6 flex items-center justify-between gap-4 rounded-xl border border-line2 bg-surface px-4 py-3 shadow-subtle"
                >
                    <div className="flex items-center gap-3 min-w-0">
                        <Clipboard className="h-5 w-5 shrink-0 text-primary-600" />
                        <div className="min-w-0">
                            <p className="text-sm font-medium text-ink">URL detected in clipboard</p>
                            <p className="truncate font-mono text-xs text-faint max-w-md">{clipboardUrl}</p>
                        </div>
                    </div>
                    <div className="flex items-center gap-2 shrink-0">
                        <button
                            onClick={() => setClipboardUrl(null)}
                            aria-label="Dismiss"
                            className="p-1 text-faint transition-colors hover:text-ink"
                        >
                            <X className="h-4 w-4" />
                        </button>
                        <button
                            onClick={handleClipboardCreate}
                            className="rounded-lg bg-primary-600 px-3 py-1.5 text-sm font-semibold text-white transition-colors hover:bg-primary-700"
                        >
                            Shorten it
                        </button>
                    </div>
                </motion.div>
            )}

            {/* Bulk Import Modal */}
            <AnimatePresence>
                {showBulkImport && (
                    <motion.div
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        exit={{ opacity: 0 }}
                        className="fixed inset-0 bg-ink/40 backdrop-blur-sm flex items-center justify-center z-50 p-4"
                        onClick={() => setShowBulkImport(false)}
                    >
                        <motion.div
                            initial={{ scale: 0.97, opacity: 0, y: 8 }}
                            animate={{ scale: 1, opacity: 1, y: 0 }}
                            exit={{ scale: 0.97, opacity: 0, y: 8 }}
                            transition={{ duration: 0.18, ease }}
                            className="bg-surface rounded-2xl border border-line2 shadow-lift max-w-lg w-full p-6"
                            onClick={e => e.stopPropagation()}
                        >
                            <div className="flex items-center justify-between mb-4">
                                <h3 className="font-display text-xl font-bold text-ink tracking-tight">Bulk import URLs</h3>
                                <button onClick={() => setShowBulkImport(false)} aria-label="Close" className="text-faint transition-colors hover:text-ink">
                                    <X className="h-5 w-5" />
                                </button>
                            </div>
                            <p className="text-sm text-muted mb-4">
                                Paste one URL per line. Each URL will be shortened automatically.
                            </p>
                            <textarea
                                value={bulkUrls}
                                onChange={(e) => setBulkUrls(e.target.value)}
                                className="w-full h-48 rounded-lg border border-line2 bg-surface px-4 py-3 font-mono text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint"
                                placeholder="https://example.com/page1&#10;https://example.com/page2&#10;https://example.com/page3"
                            />
                            <div className="flex justify-between items-center mt-4">
                                <span className="text-sm text-faint">
                                    {bulkUrls.split('\n').filter(u => u.trim() && /^https?:\/\/.+/.test(u.trim())).length} valid URLs
                                </span>
                                <div className="flex gap-3">
                                    <button
                                        onClick={() => setShowBulkImport(false)}
                                        className="rounded-lg border border-line2 px-4 py-2 font-medium text-muted transition-colors hover:text-ink hover:border-ink/30"
                                    >
                                        Cancel
                                    </button>
                                    <button
                                        onClick={handleBulkImport}
                                        disabled={bulkImporting}
                                        className="inline-flex items-center gap-2 rounded-lg bg-primary-600 px-4 py-2 font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-70"
                                    >
                                        {bulkImporting ? (
                                            <div className="h-4 w-4 rounded-full border-2 border-white/30 border-t-white animate-spin" />
                                        ) : (
                                            <Upload className="h-4 w-4" />
                                        )}
                                        Import All
                                    </button>
                                </div>
                            </div>
                        </motion.div>
                    </motion.div>
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
            <div className="rounded-2xl border border-line2 bg-surface p-6 shadow-subtle mb-8">
                <h2 className="font-display text-lg font-bold text-ink tracking-tight mb-4">Create new link</h2>
                <form onSubmit={handleCreate}>
                    <div className="flex flex-col sm:flex-row gap-3">
                        <input
                            type="url"
                            required
                            placeholder="https://example.com/long-url"
                            className="flex-1 rounded-lg border border-line2 bg-surface px-4 py-2.5 font-mono text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint"
                            value={newUrl}
                            onChange={(e) => setNewUrl(e.target.value)}
                        />
                        {appSettings.custom_aliases_enabled && (
                            <input
                                type="text"
                                placeholder={`alias (${appSettings.min_alias_length}-${appSettings.max_alias_length} chars)`}
                                className="sm:w-48 rounded-lg border border-line2 bg-surface px-4 py-2.5 font-mono text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint"
                                value={alias}
                                onChange={(e) => setAlias(e.target.value.replace(/[^a-zA-Z0-9_-]/g, ''))}
                                minLength={appSettings.min_alias_length}
                                maxLength={appSettings.max_alias_length}
                            />
                        )}
                        <button
                            type="submit"
                            disabled={creating}
                            className="inline-flex items-center justify-center gap-2 rounded-lg bg-primary-600 px-6 py-2.5 font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-70"
                        >
                            {creating ? (
                                <div className="h-4 w-4 rounded-full border-2 border-white/30 border-t-white animate-spin" />
                            ) : (
                                <Plus className="h-4 w-4" />
                            )}
                            Create
                        </button>
                    </div>

                    <button
                        type="button"
                        onClick={() => setShowAdvanced(!showAdvanced)}
                        className="mt-3 inline-flex items-center gap-1 text-sm text-muted transition-colors hover:text-ink"
                    >
                        <ChevronDown className={`h-4 w-4 transition-transform ${showAdvanced ? 'rotate-180' : ''}`} />
                        Advanced options
                    </button>

                    <AnimatePresence>
                        {showAdvanced && (
                            <motion.div
                                initial={{ height: 0, opacity: 0 }}
                                animate={{ height: 'auto', opacity: 1 }}
                                exit={{ height: 0, opacity: 0 }}
                                className="overflow-hidden"
                            >
                                <div className="space-y-4 mt-4 pt-4 border-t border-line">
                                    {/* Title - full width */}
                                    <div>
                                        <label htmlFor="create-title" className="block font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5">
                                            Title (private, only visible to you)
                                        </label>
                                        <input
                                            id="create-title"
                                            type="text"
                                            placeholder="e.g. Marketing campaign Q1"
                                            className="w-full rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint"
                                            value={title}
                                            onChange={(e) => setTitle(e.target.value)}
                                            maxLength={100}
                                        />
                                    </div>

                                    <div className="grid sm:grid-cols-2 gap-4">
                                        <div>
                                            <label htmlFor="create-password" className="flex items-center gap-1.5 font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5">
                                                <Lock className="h-3.5 w-3.5" />
                                                Password Protection
                                            </label>
                                            <input
                                                id="create-password"
                                                type="password"
                                                placeholder="Leave empty for no password"
                                                className="w-full rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint"
                                                value={password}
                                                onChange={(e) => setPassword(e.target.value)}
                                            />
                                        </div>
                                        <div>
                                            <label htmlFor="create-expires" className="flex items-center gap-1.5 font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5">
                                                <Calendar className="h-3.5 w-3.5" />
                                                Expiration
                                            </label>
                                            <div className="flex gap-2">
                                                <input
                                                    id="create-expires"
                                                    type="date"
                                                    className="flex-1 rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500"
                                                    value={expiresAt}
                                                    onChange={(e) => setExpiresAt(e.target.value)}
                                                    min={new Date().toISOString().split('T')[0]}
                                                />
                                                <input
                                                    type="time"
                                                    aria-label="Expiration time"
                                                    className="w-28 rounded-lg border border-line2 bg-surface px-3 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500"
                                                    value={expiresTime}
                                                    onChange={(e) => setExpiresTime(e.target.value)}
                                                />
                                            </div>
                                            <p className="text-xs text-faint mt-1.5">
                                                Your timezone: {Intl.DateTimeFormat().resolvedOptions().timeZone}
                                                {expiresAt && expiresTime && (
                                                    <span className="ml-2">
                                                        → Expires: {new Date(`${expiresAt}T${expiresTime}`).toLocaleString()}
                                                    </span>
                                                )}
                                            </p>
                                        </div>
                                    </div>

                                    {appSettings.burn_after_reading_enabled && (
                                        <label className="flex items-center gap-2.5 text-sm text-muted cursor-pointer">
                                            <input
                                                type="checkbox"
                                                checked={burnAfterReading}
                                                onChange={(e) => setBurnAfterReading(e.target.checked)}
                                                className="h-4 w-4 rounded border-line2 text-primary-600 focus:ring-primary-500"
                                            />
                                            <span className="inline-flex items-center gap-1.5">
                                                <Flame className="h-3.5 w-3.5 text-primary-600" />
                                                Burn after reading — opens once, then self-destructs
                                            </span>
                                        </label>
                                    )}

                                    {appSettings.safe_link_interstitial_enabled && (
                                        <label className="flex items-center gap-2.5 text-sm text-muted cursor-pointer">
                                            <input
                                                type="checkbox"
                                                checked={safeLinkInterstitial}
                                                onChange={(e) => setSafeLinkInterstitial(e.target.checked)}
                                                className="h-4 w-4 rounded border-line2 text-primary-600 focus:ring-primary-500"
                                            />
                                            <span className="inline-flex items-center gap-1.5">
                                                <ShieldCheck className="h-3.5 w-3.5 text-primary-600" />
                                                Show a safety interstitial before redirecting
                                            </span>
                                        </label>
                                    )}
                                </div>
                            </motion.div>
                        )}
                    </AnimatePresence>
                </form>
            </div>

            {/* Search and Filter */}
            {links.length > 0 && (
                <div className="mb-6 flex flex-col sm:flex-row gap-3">
                    <div className="relative flex-1">
                        <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-faint" />
                        <input
                            type="text"
                            placeholder="Search links, notes, tags..."
                            className="w-full rounded-lg border border-line2 bg-surface pl-10 pr-4 py-2.5 text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint"
                            value={searchQuery}
                            onChange={e => setSearchQuery(e.target.value)}
                        />
                    </div>
                    <div className="flex gap-2">
                        <select
                            value={sortBy}
                            onChange={(e) => setSortBy(e.target.value as 'date' | 'clicks')}
                            aria-label="Sort links by"
                            className="rounded-lg border border-line2 bg-surface px-3 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500"
                        >
                            <option value="date">Sort by Date</option>
                            <option value="clicks">Sort by Clicks</option>
                        </select>
                        <button
                            onClick={() => setSortOrder(sortOrder === 'asc' ? 'desc' : 'asc')}
                            className="rounded-lg border border-line2 bg-surface p-2.5 text-muted transition-colors hover:text-ink hover:border-ink/30"
                            title={sortOrder === 'asc' ? 'Ascending' : 'Descending'}
                            aria-label={sortOrder === 'asc' ? 'Ascending' : 'Descending'}
                        >
                            {sortOrder === 'asc' ? <SortAsc className="h-4 w-4" /> : <SortDesc className="h-4 w-4" />}
                        </button>
                    </div>
                </div>
            )}

            {/* Links List */}
            <div>
                <div className="divide-y divide-line border-y border-line">
                    <AnimatePresence mode="popLayout">
                        {paginatedLinks.map((link, index) => {
                            // Calculate link number (bottom to top, 1 = oldest)
                            const linkNumber = filteredLinks.length - ((currentPage - 1) * LINKS_PER_PAGE + index);
                            const mainUrl = `${import.meta.env.VITE_FRONTEND_URL || window.location.origin}/${link.code}`;
                            const apiUrl = link.api_url || link.short_url; // Use api_url from backend

                            return (
                            <motion.div
                                key={link.id}
                                layout
                                initial={{ opacity: 0, y: 16 }}
                                animate={{ opacity: 1, y: 0 }}
                                exit={{ opacity: 0, scale: 0.98 }}
                                className="group py-5 transition-colors hover:bg-primary-50/40"
                            >
                                <div className="flex flex-col sm:flex-row sm:items-start justify-between gap-4">
                                    <div className="space-y-1 min-w-0 flex-1">
                                        {/* Link number and title */}
                                        <div className="flex items-center gap-2 mb-1">
                                            <span className="font-mono text-xs font-semibold text-faint">
                                                #{linkNumber}
                                            </span>
                                            {link.is_pinned && (
                                                <Pin className="h-3 w-3 text-primary-600 fill-current" aria-label="Pinned" />
                                            )}
                                            {link.title && (
                                                <span className="text-sm font-medium text-ink truncate">
                                                    {link.title}
                                                </span>
                                            )}
                                        </div>

                                        {/* Primary short URL → Destination */}
                                        <div className="flex items-center gap-2 flex-wrap">
                                            <a
                                                href={mainUrl}
                                                target="_blank"
                                                rel="noreferrer"
                                                className="font-mono text-lg font-bold text-primary-600 hover:underline"
                                            >
                                                {mainUrl.replace(/^https?:\/\//, '')}
                                            </a>
                                            <button
                                                onClick={() => handleCopy(link)}
                                                className="rounded p-1 text-faint transition-colors hover:bg-primary-50 hover:text-primary-600"
                                                title="Copy short URL"
                                                aria-label="Copy short URL"
                                            >
                                                {copiedId === link.id ? (
                                                    <Check className="h-4 w-4 text-success" />
                                                ) : (
                                                    <Copy className="h-4 w-4" />
                                                )}
                                            </button>
                                            <span className="text-faint" aria-hidden="true">→</span>
                                            <a
                                                href={link.original_url}
                                                target="_blank"
                                                rel="noreferrer"
                                                className="truncate max-w-[300px] font-mono text-sm text-muted transition-colors hover:text-ink hover:underline"
                                                title={link.original_url}
                                            >
                                                {link.original_url.replace(/^https?:\/\//, '').substring(0, 50)}{link.original_url.length > 60 ? '...' : ''}
                                            </a>
                                            <button
                                                onClick={() => handleCopySource(link)}
                                                className="rounded p-1 text-faint transition-colors hover:bg-line hover:text-ink"
                                                title="Copy source URL"
                                                aria-label="Copy source URL"
                                            >
                                                {copiedSourceId === link.id ? (
                                                    <Check className="h-4 w-4 text-success" />
                                                ) : (
                                                    <Link2 className="h-4 w-4" />
                                                )}
                                            </button>
                                            {link.has_password && (
                                                <span className="inline-flex items-center gap-1 rounded border border-line px-2 py-0.5 text-xs font-medium text-muted">
                                                    <Lock className="h-3 w-3" />
                                                    Protected
                                                </span>
                                            )}
                                            {link.expires_at && (
                                                <span className="inline-flex items-center gap-1 rounded border border-line px-2 py-0.5 text-xs font-medium text-muted">
                                                    <Clock className="h-3 w-3" />
                                                    {new Date(link.expires_at).toLocaleDateString()}
                                                </span>
                                            )}
                                            {!link.is_active && (
                                                <span className="inline-flex items-center gap-1 rounded border border-danger/30 bg-danger/5 px-2 py-0.5 text-xs font-medium text-danger">
                                                    Inactive
                                                </span>
                                            )}
                                        </div>
                                        {/* API URL */}
                                        <p className="font-mono text-xs text-faint">
                                            API: <a href={apiUrl} target="_blank" rel="noreferrer" className="hover:text-muted hover:underline">{apiUrl}</a>
                                        </p>
                                        {/* Mini stats */}
                                        <MiniStats link={link} />
                                    </div>

                                    <div className="flex items-center gap-2 sm:gap-3">
                                        {/* Sparkline Chart */}
                                        {sparklineData[link.id] && (
                                            <div className="hidden sm:block">
                                                <Sparkline
                                                    data={sparklineData[link.id].data}
                                                    labels={sparklineData[link.id].labels}
                                                    width={70}
                                                    height={24}
                                                />
                                            </div>
                                        )}
                                        <Link
                                            to={`/analytics/${link.id}`}
                                            className="inline-flex items-center gap-1.5 text-sm font-medium text-muted transition-colors hover:text-primary-600"
                                        >
                                            <BarChart2 className="h-4 w-4" />
                                            <span className="font-mono font-bold tabular-nums">{link.click_count.toLocaleString()}</span>
                                        </Link>
                                        <div className="flex items-center gap-0.5">
                                            <button
                                                onClick={() => setPreviewLink(link)}
                                                className="rounded-md p-2 text-faint transition-colors hover:bg-line hover:text-ink"
                                                title="Preview destination"
                                                aria-label="Preview destination"
                                            >
                                                <Eye className="h-4 w-4" />
                                            </button>
                                            <button
                                                onClick={() => handlePin(link)}
                                                className={`rounded-md p-2 transition-colors hover:bg-line ${link.is_pinned ? 'text-primary-600' : 'text-faint hover:text-ink'}`}
                                                title={link.is_pinned ? 'Unpin' : 'Pin'}
                                                aria-label={link.is_pinned ? 'Unpin' : 'Pin'}
                                            >
                                                <Pin className={`h-4 w-4 ${link.is_pinned ? 'fill-current' : ''}`} />
                                            </button>
                                            <button
                                                onClick={() => handleClone(link)}
                                                className="rounded-md p-2 text-faint transition-colors hover:bg-line hover:text-ink"
                                                title="Clone"
                                                aria-label="Clone link"
                                            >
                                                <CopyPlus className="h-4 w-4" />
                                            </button>
                                            <button
                                                onClick={() => handleShare(link)}
                                                className="rounded-md p-2 text-faint transition-colors hover:bg-line hover:text-ink"
                                                title="Share"
                                                aria-label="Share link"
                                            >
                                                <Share2 className="h-4 w-4" />
                                            </button>
                                            <button
                                                onClick={() => setQrLink(link)}
                                                className="rounded-md p-2 text-faint transition-colors hover:bg-line hover:text-ink"
                                                title="QR Code"
                                                aria-label="Show QR code"
                                            >
                                                <QrCode className="h-4 w-4" />
                                            </button>
                                            <button
                                                onClick={() => setEditingLink(link)}
                                                className="rounded-md p-2 text-faint transition-colors hover:bg-line hover:text-ink"
                                                title="Edit"
                                                aria-label="Edit link"
                                            >
                                                <Edit2 className="h-4 w-4" />
                                            </button>
                                            <button
                                                onClick={() => handleDelete(link.id)}
                                                className="rounded-md p-2 text-faint transition-colors hover:bg-danger/5 hover:text-danger"
                                                title="Delete"
                                                aria-label="Delete link"
                                            >
                                                <Trash2 className="h-4 w-4" />
                                            </button>
                                        </div>
                                    </div>
                                </div>
                            </motion.div>
                        );})}
                    </AnimatePresence>
                </div>

                {/* Pagination */}
                {totalPages > 1 && (
                    <div className="flex items-center justify-between pt-5 mt-1">
                        <p className="text-sm text-faint">
                            Showing {((currentPage - 1) * LINKS_PER_PAGE) + 1}-{Math.min(currentPage * LINKS_PER_PAGE, filteredLinks.length)} of {filteredLinks.length}
                        </p>
                        <div className="flex items-center gap-2">
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
                        </div>
                    </div>
                )}

                {filteredLinks.length === 0 && searchQuery && (
                    <div className="py-12 text-center text-muted">
                        No links found matching "{searchQuery}"
                    </div>
                )}

                {links.length === 0 && (
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
