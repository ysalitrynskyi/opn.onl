import { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { motion } from 'framer-motion';
import { 
    Shield, Users, Link2, Ban, Globe, Trash2, Plus, 
    RefreshCw, AlertTriangle, Check, X, Database, BarChart2
} from 'lucide-react';
import { API_ENDPOINTS, getAuthHeaders } from '../config/api';

interface AdminStats {
    total_users: number;
    active_users: number;
    total_links: number;
    active_links: number;
    total_clicks: number;
    blocked_links_count: number;
    blocked_domains_count: number;
}

interface BlockedLink {
    id: number;
    url: string;
    reason: string | null;
    blocked_by: number | null;
    created_at: string;
}

interface BlockedDomain {
    id: number;
    domain: string;
    reason: string | null;
    blocked_by: number | null;
    created_at: string;
}

interface AdminUser {
    id: number;
    email: string;
    is_admin: boolean;
    email_verified: boolean;
    created_at: string;
    deleted_at: string | null;
}

export default function Admin() {
    const navigate = useNavigate();
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState('');
    const [success, setSuccess] = useState('');
    
    const [stats, setStats] = useState<AdminStats | null>(null);
    const [blockedLinks, setBlockedLinks] = useState<BlockedLink[]>([]);
    const [blockedDomains, setBlockedDomains] = useState<BlockedDomain[]>([]);
    const [users, setUsers] = useState<AdminUser[]>([]);
    
    const [newBlockedUrl, setNewBlockedUrl] = useState('');
    const [newBlockedUrlReason, setNewBlockedUrlReason] = useState('');
    const [newBlockedDomain, setNewBlockedDomain] = useState('');
    const [newBlockedDomainReason, setNewBlockedDomainReason] = useState('');
    
    const [activeTab, setActiveTab] = useState<'stats' | 'blocked' | 'users'>('stats');

    useEffect(() => {
        const token = localStorage.getItem('token');
        if (!token) {
            navigate('/login');
            return;
        }
        loadData();
    }, [navigate]);

    const loadData = async () => {
        setLoading(true);
        setError('');
        
        try {
            const [statsRes, linksRes, domainsRes, usersRes] = await Promise.all([
                fetch(`${API_ENDPOINTS.base}/admin/stats`, { headers: getAuthHeaders() }),
                fetch(`${API_ENDPOINTS.base}/admin/blocked/links`, { headers: getAuthHeaders() }),
                fetch(`${API_ENDPOINTS.base}/admin/blocked/domains`, { headers: getAuthHeaders() }),
                fetch(`${API_ENDPOINTS.base}/admin/users`, { headers: getAuthHeaders() }),
            ]);

            if (statsRes.status === 403) {
                navigate('/dashboard');
                return;
            }

            if (statsRes.ok) setStats(await statsRes.json());
            if (linksRes.ok) setBlockedLinks(await linksRes.json());
            if (domainsRes.ok) setBlockedDomains(await domainsRes.json());
            if (usersRes.ok) setUsers(await usersRes.json());
        } catch (err) {
            setError('Failed to load admin data');
        } finally {
            setLoading(false);
        }
    };

    const blockUrl = async () => {
        if (!newBlockedUrl.trim()) return;
        
        try {
            const res = await fetch(`${API_ENDPOINTS.base}/admin/blocked/links`, {
                method: 'POST',
                headers: getAuthHeaders(),
                body: JSON.stringify({ 
                    url: newBlockedUrl.trim(), 
                    reason: newBlockedUrlReason.trim() || null 
                }),
            });
            
            if (res.ok) {
                setSuccess('URL blocked successfully');
                setNewBlockedUrl('');
                setNewBlockedUrlReason('');
                loadData();
            } else {
                const data = await res.json();
                setError(data.message || 'Failed to block URL');
            }
        } catch {
            setError('Failed to block URL');
        }
    };

    const unblockUrl = async (id: number) => {
        try {
            const res = await fetch(`${API_ENDPOINTS.base}/admin/blocked/links/${id}`, {
                method: 'DELETE',
                headers: getAuthHeaders(),
            });
            
            if (res.ok) {
                setSuccess('URL unblocked');
                loadData();
            }
        } catch {
            setError('Failed to unblock URL');
        }
    };

    const blockDomain = async () => {
        if (!newBlockedDomain.trim()) return;
        
        try {
            const res = await fetch(`${API_ENDPOINTS.base}/admin/blocked/domains`, {
                method: 'POST',
                headers: getAuthHeaders(),
                body: JSON.stringify({ 
                    domain: newBlockedDomain.trim(), 
                    reason: newBlockedDomainReason.trim() || null 
                }),
            });
            
            if (res.ok) {
                setSuccess('Domain blocked successfully');
                setNewBlockedDomain('');
                setNewBlockedDomainReason('');
                loadData();
            } else {
                const data = await res.json();
                setError(data.message || 'Failed to block domain');
            }
        } catch {
            setError('Failed to block domain');
        }
    };

    const unblockDomain = async (id: number) => {
        try {
            const res = await fetch(`${API_ENDPOINTS.base}/admin/blocked/domains/${id}`, {
                method: 'DELETE',
                headers: getAuthHeaders(),
            });
            
            if (res.ok) {
                setSuccess('Domain unblocked');
                loadData();
            }
        } catch {
            setError('Failed to unblock domain');
        }
    };

    const toggleAdmin = async (userId: number, isCurrentlyAdmin: boolean) => {
        const endpoint = isCurrentlyAdmin ? 'remove-admin' : 'make-admin';
        
        try {
            const res = await fetch(`${API_ENDPOINTS.base}/admin/users/${userId}/${endpoint}`, {
                method: 'POST',
                headers: getAuthHeaders(),
            });
            
            if (res.ok) {
                setSuccess(isCurrentlyAdmin ? 'Admin status removed' : 'User is now admin');
                loadData();
            } else {
                const data = await res.json();
                setError(data.message || 'Failed to update user');
            }
        } catch {
            setError('Failed to update user');
        }
    };

    const deleteUser = async (userId: number) => {
        if (!confirm('Are you sure you want to delete this user?')) return;
        
        try {
            const res = await fetch(`${API_ENDPOINTS.base}/admin/users/${userId}`, {
                method: 'DELETE',
                headers: getAuthHeaders(),
            });
            
            if (res.ok) {
                setSuccess('User deleted');
                loadData();
            }
        } catch {
            setError('Failed to delete user');
        }
    };

    const restoreUser = async (userId: number) => {
        try {
            const res = await fetch(`${API_ENDPOINTS.base}/admin/users/${userId}/restore`, {
                method: 'POST',
                headers: getAuthHeaders(),
            });
            
            if (res.ok) {
                setSuccess('User restored');
                loadData();
            }
        } catch {
            setError('Failed to restore user');
        }
    };

    const createBackup = async () => {
        try {
            const res = await fetch(`${API_ENDPOINTS.base}/admin/backup`, {
                method: 'POST',
                headers: getAuthHeaders(),
            });
            
            const data = await res.json();
            if (res.ok) {
                setSuccess(`Backup created: ${data.filename}`);
            } else {
                setError(data.message || 'Failed to create backup');
            }
        } catch {
            setError('Failed to create backup');
        }
    };

    if (loading) {
        return (
            <div className="flex items-center justify-center min-h-[60vh]">
                <RefreshCw className="h-8 w-8 animate-spin text-primary-600" />
            </div>
        );
    }

    return (
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-12">
            {/* Header */}
            <motion.div
                initial={{ opacity: 0, y: -20 }}
                animate={{ opacity: 1, y: 0 }}
                className="mb-8"
            >
                <div className="flex items-center gap-3 mb-2">
                    <Shield className="h-8 w-8 text-primary-600" />
                    <h1 className="text-3xl font-bold text-slate-900">Admin Dashboard</h1>
                </div>
                <p className="text-slate-500">Manage users, blocked content, and system settings</p>
            </motion.div>

            {/* Alerts */}
            {error && (
                <div className="mb-6 bg-red-50 border border-red-200 rounded-xl p-4 flex items-center gap-3">
                    <AlertTriangle className="h-5 w-5 text-red-600" />
                    <span className="text-red-800">{error}</span>
                    <button onClick={() => setError('')} className="ml-auto">
                        <X className="h-4 w-4 text-red-600" />
                    </button>
                </div>
            )}
            
            {success && (
                <div className="mb-6 bg-green-50 border border-green-200 rounded-xl p-4 flex items-center gap-3">
                    <Check className="h-5 w-5 text-green-600" />
                    <span className="text-green-800">{success}</span>
                    <button onClick={() => setSuccess('')} className="ml-auto">
                        <X className="h-4 w-4 text-green-600" />
                    </button>
                </div>
            )}

            {/* Tabs */}
            <div className="flex gap-2 mb-8 border-b border-slate-200">
                <button
                    onClick={() => setActiveTab('stats')}
                    className={`px-4 py-3 font-medium border-b-2 transition-colors ${
                        activeTab === 'stats' 
                            ? 'text-primary-600 border-primary-600' 
                            : 'text-slate-500 border-transparent hover:text-slate-700'
                    }`}
                >
                    <BarChart2 className="h-4 w-4 inline mr-2" />
                    Statistics
                </button>
                <button
                    onClick={() => setActiveTab('blocked')}
                    className={`px-4 py-3 font-medium border-b-2 transition-colors ${
                        activeTab === 'blocked' 
                            ? 'text-primary-600 border-primary-600' 
                            : 'text-slate-500 border-transparent hover:text-slate-700'
                    }`}
                >
                    <Ban className="h-4 w-4 inline mr-2" />
                    Blocked Content
                </button>
                <button
                    onClick={() => setActiveTab('users')}
                    className={`px-4 py-3 font-medium border-b-2 transition-colors ${
                        activeTab === 'users' 
                            ? 'text-primary-600 border-primary-600' 
                            : 'text-slate-500 border-transparent hover:text-slate-700'
                    }`}
                >
                    <Users className="h-4 w-4 inline mr-2" />
                    Users
                </button>
            </div>

            {/* Stats Tab */}
            {activeTab === 'stats' && stats && (
                <motion.div
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    className="space-y-6"
                >
                    <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                        <StatCard label="Total Users" value={stats.total_users} icon={Users} />
                        <StatCard label="Active Users" value={stats.active_users} icon={Users} color="green" />
                        <StatCard label="Total Links" value={stats.total_links} icon={Link2} />
                        <StatCard label="Active Links" value={stats.active_links} icon={Link2} color="green" />
                        <StatCard label="Total Clicks" value={stats.total_clicks} icon={BarChart2} color="blue" />
                        <StatCard label="Blocked URLs" value={stats.blocked_links_count} icon={Ban} color="red" />
                        <StatCard label="Blocked Domains" value={stats.blocked_domains_count} icon={Globe} color="red" />
                    </div>

                    <div className="bg-white rounded-xl border border-slate-200 p-6">
                        <h3 className="text-lg font-semibold mb-4 flex items-center gap-2">
                            <Database className="h-5 w-5" />
                            Backup Management
                        </h3>
                        <button
                            onClick={createBackup}
                            className="bg-primary-600 text-white px-4 py-2 rounded-lg hover:bg-primary-700 transition-colors"
                        >
                            Create Backup Now
                        </button>
                    </div>
                </motion.div>
            )}

            {/* Blocked Content Tab */}
            {activeTab === 'blocked' && (
                <motion.div
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    className="space-y-8"
                >
                    {/* Block URL */}
                    <div className="bg-white rounded-xl border border-slate-200 p-6">
                        <h3 className="text-lg font-semibold mb-4 flex items-center gap-2">
                            <Ban className="h-5 w-5 text-red-600" />
                            Block URL
                        </h3>
                        <div className="flex gap-3">
                            <input
                                type="url"
                                value={newBlockedUrl}
                                onChange={(e) => setNewBlockedUrl(e.target.value)}
                                placeholder="https://example.com/malicious"
                                className="flex-1 px-4 py-2 border border-slate-300 rounded-lg focus:ring-2 focus:ring-primary-500"
                            />
                            <input
                                type="text"
                                value={newBlockedUrlReason}
                                onChange={(e) => setNewBlockedUrlReason(e.target.value)}
                                placeholder="Reason (optional)"
                                className="w-48 px-4 py-2 border border-slate-300 rounded-lg focus:ring-2 focus:ring-primary-500"
                            />
                            <button
                                onClick={blockUrl}
                                className="bg-red-600 text-white px-4 py-2 rounded-lg hover:bg-red-700 transition-colors flex items-center gap-2"
                            >
                                <Plus className="h-4 w-4" />
                                Block
                            </button>
                        </div>
                        
                        {blockedLinks.length > 0 && (
                            <div className="mt-4 space-y-2">
                                {blockedLinks.map((link) => (
                                    <div key={link.id} className="flex items-center justify-between bg-red-50 p-3 rounded-lg">
                                        <div>
                                            <span className="font-mono text-sm text-red-800 break-all">{link.url}</span>
                                            {link.reason && <span className="ml-2 text-xs text-red-600">({link.reason})</span>}
                                        </div>
                                        <button
                                            onClick={() => unblockUrl(link.id)}
                                            className="text-red-600 hover:text-red-800"
                                        >
                                            <Trash2 className="h-4 w-4" />
                                        </button>
                                    </div>
                                ))}
                            </div>
                        )}
                    </div>

                    {/* Block Domain */}
                    <div className="bg-white rounded-xl border border-slate-200 p-6">
                        <h3 className="text-lg font-semibold mb-4 flex items-center gap-2">
                            <Globe className="h-5 w-5 text-red-600" />
                            Block Domain
                        </h3>
                        <div className="flex gap-3">
                            <input
                                type="text"
                                value={newBlockedDomain}
                                onChange={(e) => setNewBlockedDomain(e.target.value)}
                                placeholder="malicious-domain.com"
                                className="flex-1 px-4 py-2 border border-slate-300 rounded-lg focus:ring-2 focus:ring-primary-500"
                            />
                            <input
                                type="text"
                                value={newBlockedDomainReason}
                                onChange={(e) => setNewBlockedDomainReason(e.target.value)}
                                placeholder="Reason (optional)"
                                className="w-48 px-4 py-2 border border-slate-300 rounded-lg focus:ring-2 focus:ring-primary-500"
                            />
                            <button
                                onClick={blockDomain}
                                className="bg-red-600 text-white px-4 py-2 rounded-lg hover:bg-red-700 transition-colors flex items-center gap-2"
                            >
                                <Plus className="h-4 w-4" />
                                Block
                            </button>
                        </div>
                        
                        {blockedDomains.length > 0 && (
                            <div className="mt-4 space-y-2">
                                {blockedDomains.map((domain) => (
                                    <div key={domain.id} className="flex items-center justify-between bg-red-50 p-3 rounded-lg">
                                        <div>
                                            <span className="font-mono text-sm text-red-800">{domain.domain}</span>
                                            {domain.reason && <span className="ml-2 text-xs text-red-600">({domain.reason})</span>}
                                        </div>
                                        <button
                                            onClick={() => unblockDomain(domain.id)}
                                            className="text-red-600 hover:text-red-800"
                                        >
                                            <Trash2 className="h-4 w-4" />
                                        </button>
                                    </div>
                                ))}
                            </div>
                        )}
                    </div>
                </motion.div>
            )}

            {/* Users Tab */}
            {activeTab === 'users' && (
                <motion.div
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                >
                    <div className="bg-white rounded-xl border border-slate-200 overflow-hidden">
                        <table className="w-full">
                            <thead className="bg-slate-50 border-b border-slate-200">
                                <tr>
                                    <th className="text-left px-6 py-3 text-xs font-semibold text-slate-500 uppercase">ID</th>
                                    <th className="text-left px-6 py-3 text-xs font-semibold text-slate-500 uppercase">Email</th>
                                    <th className="text-left px-6 py-3 text-xs font-semibold text-slate-500 uppercase">Status</th>
                                    <th className="text-left px-6 py-3 text-xs font-semibold text-slate-500 uppercase">Role</th>
                                    <th className="text-left px-6 py-3 text-xs font-semibold text-slate-500 uppercase">Joined</th>
                                    <th className="text-right px-6 py-3 text-xs font-semibold text-slate-500 uppercase">Actions</th>
                                </tr>
                            </thead>
                            <tbody className="divide-y divide-slate-200">
                                {users.map((user) => (
                                    <tr key={user.id} className={user.deleted_at ? 'bg-red-50' : ''}>
                                        <td className="px-6 py-4 text-sm text-slate-600">{user.id}</td>
                                        <td className="px-6 py-4 text-sm font-medium text-slate-900">{user.email}</td>
                                        <td className="px-6 py-4">
                                            {user.deleted_at ? (
                                                <span className="px-2 py-1 text-xs font-medium bg-red-100 text-red-700 rounded-full">Deleted</span>
                                            ) : user.email_verified ? (
                                                <span className="px-2 py-1 text-xs font-medium bg-green-100 text-green-700 rounded-full">Verified</span>
                                            ) : (
                                                <span className="px-2 py-1 text-xs font-medium bg-yellow-100 text-yellow-700 rounded-full">Unverified</span>
                                            )}
                                        </td>
                                        <td className="px-6 py-4">
                                            {user.is_admin && (
                                                <span className="px-2 py-1 text-xs font-medium bg-primary-100 text-primary-700 rounded-full">Admin</span>
                                            )}
                                        </td>
                                        <td className="px-6 py-4 text-sm text-slate-500">
                                            {new Date(user.created_at).toLocaleDateString()}
                                        </td>
                                        <td className="px-6 py-4 text-right space-x-2">
                                            {user.deleted_at ? (
                                                <button
                                                    onClick={() => restoreUser(user.id)}
                                                    className="text-green-600 hover:text-green-800 text-sm font-medium"
                                                >
                                                    Restore
                                                </button>
                                            ) : (
                                                <>
                                                    <button
                                                        onClick={() => toggleAdmin(user.id, user.is_admin)}
                                                        className="text-primary-600 hover:text-primary-800 text-sm font-medium"
                                                    >
                                                        {user.is_admin ? 'Remove Admin' : 'Make Admin'}
                                                    </button>
                                                    <button
                                                        onClick={() => deleteUser(user.id)}
                                                        className="text-red-600 hover:text-red-800 text-sm font-medium"
                                                    >
                                                        Delete
                                                    </button>
                                                </>
                                            )}
                                        </td>
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    </div>
                </motion.div>
            )}
        </div>
    );
}

function StatCard({ 
    label, 
    value, 
    icon: Icon, 
    color = 'slate' 
}: { 
    label: string; 
    value: number; 
    icon: React.ComponentType<{ className?: string }>;
    color?: 'slate' | 'green' | 'blue' | 'red';
}) {
    const colors = {
        slate: 'bg-slate-100 text-slate-600',
        green: 'bg-green-100 text-green-600',
        blue: 'bg-blue-100 text-blue-600',
        red: 'bg-red-100 text-red-600',
    };

    return (
        <div className="bg-white rounded-xl border border-slate-200 p-4">
            <div className={`inline-flex p-2 rounded-lg ${colors[color]} mb-3`}>
                <Icon className="h-5 w-5" />
            </div>
            <div className="text-2xl font-bold text-slate-900">{value.toLocaleString()}</div>
            <div className="text-sm text-slate-500">{label}</div>
        </div>
    );
}

