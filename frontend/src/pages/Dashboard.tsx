import { useEffect, useState, useMemo } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { 
    Copy, Plus, Trash2, BarChart2, 
    QrCode, Download, Lock, Clock, Edit2, X, Check,
    Search, ChevronDown, Calendar, ChevronLeft, ChevronRight,
    TrendingUp, MousePointer, SortAsc, SortDesc,
    Zap, Link2, Share2, Upload, Clipboard
} from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { API_ENDPOINTS, getAuthHeaders } from '../config/api';
import SEO from '../components/SEO';
import logger from '../utils/logger';
import { toast } from '../components/Toast';
import ShareModal from '../components/ShareModal';
import { useKeyboardShortcuts } from '../hooks/useKeyboardShortcuts';

interface LinkData {
    id: number;
    code: string;
    original_url: string;
    short_url: string;
    title: string | null;
    click_count: number;
    created_at: string;
    expires_at: string | null;
    has_password: boolean;
    notes: string | null;
    is_active: boolean;
    tags: { id: number; name: string; color: string }[];
}

interface AppSettings {
    custom_aliases_enabled: boolean;
    min_alias_length: number;
    max_alias_length: number;
}

interface EditModalProps {
    link: LinkData;
    onClose: () => void;
    onSave: (id: number, data: any) => Promise<void>;
}

function EditModal({ link, onClose, onSave }: EditModalProps) {
    const [url, setUrl] = useState(link.original_url);
    const [password, setPassword] = useState('');
    const [expiresAt, setExpiresAt] = useState(link.expires_at?.split('T')[0] || '');
    const [removePassword, setRemovePassword] = useState(false);
    const [removeExpiration, setRemoveExpiration] = useState(false);
    const [saving, setSaving] = useState(false);

    const handleSave = async () => {
        setSaving(true);
        await onSave(link.id, {
            original_url: url !== link.original_url ? url : undefined,
            password: password || undefined,
            expires_at: expiresAt && !removeExpiration ? new Date(expiresAt).toISOString() : undefined,
            remove_password: removePassword || undefined,
            remove_expiration: removeExpiration || undefined,
        });
        setSaving(false);
        onClose();
    };

    return (
        <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4"
            onClick={onClose}
        >
            <motion.div
                initial={{ scale: 0.95, opacity: 0 }}
                animate={{ scale: 1, opacity: 1 }}
                exit={{ scale: 0.95, opacity: 0 }}
                className="bg-white rounded-2xl shadow-xl max-w-lg w-full p-6"
                onClick={e => e.stopPropagation()}
            >
                <div className="flex items-center justify-between mb-6">
                    <h3 className="text-xl font-bold text-slate-900">Edit Link</h3>
                    <button onClick={onClose} className="text-slate-400 hover:text-slate-600">
                        <X className="h-5 w-5" />
                    </button>
                </div>

                <div className="space-y-4">
                    <div>
                        <label className="block text-sm font-medium text-slate-700 mb-1">Short URL</label>
                        <div className="px-4 py-2 bg-slate-100 rounded-lg text-slate-600 text-sm font-mono">
                            {link.code}
                        </div>
                    </div>

                    <div>
                        <label className="block text-sm font-medium text-slate-700 mb-1">Destination URL</label>
                        <input
                            type="url"
                            value={url}
                            onChange={(e) => setUrl(e.target.value)}
                            className="w-full rounded-lg border border-slate-300 px-4 py-2 focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                        />
                    </div>

                    <div>
                        <label className="block text-sm font-medium text-slate-700 mb-1">
                            {link.has_password ? 'Change Password' : 'Add Password'}
                        </label>
                        <input
                            type="password"
                            value={password}
                            onChange={(e) => setPassword(e.target.value)}
                            placeholder={link.has_password ? 'Leave empty to keep current' : 'Optional'}
                            className="w-full rounded-lg border border-slate-300 px-4 py-2 focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                        />
                        {link.has_password && (
                            <label className="flex items-center gap-2 mt-2 text-sm text-slate-600">
                                <input
                                    type="checkbox"
                                    checked={removePassword}
                                    onChange={(e) => setRemovePassword(e.target.checked)}
                                    className="rounded border-slate-300"
                                />
                                Remove password protection
                            </label>
                        )}
                    </div>

                    <div>
                        <label className="block text-sm font-medium text-slate-700 mb-1">Expiration Date</label>
                        <input
                            type="date"
                            value={expiresAt}
                            onChange={(e) => setExpiresAt(e.target.value)}
                            className="w-full rounded-lg border border-slate-300 px-4 py-2 focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                            min={new Date().toISOString().split('T')[0]}
                        />
                        {link.expires_at && (
                            <label className="flex items-center gap-2 mt-2 text-sm text-slate-600">
                                <input
                                    type="checkbox"
                                    checked={removeExpiration}
                                    onChange={(e) => setRemoveExpiration(e.target.checked)}
                                    className="rounded border-slate-300"
                                />
                                Remove expiration date
                            </label>
                        )}
                    </div>
                </div>

                <div className="flex gap-3 mt-6">
                    <button
                        onClick={onClose}
                        className="flex-1 px-4 py-2 text-slate-600 hover:text-slate-800 font-medium"
                    >
                        Cancel
                    </button>
                    <button
                        onClick={handleSave}
                        disabled={saving}
                        className="flex-1 px-4 py-2 bg-primary-600 text-white rounded-lg font-medium hover:bg-primary-700 disabled:opacity-70"
                    >
                        {saving ? 'Saving...' : 'Save Changes'}
                    </button>
                </div>
            </motion.div>
        </motion.div>
    );
}

function QRModal({ link, onClose }: { link: LinkData; onClose: () => void }) {
    const [qrUrl, setQrUrl] = useState<string | null>(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState('');

    useEffect(() => {
        const fetchQR = async () => {
            try {
                const res = await fetch(API_ENDPOINTS.linkQr(link.id), {
                    headers: getAuthHeaders()
                });
                if (res.ok) {
                    const blob = await res.blob();
                    setQrUrl(URL.createObjectURL(blob));
                } else {
                    setError('Failed to load QR code');
                }
            } catch {
                setError('Failed to load QR code');
            } finally {
                setLoading(false);
            }
        };
        fetchQR();
        return () => { if (qrUrl) URL.revokeObjectURL(qrUrl); };
    }, [link.id]);

    const downloadQR = () => {
        if (!qrUrl) return;
        const a = document.createElement('a');
        a.href = qrUrl;
        a.download = `qr-${link.code}.png`;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
    };

    return (
        <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4"
            onClick={onClose}
        >
            <motion.div
                initial={{ scale: 0.95, opacity: 0 }}
                animate={{ scale: 1, opacity: 1 }}
                exit={{ scale: 0.95, opacity: 0 }}
                className="bg-white rounded-2xl shadow-xl max-w-sm w-full p-6 text-center"
                onClick={e => e.stopPropagation()}
            >
                <h3 className="text-xl font-bold text-slate-900 mb-4">QR Code</h3>
                <div className="bg-white p-4 rounded-xl border border-slate-200 inline-block mb-4">
                    {loading ? (
                        <div className="w-48 h-48 flex items-center justify-center">
                            <div className="h-8 w-8 border-4 border-primary-200 border-t-primary-600 rounded-full animate-spin" />
                        </div>
                    ) : error ? (
                        <div className="w-48 h-48 flex items-center justify-center text-red-500 text-sm">
                            {error}
                        </div>
                    ) : (
                        <img 
                            src={qrUrl || ''} 
                            alt="QR Code" 
                            className="w-48 h-48"
                        />
                    )}
                </div>
                <p className="text-sm text-slate-500 mb-4 font-mono">{link.code}</p>
                <div className="flex gap-3 justify-center">
                    <button
                        onClick={onClose}
                        className="px-4 py-2 text-slate-600 hover:text-slate-800 font-medium"
                    >
                        Close
                    </button>
                    <button
                        onClick={downloadQR}
                        disabled={!qrUrl}
                        className="px-4 py-2 bg-primary-600 text-white rounded-lg font-medium hover:bg-primary-700 disabled:opacity-50 flex items-center gap-2"
                    >
                        <Download className="h-4 w-4" />
                        Download
                    </button>
                </div>
            </motion.div>
        </motion.div>
    );
}

function Skeleton() {
    return (
        <div className="animate-pulse space-y-4">
            {[1, 2, 3].map(i => (
                <div key={i} className="bg-white p-6 rounded-xl border border-slate-200">
                    <div className="flex items-center justify-between">
                        <div className="space-y-3 flex-1">
                            <div className="h-5 bg-slate-200 rounded w-48" />
                            <div className="h-4 bg-slate-100 rounded w-72" />
                        </div>
                        <div className="flex items-center gap-4">
                            <div className="h-8 w-20 bg-slate-100 rounded" />
                            <div className="h-8 w-8 bg-slate-100 rounded" />
                        </div>
                    </div>
                </div>
            ))}
        </div>
    );
}

// Mini stats component for each link
function MiniStats({ link }: { link: LinkData }) {
    const createdDate = new Date(link.created_at);
    const daysSinceCreation = Math.max(1, Math.floor((Date.now() - createdDate.getTime()) / (1000 * 60 * 60 * 24)));
    const avgClicksPerDay = (link.click_count / daysSinceCreation).toFixed(1);
    
    return (
        <div className="flex items-center gap-4 mt-2 text-xs text-slate-500">
            <span className="flex items-center gap-1">
                <TrendingUp className="h-3 w-3" />
                {avgClicksPerDay}/day avg
            </span>
            <span className="flex items-center gap-1">
                <Calendar className="h-3 w-3" />
                {createdDate.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })}
            </span>
            {link.tags && link.tags.length > 0 && (
                <div className="flex gap-1">
                    {link.tags.slice(0, 2).map(tag => (
                        <span 
                            key={tag.id}
                            className="px-1.5 py-0.5 rounded text-xs"
                            style={{ backgroundColor: `${tag.color}20`, color: tag.color }}
                        >
                            {tag.name}
                        </span>
                    ))}
                    {link.tags.length > 2 && (
                        <span className="text-slate-400">+{link.tags.length - 2}</span>
                    )}
                </div>
            )}
        </div>
    );
}

const LINKS_PER_PAGE = 20;

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
    const [error, setError] = useState('');
    const [currentPage, setCurrentPage] = useState(1);
    const [sortBy, setSortBy] = useState<'date' | 'clicks'>('date');
    const [sortOrder, setSortOrder] = useState<'asc' | 'desc'>('desc');
    const [appSettings, setAppSettings] = useState<AppSettings>({ 
        custom_aliases_enabled: true, 
        min_alias_length: 5, 
        max_alias_length: 50 
    });
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
        const checkClipboard = async () => {
            try {
                const text = await navigator.clipboard.readText();
                if (text && /^https?:\/\/.+/.test(text.trim())) {
                    setClipboardUrl(text.trim());
                } else {
                    setClipboardUrl(null);
                }
            } catch {
                // Clipboard access denied or not available
            }
        };

        window.addEventListener('focus', checkClipboard);
        checkClipboard();

        return () => window.removeEventListener('focus', checkClipboard);
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
            const res = await fetch(API_ENDPOINTS.bulkLinks, {
                method: 'POST',
                headers: getAuthHeaders(),
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
        } catch (err) {
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
        
        // Sort
        result.sort((a, b) => {
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
            const res = await fetch(API_ENDPOINTS.links, {
                headers: getAuthHeaders()
            });
            if (res.ok) {
                const data = await res.json();
                setLinks(data);
            } else if (res.status === 401) {
                localStorage.removeItem('token');
                navigate('/login');
            }
        } catch (error) {
            logger.error('Failed to fetch links', error);
            setError('Failed to load links. Please try again.');
        } finally {
            setLoading(false);
        }
    };

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
            
            const res = await fetch(API_ENDPOINTS.links, {
                method: 'POST',
                headers: getAuthHeaders(),
                body: JSON.stringify({
                    original_url: newUrl,
                    custom_alias: alias || undefined,
                    title: title || undefined,
                    password: password || undefined,
                    expires_at: expirationDate,
                }),
            });

            if (res.ok) {
                setNewUrl('');
                setAlias('');
                setTitle('');
                setPassword('');
                setExpiresAt('');
                setExpiresTime('23:59');
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
            const res = await fetch(API_ENDPOINTS.linkDelete(id), {
                method: 'DELETE',
                headers: getAuthHeaders()
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

    const handleUpdate = async (id: number, data: any) => {
        try {
            const res = await fetch(API_ENDPOINTS.linkUpdate(id), {
                method: 'PUT',
                headers: getAuthHeaders(),
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
            toast('Link copied to clipboard!', 'success');
            setTimeout(() => setCopiedId(null), 2000);
        } catch {
            toast('Failed to copy link', 'error');
        }
    };

    const handleExport = async () => {
        try {
            const response = await fetch(API_ENDPOINTS.exportLinks, {
                headers: getAuthHeaders(),
            });
            
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
        } catch (err: any) {
            setError(err.message || 'Failed to export links');
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
            <div className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
                <SEO title="Dashboard" noIndex />
                <div className="mb-8">
                    <div className="h-8 bg-slate-200 rounded w-48 animate-pulse" />
                </div>
                <Skeleton />
            </div>
        );
    }

    return (
        <div className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
            <SEO title="Dashboard" noIndex />
            
            <AnimatePresence>
                {editingLink && (
                    <EditModal 
                        link={editingLink} 
                        onClose={() => setEditingLink(null)} 
                        onSave={handleUpdate}
                    />
                )}
                {qrLink && (
                    <QRModal link={qrLink} onClose={() => setQrLink(null)} />
                )}
                {shareLink && (
                    <ShareModal 
                        url={`${import.meta.env.VITE_FRONTEND_URL || window.location.origin}/${shareLink.code}`}
                        title={shareLink.title || `Check out this link`}
                        onClose={() => setShareLink(null)} 
                    />
                )}
            </AnimatePresence>

            {/* Header with Stats */}
            <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-8">
                <div>
                    <h1 className="text-3xl font-bold text-slate-900">Dashboard</h1>
                    <div className="flex items-center gap-4 mt-2 text-sm text-slate-500">
                        <span className="flex items-center gap-1">
                            <Link2 className="h-4 w-4" />
                            {links.length} links
                        </span>
                        <span className="flex items-center gap-1">
                            <Zap className="h-4 w-4 text-green-500" />
                            {activeLinks} active
                        </span>
                        <span className="flex items-center gap-1">
                            <MousePointer className="h-4 w-4" />
                            {totalClicks.toLocaleString()} clicks
                        </span>
                    </div>
                </div>
                <div className="flex gap-2">
                    <button
                        onClick={() => setShowBulkImport(true)}
                        className="inline-flex items-center gap-2 px-4 py-2 bg-primary-50 text-primary-700 rounded-lg font-medium hover:bg-primary-100 transition-colors"
                    >
                        <Upload className="h-4 w-4" />
                        Bulk Import
                    </button>
                    <button
                        onClick={handleExport}
                        className="inline-flex items-center gap-2 px-4 py-2 bg-slate-100 text-slate-700 rounded-lg font-medium hover:bg-slate-200 transition-colors"
                    >
                        <Download className="h-4 w-4" />
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
                    className="mb-6 p-4 bg-primary-50 border border-primary-200 rounded-xl flex items-center justify-between"
                >
                    <div className="flex items-center gap-3">
                        <Clipboard className="h-5 w-5 text-primary-600" />
                        <div>
                            <p className="text-sm font-medium text-primary-800">URL detected in clipboard</p>
                            <p className="text-xs text-primary-600 truncate max-w-md">{clipboardUrl}</p>
                        </div>
                    </div>
                    <div className="flex items-center gap-2">
                        <button
                            onClick={() => setClipboardUrl(null)}
                            className="p-1 text-primary-400 hover:text-primary-600"
                        >
                            <X className="h-4 w-4" />
                        </button>
                        <button
                            onClick={handleClipboardCreate}
                            className="px-3 py-1.5 bg-primary-600 text-white rounded-lg text-sm font-medium hover:bg-primary-700"
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
                        className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4"
                        onClick={() => setShowBulkImport(false)}
                    >
                        <motion.div
                            initial={{ scale: 0.95, opacity: 0 }}
                            animate={{ scale: 1, opacity: 1 }}
                            exit={{ scale: 0.95, opacity: 0 }}
                            className="bg-white rounded-2xl shadow-xl max-w-lg w-full p-6"
                            onClick={e => e.stopPropagation()}
                        >
                            <div className="flex items-center justify-between mb-4">
                                <h3 className="text-xl font-bold text-slate-900">Bulk Import URLs</h3>
                                <button onClick={() => setShowBulkImport(false)} className="text-slate-400 hover:text-slate-600">
                                    <X className="h-5 w-5" />
                                </button>
                            </div>
                            <p className="text-sm text-slate-600 mb-4">
                                Paste one URL per line. Each URL will be shortened automatically.
                            </p>
                            <textarea
                                value={bulkUrls}
                                onChange={(e) => setBulkUrls(e.target.value)}
                                className="w-full h-48 px-4 py-3 border border-slate-300 rounded-lg focus:border-primary-500 focus:ring-1 focus:ring-primary-500 font-mono text-sm"
                                placeholder="https://example.com/page1&#10;https://example.com/page2&#10;https://example.com/page3"
                            />
                            <div className="flex justify-between items-center mt-4">
                                <span className="text-sm text-slate-500">
                                    {bulkUrls.split('\n').filter(u => u.trim() && /^https?:\/\/.+/.test(u.trim())).length} valid URLs
                                </span>
                                <div className="flex gap-3">
                                    <button
                                        onClick={() => setShowBulkImport(false)}
                                        className="px-4 py-2 text-slate-600 hover:text-slate-800"
                                    >
                                        Cancel
                                    </button>
                                    <button
                                        onClick={handleBulkImport}
                                        disabled={bulkImporting}
                                        className="px-4 py-2 bg-primary-600 text-white rounded-lg font-medium hover:bg-primary-700 disabled:opacity-70 flex items-center gap-2"
                                    >
                                        {bulkImporting ? (
                                            <div className="h-4 w-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
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
                    className="mb-6 p-4 bg-red-50 border border-red-200 rounded-xl text-red-700 text-sm flex items-center justify-between"
                >
                    {error}
                    <button onClick={() => setError('')} className="text-red-500 hover:text-red-700">
                        <X className="h-4 w-4" />
                    </button>
                </motion.div>
            )}

            {/* Create Link Form */}
            <div className="bg-white p-6 rounded-xl shadow-sm border border-slate-200 mb-8">
                <h2 className="text-lg font-semibold mb-4">Create new link</h2>
                <form onSubmit={handleCreate}>
                    <div className="flex flex-col sm:flex-row gap-4">
                        <input
                            type="url"
                            required
                            placeholder="https://example.com/long-url"
                            className="flex-1 rounded-lg border border-slate-300 px-4 py-2.5 focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                            value={newUrl}
                            onChange={(e) => setNewUrl(e.target.value)}
                        />
                        {appSettings.custom_aliases_enabled && (
                            <input
                                type="text"
                                placeholder={`alias (${appSettings.min_alias_length}-${appSettings.max_alias_length} chars)`}
                                className="sm:w-48 rounded-lg border border-slate-300 px-4 py-2.5 focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                                value={alias}
                                onChange={(e) => setAlias(e.target.value.replace(/[^a-zA-Z0-9_-]/g, ''))}
                                minLength={appSettings.min_alias_length}
                                maxLength={appSettings.max_alias_length}
                            />
                        )}
                        <button
                            type="submit"
                            disabled={creating}
                            className="bg-primary-600 text-white px-6 py-2.5 rounded-lg font-medium hover:bg-primary-700 transition-colors flex items-center justify-center gap-2 disabled:opacity-70"
                        >
                            {creating ? (
                                <div className="h-4 w-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
                            ) : (
                                <Plus className="h-4 w-4" />
                            )}
                            Create
                        </button>
                    </div>

                    <button
                        type="button"
                        onClick={() => setShowAdvanced(!showAdvanced)}
                        className="mt-3 text-sm text-slate-500 hover:text-slate-700 flex items-center gap-1"
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
                                <div className="space-y-4 mt-4 pt-4 border-t border-slate-100">
                                    {/* Title - full width */}
                                    <div>
                                        <label className="block text-sm font-medium text-slate-700 mb-1">
                                            Title (private, only visible to you)
                                        </label>
                                        <input
                                            type="text"
                                            placeholder="e.g. Marketing campaign Q1"
                                            className="w-full rounded-lg border border-slate-300 px-4 py-2 focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                                            value={title}
                                            onChange={(e) => setTitle(e.target.value)}
                                            maxLength={100}
                                        />
                                    </div>
                                    
                                    <div className="grid sm:grid-cols-2 gap-4">
                                        <div>
                                            <label className="block text-sm font-medium text-slate-700 mb-1">
                                                <Lock className="h-3.5 w-3.5 inline mr-1" />
                                                Password Protection
                                            </label>
                                            <input
                                                type="password"
                                                placeholder="Leave empty for no password"
                                                className="w-full rounded-lg border border-slate-300 px-4 py-2 focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                                                value={password}
                                                onChange={(e) => setPassword(e.target.value)}
                                            />
                                        </div>
                                        <div>
                                            <label className="block text-sm font-medium text-slate-700 mb-1">
                                                <Calendar className="h-3.5 w-3.5 inline mr-1" />
                                                Expiration
                                            </label>
                                            <div className="flex gap-2">
                                                <input
                                                    type="date"
                                                    className="flex-1 rounded-lg border border-slate-300 px-4 py-2 focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                                                    value={expiresAt}
                                                    onChange={(e) => setExpiresAt(e.target.value)}
                                                    min={new Date().toISOString().split('T')[0]}
                                                />
                                                <input
                                                    type="time"
                                                    className="w-28 rounded-lg border border-slate-300 px-3 py-2 focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                                                    value={expiresTime}
                                                    onChange={(e) => setExpiresTime(e.target.value)}
                                                />
                                            </div>
                                            <p className="text-xs text-slate-500 mt-1">
                                                Your timezone: {Intl.DateTimeFormat().resolvedOptions().timeZone}
                                                {expiresAt && expiresTime && (
                                                    <span className="ml-2">
                                                        → Expires: {new Date(`${expiresAt}T${expiresTime}`).toLocaleString()}
                                                    </span>
                                                )}
                                            </p>
                                        </div>
                                    </div>
                                </div>
                            </motion.div>
                        )}
                    </AnimatePresence>
                </form>
            </div>

            {/* Search and Filter */}
            {links.length > 0 && (
                <div className="mb-6 flex flex-col sm:flex-row gap-4">
                    <div className="relative flex-1">
                        <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-slate-400" />
                        <input
                            type="text"
                            placeholder="Search links, notes, tags..."
                            className="w-full pl-10 pr-4 py-2.5 border border-slate-300 rounded-lg focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                            value={searchQuery}
                            onChange={e => setSearchQuery(e.target.value)}
                        />
                    </div>
                    <div className="flex gap-2">
                        <select
                            value={sortBy}
                            onChange={(e) => setSortBy(e.target.value as 'date' | 'clicks')}
                            className="px-3 py-2 border border-slate-300 rounded-lg text-sm focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                        >
                            <option value="date">Sort by Date</option>
                            <option value="clicks">Sort by Clicks</option>
                        </select>
                        <button
                            onClick={() => setSortOrder(sortOrder === 'asc' ? 'desc' : 'asc')}
                            className="p-2.5 border border-slate-300 rounded-lg hover:bg-slate-50"
                            title={sortOrder === 'asc' ? 'Ascending' : 'Descending'}
                        >
                            {sortOrder === 'asc' ? <SortAsc className="h-4 w-4" /> : <SortDesc className="h-4 w-4" />}
                        </button>
                    </div>
                </div>
            )}

            {/* Links List */}
            <div className="space-y-4">
                <AnimatePresence mode="popLayout">
                    {paginatedLinks.map((link, index) => {
                        // Calculate link number (bottom to top, 1 = oldest)
                        const linkNumber = filteredLinks.length - ((currentPage - 1) * LINKS_PER_PAGE + index);
                        const mainUrl = `${import.meta.env.VITE_FRONTEND_URL || window.location.origin}/${link.code}`;
                        const apiUrl = link.short_url; // This is the API URL from backend
                        
                        return (
                        <motion.div 
                            key={link.id}
                            layout
                            initial={{ opacity: 0, y: 20 }}
                            animate={{ opacity: 1, y: 0 }}
                            exit={{ opacity: 0, scale: 0.95 }}
                            className="bg-white p-5 rounded-xl shadow-sm border border-slate-200 hover:shadow-md transition-shadow"
                        >
                            <div className="flex flex-col sm:flex-row sm:items-start justify-between gap-4">
                                <div className="space-y-1 min-w-0 flex-1">
                                    {/* Link number and title */}
                                    <div className="flex items-center gap-2 mb-1">
                                        <span className="text-xs font-bold text-slate-400 bg-slate-100 px-1.5 py-0.5 rounded">
                                            #{linkNumber}
                                        </span>
                                        {link.title && (
                                            <span className="text-sm font-medium text-slate-700 truncate">
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
                                            className="text-lg font-bold text-primary-600 hover:underline"
                                        >
                                            {mainUrl.replace(/^https?:\/\//, '')}
                                        </a>
                                        <span className="text-slate-400">→</span>
                                        <a 
                                            href={link.original_url}
                                            target="_blank" 
                                            rel="noreferrer" 
                                            className="text-sm text-slate-500 hover:text-slate-700 hover:underline truncate max-w-[300px]"
                                            title={link.original_url}
                                        >
                                            {link.original_url.replace(/^https?:\/\//, '').substring(0, 50)}{link.original_url.length > 60 ? '...' : ''}
                                        </a>
                                        <button
                                            onClick={() => handleCopy(link)}
                                            className="text-slate-400 hover:text-slate-600 transition-colors"
                                            title="Copy short URL"
                                        >
                                            {copiedId === link.id ? (
                                                <Check className="h-4 w-4 text-green-500" />
                                            ) : (
                                                <Copy className="h-4 w-4" />
                                            )}
                                        </button>
                                        {link.has_password && (
                                            <span className="inline-flex items-center gap-1 px-2 py-0.5 bg-amber-100 text-amber-700 rounded text-xs font-medium">
                                                <Lock className="h-3 w-3" />
                                                Protected
                                            </span>
                                        )}
                                        {link.expires_at && (
                                            <span className="inline-flex items-center gap-1 px-2 py-0.5 bg-blue-100 text-blue-700 rounded text-xs font-medium">
                                                <Clock className="h-3 w-3" />
                                                {new Date(link.expires_at).toLocaleDateString()}
                                            </span>
                                        )}
                                        {!link.is_active && (
                                            <span className="inline-flex items-center gap-1 px-2 py-0.5 bg-red-100 text-red-700 rounded text-xs font-medium">
                                                Inactive
                                            </span>
                                        )}
                                    </div>
                                    {/* API URL */}
                                    <p className="text-xs text-slate-400">
                                        API: <a href={apiUrl} target="_blank" rel="noreferrer" className="hover:underline">{apiUrl}</a>
                                    </p>
                                    {/* Mini stats */}
                                    <MiniStats link={link} />
                                </div>

                                <div className="flex items-center gap-2 sm:gap-4">
                                    <Link 
                                        to={`/analytics/${link.id}`}
                                        className="flex items-center gap-1.5 text-slate-600 hover:text-primary-600 text-sm font-medium transition-colors"
                                    >
                                        <BarChart2 className="h-4 w-4" />
                                        <span className="font-bold">{link.click_count.toLocaleString()}</span>
                                    </Link>
                                    <div className="flex items-center gap-1">
                                        <button 
                                            onClick={() => handleShare(link)}
                                            className="p-2 text-slate-400 hover:text-slate-600 transition-colors"
                                            title="Share"
                                        >
                                            <Share2 className="h-4 w-4" />
                                        </button>
                                        <button 
                                            onClick={() => setQrLink(link)}
                                            className="p-2 text-slate-400 hover:text-slate-600 transition-colors"
                                            title="QR Code"
                                        >
                                            <QrCode className="h-4 w-4" />
                                        </button>
                                        <button 
                                            onClick={() => setEditingLink(link)}
                                            className="p-2 text-slate-400 hover:text-slate-600 transition-colors"
                                            title="Edit"
                                        >
                                            <Edit2 className="h-4 w-4" />
                                        </button>
                                        <button 
                                            onClick={() => handleDelete(link.id)}
                                            className="p-2 text-slate-400 hover:text-red-600 transition-colors"
                                            title="Delete"
                                        >
                                            <Trash2 className="h-4 w-4" />
                                        </button>
                                    </div>
                                </div>
                            </div>
                        </motion.div>
                    );})}
                </AnimatePresence>

                {/* Pagination */}
                {totalPages > 1 && (
                    <div className="flex items-center justify-between pt-4 border-t border-slate-200">
                        <p className="text-sm text-slate-500">
                            Showing {((currentPage - 1) * LINKS_PER_PAGE) + 1}-{Math.min(currentPage * LINKS_PER_PAGE, filteredLinks.length)} of {filteredLinks.length}
                        </p>
                        <div className="flex items-center gap-2">
                            <button
                                onClick={() => setCurrentPage(p => Math.max(1, p - 1))}
                                disabled={currentPage === 1}
                                className="p-2 border border-slate-300 rounded-lg hover:bg-slate-50 disabled:opacity-50 disabled:cursor-not-allowed"
                            >
                                <ChevronLeft className="h-4 w-4" />
                            </button>
                            <span className="px-3 py-1 text-sm font-medium">
                                {currentPage} / {totalPages}
                            </span>
                            <button
                                onClick={() => setCurrentPage(p => Math.min(totalPages, p + 1))}
                                disabled={currentPage === totalPages}
                                className="p-2 border border-slate-300 rounded-lg hover:bg-slate-50 disabled:opacity-50 disabled:cursor-not-allowed"
                            >
                                <ChevronRight className="h-4 w-4" />
                            </button>
                        </div>
                    </div>
                )}

                {filteredLinks.length === 0 && searchQuery && (
                    <div className="text-center py-12 text-slate-500">
                        No links found matching "{searchQuery}"
                    </div>
                )}

                {links.length === 0 && (
                    <motion.div 
                        initial={{ opacity: 0 }}
                        animate={{ opacity: 1 }}
                        className="text-center py-16 bg-slate-50 rounded-xl border-2 border-dashed border-slate-200"
                    >
                        <div className="h-12 w-12 bg-slate-100 rounded-full flex items-center justify-center mx-auto mb-4">
                            <Plus className="h-6 w-6 text-slate-400" />
                        </div>
                        <h3 className="text-lg font-semibold text-slate-700 mb-1">No links yet</h3>
                        <p className="text-slate-500">Create your first shortened link above!</p>
                    </motion.div>
                )}
            </div>
        </div>
    );
}
