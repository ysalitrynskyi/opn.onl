import { useEffect, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { 
    Copy, ExternalLink, Plus, Trash2, BarChart2, 
    QrCode, Download, Lock, Clock, Edit2, X, Check,
    Search, ChevronDown, Calendar
} from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { API_ENDPOINTS, getAuthHeaders } from '../config/api';

interface LinkData {
    id: number;
    code: string;
    original_url: string;
    short_url: string;
    click_count: number;
    created_at: string;
    expires_at: string | null;
    has_password: boolean;
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
                        <div className="px-4 py-2 bg-slate-100 rounded-lg text-slate-600 text-sm">
                            {link.short_url}
                        </div>
                    </div>

                    <div>
                        <label className="block text-sm font-medium text-slate-700 mb-1">Destination URL</label>
                        <input
                            type="url"
                            value={url}
                            onChange={e => setUrl(e.target.value)}
                            className="w-full px-4 py-2 border border-slate-300 rounded-lg focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                        />
                    </div>

                    <div>
                        <label className="block text-sm font-medium text-slate-700 mb-1">
                            {link.has_password ? 'Change Password' : 'Add Password'}
                        </label>
                        <input
                            type="password"
                            value={password}
                            onChange={e => setPassword(e.target.value)}
                            placeholder={link.has_password ? '••••••••' : 'Leave empty for no password'}
                            className="w-full px-4 py-2 border border-slate-300 rounded-lg focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                        />
                        {link.has_password && (
                            <label className="flex items-center gap-2 mt-2 text-sm text-slate-600">
                                <input
                                    type="checkbox"
                                    checked={removePassword}
                                    onChange={e => setRemovePassword(e.target.checked)}
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
                            onChange={e => setExpiresAt(e.target.value)}
                            disabled={removeExpiration}
                            className="w-full px-4 py-2 border border-slate-300 rounded-lg focus:border-primary-500 focus:ring-1 focus:ring-primary-500 disabled:opacity-50"
                        />
                        {link.expires_at && (
                            <label className="flex items-center gap-2 mt-2 text-sm text-slate-600">
                                <input
                                    type="checkbox"
                                    checked={removeExpiration}
                                    onChange={e => setRemoveExpiration(e.target.checked)}
                                    className="rounded border-slate-300"
                                />
                                Remove expiration
                            </label>
                        )}
                    </div>
                </div>

                <div className="flex justify-end gap-3 mt-6">
                    <button
                        onClick={onClose}
                        className="px-4 py-2 text-slate-600 hover:text-slate-800 font-medium"
                    >
                        Cancel
                    </button>
                    <button
                        onClick={handleSave}
                        disabled={saving}
                        className="px-4 py-2 bg-primary-600 text-white rounded-lg font-medium hover:bg-primary-700 disabled:opacity-70 flex items-center gap-2"
                    >
                        {saving ? (
                            <div className="h-4 w-4 border-2 border-white/30 border-t-white rounded-full animate-spin" />
                        ) : (
                            <Check className="h-4 w-4" />
                        )}
                        Save Changes
                    </button>
                </div>
            </motion.div>
        </motion.div>
    );
}

function QRModal({ link, onClose }: { link: LinkData; onClose: () => void }) {
    const [qrUrl, setQrUrl] = useState<string | null>(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        const fetchQR = async () => {
            try {
                const response = await fetch(API_ENDPOINTS.linkQr(link.id), {
                    headers: getAuthHeaders(),
                });
                if (!response.ok) {
                    throw new Error('Failed to load QR code');
                }
                const blob = await response.blob();
                const url = URL.createObjectURL(blob);
                setQrUrl(url);
            } catch (err) {
                setError(err instanceof Error ? err.message : 'Failed to load QR code');
            } finally {
                setLoading(false);
            }
        };
        fetchQR();
        
        return () => {
            if (qrUrl) {
                URL.revokeObjectURL(qrUrl);
            }
        };
    }, [link.id]);

    const downloadQR = async () => {
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
                <p className="text-sm text-slate-500 mb-4">{link.short_url}</p>
                <div className="flex gap-3 justify-center">
                    <button
                        onClick={onClose}
                        className="px-4 py-2 text-slate-600 hover:text-slate-800 font-medium"
                    >
                        Close
                    </button>
                    <button
                        onClick={downloadQR}
                        className="px-4 py-2 bg-primary-600 text-white rounded-lg font-medium hover:bg-primary-700 flex items-center gap-2"
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

export default function Dashboard() {
    const [links, setLinks] = useState<LinkData[]>([]);
    const [filteredLinks, setFilteredLinks] = useState<LinkData[]>([]);
    const [loading, setLoading] = useState(true);
    const [newUrl, setNewUrl] = useState('');
    const [alias, setAlias] = useState('');
    const [password, setPassword] = useState('');
    const [expiresAt, setExpiresAt] = useState('');
    const [creating, setCreating] = useState(false);
    const [showAdvanced, setShowAdvanced] = useState(false);
    const [searchQuery, setSearchQuery] = useState('');
    const [editingLink, setEditingLink] = useState<LinkData | null>(null);
    const [qrLink, setQrLink] = useState<LinkData | null>(null);
    const [copiedId, setCopiedId] = useState<number | null>(null);
    const [error, setError] = useState('');
    const navigate = useNavigate();

    useEffect(() => {
        const token = localStorage.getItem('token');
        if (!token) {
            navigate('/login');
            return;
        }
        fetchLinks();
    }, [navigate]);

    useEffect(() => {
        if (searchQuery) {
            const query = searchQuery.toLowerCase();
            setFilteredLinks(links.filter(link => 
                link.code.toLowerCase().includes(query) ||
                link.original_url.toLowerCase().includes(query)
            ));
        } else {
            setFilteredLinks(links);
        }
    }, [searchQuery, links]);

    const fetchLinks = async () => {
        try {
            const res = await fetch(API_ENDPOINTS.links, {
                headers: getAuthHeaders()
            });
            if (res.ok) {
                const data = await res.json();
                setLinks(data);
                setFilteredLinks(data);
            } else if (res.status === 401) {
                localStorage.removeItem('token');
                navigate('/login');
            }
        } catch (error) {
            console.error('Failed to fetch links', error);
            setError('Failed to load links. Please try again.');
        } finally {
            setLoading(false);
        }
    };

    const handleCreate = async (e: React.FormEvent) => {
        e.preventDefault();
        setCreating(true);
        setError('');
        
        try {
            const res = await fetch(API_ENDPOINTS.links, {
                method: 'POST',
                headers: getAuthHeaders(),
                body: JSON.stringify({
                    original_url: newUrl,
                    custom_alias: alias || undefined,
                    password: password || undefined,
                    expires_at: expiresAt ? new Date(expiresAt).toISOString() : undefined,
                }),
            });

            const data = await res.json();

            if (res.ok) {
                setNewUrl('');
                setAlias('');
                setPassword('');
                setExpiresAt('');
                setShowAdvanced(false);
                fetchLinks();
            } else {
                setError(data.error || 'Failed to create link');
            }
        } catch (error) {
            console.error('Failed to create link', error);
            setError('Network error. Please try again.');
        } finally {
            setCreating(false);
        }
    };

    const handleDelete = async (id: number) => {
        if (!confirm('Are you sure you want to delete this link? This action cannot be undone.')) {
            return;
        }

        try {
            const res = await fetch(API_ENDPOINTS.linkDelete(id), {
                method: 'DELETE',
                headers: getAuthHeaders(),
            });

            if (res.ok) {
                setLinks(links.filter(l => l.id !== id));
            } else {
                const data = await res.json();
                setError(data.error || 'Failed to delete link');
            }
        } catch (error) {
            console.error('Failed to delete link', error);
            setError('Network error. Please try again.');
        }
    };

    const handleUpdate = async (id: number, updateData: any) => {
        try {
            const res = await fetch(API_ENDPOINTS.linkUpdate(id), {
                method: 'PUT',
                headers: getAuthHeaders(),
                body: JSON.stringify(updateData),
            });

            if (res.ok) {
                fetchLinks();
            } else {
                const data = await res.json();
                setError(data.error || 'Failed to update link');
            }
        } catch (error) {
            console.error('Failed to update link', error);
            setError('Network error. Please try again.');
        }
    };

    const handleCopy = async (link: LinkData) => {
        await navigator.clipboard.writeText(link.short_url);
        setCopiedId(link.id);
        setTimeout(() => setCopiedId(null), 2000);
    };

    const handleExport = async () => {
        try {
            const token = localStorage.getItem('token');
            const response = await fetch(API_ENDPOINTS.exportLinks, {
                headers: {
                    'Authorization': `Bearer ${token}`,
                },
            });
            
            if (!response.ok) {
                throw new Error('Failed to export links');
            }
            
            const blob = await response.blob();
            const url = window.URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url;
            a.download = 'links.csv';
            document.body.appendChild(a);
            a.click();
            window.URL.revokeObjectURL(url);
            document.body.removeChild(a);
        } catch (err: any) {
            alert(err.message || 'Failed to export links');
        }
    };

    const totalClicks = links.reduce((sum, link) => sum + link.click_count, 0);

    if (loading) {
        return (
            <div className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
                <div className="mb-8">
                    <div className="h-8 bg-slate-200 rounded w-48 animate-pulse" />
                </div>
                <Skeleton />
            </div>
        );
    }

    return (
        <div className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
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
            </AnimatePresence>

            <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4 mb-8">
                <div>
                    <h1 className="text-3xl font-bold text-slate-900">Dashboard</h1>
                    <p className="text-slate-500 mt-1">
                        {links.length} links · {totalClicks.toLocaleString()} total clicks
                    </p>
                </div>
                <button
                    onClick={handleExport}
                    className="inline-flex items-center gap-2 px-4 py-2 bg-slate-100 text-slate-700 rounded-lg font-medium hover:bg-slate-200 transition-colors"
                >
                    <Download className="h-4 w-4" />
                    Export CSV
                </button>
            </div>

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
                        <input
                            type="text"
                            placeholder="alias (optional)"
                            className="sm:w-40 rounded-lg border border-slate-300 px-4 py-2.5 focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                            value={alias}
                            onChange={(e) => setAlias(e.target.value)}
                        />
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
                                <div className="grid sm:grid-cols-2 gap-4 mt-4 pt-4 border-t border-slate-100">
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
                                            Expiration Date
                                        </label>
                                        <input
                                            type="date"
                                            className="w-full rounded-lg border border-slate-300 px-4 py-2 focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                                            value={expiresAt}
                                            onChange={(e) => setExpiresAt(e.target.value)}
                                            min={new Date().toISOString().split('T')[0]}
                                        />
                                    </div>
                                </div>
                            </motion.div>
                        )}
                    </AnimatePresence>
                </form>
            </div>

            {links.length > 0 && (
                <div className="mb-6">
                    <div className="relative">
                        <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-slate-400" />
                        <input
                            type="text"
                            placeholder="Search links..."
                            className="w-full pl-10 pr-4 py-2.5 border border-slate-300 rounded-lg focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                            value={searchQuery}
                            onChange={e => setSearchQuery(e.target.value)}
                        />
                    </div>
                </div>
            )}

            <div className="space-y-4">
                <AnimatePresence mode="popLayout">
                    {filteredLinks.map((link) => (
                        <motion.div 
                            key={link.id}
                            layout
                            initial={{ opacity: 0, y: 20 }}
                            animate={{ opacity: 1, y: 0 }}
                            exit={{ opacity: 0, scale: 0.95 }}
                            className="bg-white p-5 rounded-xl shadow-sm border border-slate-200 hover:shadow-md transition-shadow"
                        >
                            <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
                                <div className="space-y-1 min-w-0 flex-1">
                                    <div className="flex items-center gap-3 flex-wrap">
                                        <a 
                                            href={link.short_url} 
                                            target="_blank" 
                                            rel="noreferrer" 
                                            className="text-lg font-bold text-primary-600 hover:underline flex items-center gap-1"
                                        >
                                            {link.short_url.replace('http://localhost:3000/', 'opn.onl/')}
                                            <ExternalLink className="h-3.5 w-3.5" />
                                        </a>
                                        <button
                                            onClick={() => handleCopy(link)}
                                            className="text-slate-400 hover:text-slate-600 transition-colors"
                                            title="Copy to clipboard"
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
                                                Expires {new Date(link.expires_at).toLocaleDateString()}
                                            </span>
                                        )}
                                    </div>
                                    <p className="text-slate-500 text-sm truncate">{link.original_url}</p>
                                </div>

                                <div className="flex items-center gap-2 sm:gap-4">
                                    <Link 
                                        to={`/analytics/${link.id}`}
                                        className="flex items-center gap-1.5 text-slate-600 hover:text-primary-600 text-sm font-medium transition-colors"
                                    >
                                        <BarChart2 className="h-4 w-4" />
                                        <span>{link.click_count.toLocaleString()} clicks</span>
                                    </Link>
                                    <div className="flex items-center gap-1">
                                        <button 
                                            onClick={() => setQrLink(link)}
                                            className="p-2 text-slate-400 hover:text-slate-600 transition-colors"
                                            title="View QR Code"
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
                    ))}
                </AnimatePresence>

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
