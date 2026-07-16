import { useState, useEffect, useCallback } from 'react';
import { useNavigate } from 'react-router-dom';
import { motion } from 'framer-motion';
import {
    Shield, Users, Link2, Ban, Globe, Trash2, Plus,
    RefreshCw, AlertTriangle, Check, X, Database, BarChart2,
    Search, ChevronLeft, ChevronRight, KeyRound, Building2,
    ExternalLink, Copy, RotateCcw, Flame, Lock, Pin, MailCheck,
    ShieldAlert,
} from 'lucide-react';
import {
    ResponsiveContainer, ComposedChart, Area, Line, XAxis, YAxis,
    Tooltip as ChartTooltip, CartesianGrid, Legend,
} from 'recharts';
import { API_ENDPOINTS, authFetch } from '../config/api';

interface AdminStats {
    total_users: number;
    active_users: number;
    verified_users: number;
    admin_users: number;
    total_links: number;
    active_links: number;
    total_clicks: number;
    total_orgs: number;
    users_today: number;
    links_today: number;
    clicks_today: number;
    blocked_links_count: number;
    blocked_domains_count: number;
    blocked_email_domains_count: number;
    disabled_users_count: number;
    suspicious_links_count: number;
}

interface ActivityDay {
    date: string;
    new_users: number;
    new_links: number;
    clicks: number;
}

interface BlockedLink {
    id: number;
    url: string;
    reason: string | null;
    blocked_by: number | null;
    created_at: string;
    affected_links?: number;
}

interface BlockedEmailDomain {
    id: number;
    domain: string;
    reason: string | null;
    blocked_by: number | null;
    created_at: string;
    affected_users?: number;
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
    display_name: string | null;
    is_admin: boolean;
    email_verified: boolean;
    created_at: string;
    deleted_at: string | null;
    disabled_at: string | null;
    disabled_reason: string | null;
    disabled_by: number | null;
    bio_username: string | null;
    bio_enabled: boolean;
    links_count: number;
    total_clicks: number;
    api_keys_count: number;
    passkeys_count: number;
    orgs_owned: number;
}

interface AdminLink {
    id: number;
    code: string;
    original_url: string;
    title: string | null;
    user_id: number | null;
    user_email: string | null;
    org_id: number | null;
    folder_id: number | null;
    click_count: number;
    max_clicks: number | null;
    created_at: string;
    starts_at: string | null;
    expires_at: string | null;
    deleted_at: string | null;
    burned_at: string | null;
    is_pinned: boolean;
    burn_after_reading: boolean;
    safe_link_interstitial: boolean;
    bio_visible: boolean;
    has_password: boolean;
    is_active: boolean;
    inactive_reason: string | null;
    suspicious: boolean;
    suspicion_reason: string | null;
}

interface AdminOrg {
    id: number;
    name: string;
    slug: string;
    owner_id: number;
    owner_email: string | null;
    member_count: number;
    links_count: number;
    created_at: string;
}

type Tab = 'overview' | 'users' | 'links' | 'orgs' | 'blocked';

const PER_PAGE = 25;

const SHORT_BASE = import.meta.env.VITE_FRONTEND_URL ||
    (typeof window !== 'undefined' ? window.location.origin : '');

function formatDate(value: string) {
    const d = new Date(value);
    return Number.isNaN(d.getTime()) ? value : d.toLocaleDateString();
}

export default function Admin() {
    const navigate = useNavigate();
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState('');
    const [success, setSuccess] = useState('');

    const [activeTab, setActiveTab] = useState<Tab>('overview');

    // Overview
    const [stats, setStats] = useState<AdminStats | null>(null);
    const [activity, setActivity] = useState<ActivityDay[]>([]);

    // Users
    const [users, setUsers] = useState<AdminUser[]>([]);
    const [usersTotal, setUsersTotal] = useState(0);
    const [usersPage, setUsersPage] = useState(1);
    const [userSearch, setUserSearch] = useState('');
    const [userStatus, setUserStatus] = useState('all');

    // Links
    const [links, setLinks] = useState<AdminLink[]>([]);
    const [linksTotal, setLinksTotal] = useState(0);
    const [linksPage, setLinksPage] = useState(1);
    const [linkSearch, setLinkSearch] = useState('');
    const [linkStatus, setLinkStatus] = useState('all');
    const [linkSort, setLinkSort] = useState('created_desc');
    const [linkSuspiciousOnly, setLinkSuspiciousOnly] = useState(false);
    const [linkUserFilter, setLinkUserFilter] = useState<{ id: number; email: string } | null>(null);
    const [selectedLinkIds, setSelectedLinkIds] = useState<Set<number>>(new Set());

    // Orgs
    const [orgs, setOrgs] = useState<AdminOrg[]>([]);
    const [orgsTotal, setOrgsTotal] = useState(0);
    const [orgsPage, setOrgsPage] = useState(1);
    const [orgSearch, setOrgSearch] = useState('');

    // Blocked content
    const [blockedLinks, setBlockedLinks] = useState<BlockedLink[]>([]);
    const [blockedDomains, setBlockedDomains] = useState<BlockedDomain[]>([]);
    const [blockedEmailDomains, setBlockedEmailDomains] = useState<BlockedEmailDomain[]>([]);
    const [newBlockedUrl, setNewBlockedUrl] = useState('');
    const [newBlockedUrlReason, setNewBlockedUrlReason] = useState('');
    const [newBlockedDomain, setNewBlockedDomain] = useState('');
    const [newBlockedDomainReason, setNewBlockedDomainReason] = useState('');
    const [newBlockedEmailDomain, setNewBlockedEmailDomain] = useState('');
    const [newBlockedEmailDomainReason, setNewBlockedEmailDomainReason] = useState('');

    const flash = (setter: (v: string) => void, message: string) => {
        setter(message);
    };

    const loadOverview = useCallback(async () => {
        const [statsRes, activityRes] = await Promise.all([
            authFetch(API_ENDPOINTS.adminStats),
            authFetch(`${API_ENDPOINTS.adminActivity}?days=30`),
        ]);

        if (statsRes.status === 403) {
            navigate('/dashboard');
            return false;
        }
        if (statsRes.ok) setStats(await statsRes.json());
        if (activityRes.ok) {
            const data = await activityRes.json();
            setActivity(data.days ?? []);
        }
        return true;
    }, [navigate]);

    const loadUsers = useCallback(async () => {
        const params = new URLSearchParams({
            page: String(usersPage),
            per_page: String(PER_PAGE),
        });
        if (userSearch.trim()) params.set('search', userSearch.trim());
        if (userStatus !== 'all') params.set('status', userStatus);

        const res = await authFetch(`${API_ENDPOINTS.adminUsers}?${params}`);
        if (res.ok) {
            const data = await res.json();
            setUsers(data.users ?? []);
            setUsersTotal(data.total ?? 0);
        }
    }, [usersPage, userSearch, userStatus]);

    const loadLinks = useCallback(async () => {
        const params = new URLSearchParams({
            page: String(linksPage),
            per_page: String(PER_PAGE),
        });
        if (linkSearch.trim()) params.set('search', linkSearch.trim());
        if (linkStatus !== 'all') params.set('status', linkStatus);
        if (linkSuspiciousOnly) params.set('suspicious', 'true');
        if (linkUserFilter) params.set('user_id', String(linkUserFilter.id));
        if (linkSort === 'clicks_desc') {
            params.set('sort', 'clicks');
        } else if (linkSort === 'created_asc') {
            params.set('order', 'asc');
        }

        const res = await authFetch(`${API_ENDPOINTS.adminLinks}?${params}`);
        if (res.ok) {
            const data = await res.json();
            setLinks(data.links ?? []);
            setLinksTotal(data.total ?? 0);
            setSelectedLinkIds(new Set());
        }
    }, [linksPage, linkSearch, linkStatus, linkSort, linkSuspiciousOnly, linkUserFilter]);

    const loadOrgs = useCallback(async () => {
        const params = new URLSearchParams({
            page: String(orgsPage),
            per_page: String(PER_PAGE),
        });
        if (orgSearch.trim()) params.set('search', orgSearch.trim());

        const res = await authFetch(`${API_ENDPOINTS.adminOrgs}?${params}`);
        if (res.ok) {
            const data = await res.json();
            setOrgs(data.orgs ?? []);
            setOrgsTotal(data.total ?? 0);
        }
    }, [orgsPage, orgSearch]);

    const loadBlocked = useCallback(async () => {
        const [linksRes, domainsRes, emailDomainsRes] = await Promise.all([
            authFetch(API_ENDPOINTS.adminBlockedLinks),
            authFetch(API_ENDPOINTS.adminBlockedDomains),
            authFetch(API_ENDPOINTS.adminBlockedEmailDomains),
        ]);
        if (linksRes.ok) setBlockedLinks(await linksRes.json());
        if (domainsRes.ok) setBlockedDomains(await domainsRes.json());
        if (emailDomainsRes.ok) setBlockedEmailDomains(await emailDomainsRes.json());
    }, []);

    // Initial load: overview decides whether the visitor is an admin at all.
    useEffect(() => {
        const token = localStorage.getItem('token');
        if (!token) {
            navigate('/login');
            return;
        }
        (async () => {
            setLoading(true);
            setError('');
            try {
                await loadOverview();
            } catch {
                setError('Failed to load admin data');
            } finally {
                setLoading(false);
            }
        })();
    }, [navigate, loadOverview]);

    // Per-tab data loads, re-run when their filters change.
    useEffect(() => {
        if (activeTab !== 'users') return;
        loadUsers().catch(() => setError('Failed to load users'));
    }, [activeTab, loadUsers]);

    useEffect(() => {
        if (activeTab !== 'links') return;
        loadLinks().catch(() => setError('Failed to load links'));
    }, [activeTab, loadLinks]);

    useEffect(() => {
        if (activeTab !== 'orgs') return;
        loadOrgs().catch(() => setError('Failed to load organizations'));
    }, [activeTab, loadOrgs]);

    useEffect(() => {
        if (activeTab !== 'blocked') return;
        loadBlocked().catch(() => setError('Failed to load blocked content'));
    }, [activeTab, loadBlocked]);

    // ---- Actions ----

    const doAction = async (
        action: () => Promise<Response>,
        successMessage: string,
        reload: () => Promise<unknown> | void,
    ) => {
        setError('');
        try {
            const res = await action();
            const data = await res.json().catch(() => ({}));
            if (res.ok) {
                const affectedSuffix =
                    typeof data.affected_links === 'number'
                        ? ` (${data.affected_links} link${data.affected_links === 1 ? '' : 's'} disabled)`
                        : typeof data.affected_users === 'number'
                          ? ` (${data.affected_users} user${data.affected_users === 1 ? '' : 's'} disabled)`
                          : '';
                flash(setSuccess, `${data.message || successMessage}${affectedSuffix}`);
                await reload();
            } else {
                flash(setError, data.message || 'Action failed');
            }
        } catch {
            flash(setError, 'Action failed');
        }
    };

    const toggleAdmin = (user: AdminUser) => doAction(
        () => authFetch(
            user.is_admin
                ? API_ENDPOINTS.adminUserRemoveAdmin(user.id)
                : API_ENDPOINTS.adminUserMakeAdmin(user.id),
            { method: 'POST' },
        ),
        user.is_admin ? 'Admin status removed' : 'User is now admin',
        loadUsers,
    );

    const deleteUser = (userId: number) => {
        if (!confirm('Delete this user? Their links stop working until restored.')) return;
        doAction(
            () => authFetch(API_ENDPOINTS.adminUser(userId), { method: 'DELETE' }),
            'User deleted',
            loadUsers,
        );
    };

    const restoreUser = (userId: number) => doAction(
        () => authFetch(API_ENDPOINTS.adminUserRestore(userId), { method: 'POST' }),
        'User restored',
        loadUsers,
    );

    const enableUser = (userId: number) => doAction(
        () => authFetch(API_ENDPOINTS.adminUserEnable(userId), { method: 'POST' }),
        'User enabled',
        loadUsers,
    );

    const verifyUserEmail = (userId: number) => doAction(
        () => authFetch(API_ENDPOINTS.adminUserVerifyEmail(userId), { method: 'POST' }),
        'Email verified',
        loadUsers,
    );

    const deleteLink = (link: AdminLink) => {
        if (!confirm(`Delete link /${link.code}? It stops redirecting immediately.`)) return;
        doAction(
            () => authFetch(API_ENDPOINTS.adminLink(link.id), { method: 'DELETE' }),
            'Link deleted',
            loadLinks,
        );
    };

    const restoreLink = (link: AdminLink) => doAction(
        () => authFetch(API_ENDPOINTS.adminLinkRestore(link.id), { method: 'POST' }),
        'Link restored',
        loadLinks,
    );

    const blockDomainFromLink = (link: AdminLink) => {
        let host = link.original_url;
        try {
            host = new URL(link.original_url).host;
        } catch {
            // fall back to the raw URL in the prompt
        }
        if (!confirm(`Block ${host} and delete /${link.code}? All existing and future links to this host stop working.`)) return;
        doAction(
            () => authFetch(API_ENDPOINTS.adminLinkBlockDomain(link.id), { method: 'POST' }),
            'Domain blocked and link deleted',
            loadLinks,
        );
    };

    const bulkDeleteSelectedLinks = () => {
        const ids = [...selectedLinkIds];
        if (ids.length === 0) return;
        if (!confirm(`Delete ${ids.length} selected link(s)? They stop redirecting immediately.`)) return;
        doAction(
            () => authFetch(API_ENDPOINTS.adminLinksBulkDelete, {
                method: 'POST',
                body: JSON.stringify({ ids }),
            }),
            `Deleted ${ids.length} link(s)`,
            loadLinks,
        );
    };

    const bulkRestoreSelectedLinks = () => {
        const ids = [...selectedLinkIds];
        if (ids.length === 0) return;
        doAction(
            () => authFetch(API_ENDPOINTS.adminLinksBulkRestore, {
                method: 'POST',
                body: JSON.stringify({ ids }),
            }),
            `Restored ${ids.length} link(s)`,
            loadLinks,
        );
    };

    const toggleLinkSelected = (id: number) => {
        setSelectedLinkIds((prev) => {
            const next = new Set(prev);
            if (next.has(id)) next.delete(id); else next.add(id);
            return next;
        });
    };

    const toggleSelectAllLinks = () => {
        setSelectedLinkIds((prev) =>
            prev.size === links.length ? new Set() : new Set(links.map((l) => l.id)),
        );
    };

    const copyShortUrl = async (code: string) => {
        try {
            await navigator.clipboard.writeText(`${SHORT_BASE}/${code}`);
            flash(setSuccess, 'Short URL copied');
        } catch {
            flash(setError, 'Could not copy to clipboard');
        }
    };

    const showUserLinks = (user: AdminUser) => {
        setLinkUserFilter({ id: user.id, email: user.email });
        setLinksPage(1);
        setActiveTab('links');
    };

    const blockUrl = () => {
        if (!newBlockedUrl.trim()) return;
        doAction(
            () => authFetch(API_ENDPOINTS.adminBlockedLinks, {
                method: 'POST',
                body: JSON.stringify({
                    url: newBlockedUrl.trim(),
                    reason: newBlockedUrlReason.trim() || null,
                }),
            }),
            'URL blocked successfully',
            () => {
                setNewBlockedUrl('');
                setNewBlockedUrlReason('');
                return loadBlocked();
            },
        );
    };

    const unblockUrl = (id: number) => doAction(
        () => authFetch(API_ENDPOINTS.adminBlockedLink(id), { method: 'DELETE' }),
        'URL unblocked',
        loadBlocked,
    );

    const blockDomain = () => {
        if (!newBlockedDomain.trim()) return;
        doAction(
            () => authFetch(API_ENDPOINTS.adminBlockedDomains, {
                method: 'POST',
                body: JSON.stringify({
                    domain: newBlockedDomain.trim(),
                    reason: newBlockedDomainReason.trim() || null,
                }),
            }),
            'Domain blocked successfully',
            () => {
                setNewBlockedDomain('');
                setNewBlockedDomainReason('');
                return loadBlocked();
            },
        );
    };

    const unblockDomain = (id: number) => doAction(
        () => authFetch(API_ENDPOINTS.adminBlockedDomain(id), { method: 'DELETE' }),
        'Domain unblocked',
        loadBlocked,
    );

    const blockEmailDomain = () => {
        if (!newBlockedEmailDomain.trim()) return;
        doAction(
            () => authFetch(API_ENDPOINTS.adminBlockedEmailDomains, {
                method: 'POST',
                body: JSON.stringify({
                    domain: newBlockedEmailDomain.trim(),
                    reason: newBlockedEmailDomainReason.trim() || null,
                }),
            }),
            'Email domain blocked successfully',
            () => {
                setNewBlockedEmailDomain('');
                setNewBlockedEmailDomainReason('');
                return loadBlocked();
            },
        );
    };

    const unblockEmailDomain = (id: number) => doAction(
        () => authFetch(API_ENDPOINTS.adminBlockedEmailDomain(id), { method: 'DELETE' }),
        'Email domain unblocked',
        loadBlocked,
    );

    const createBackup = () => doAction(
        () => authFetch(API_ENDPOINTS.adminBackup, { method: 'POST' }),
        'Backup created',
        () => undefined,
    );

    if (loading) {
        return (
            <div className="flex items-center justify-center min-h-[60vh]">
                <RefreshCw className="h-8 w-8 animate-spin text-primary-600" />
            </div>
        );
    }

    const tabs: { id: Tab; label: string; icon: React.ComponentType<{ className?: string }> }[] = [
        { id: 'overview', label: 'Overview', icon: BarChart2 },
        { id: 'users', label: 'Users', icon: Users },
        { id: 'links', label: 'Links', icon: Link2 },
        { id: 'orgs', label: 'Organizations', icon: Building2 },
        { id: 'blocked', label: 'Blocked Content', icon: Ban },
    ];

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
                <p className="text-slate-500">Every user, link, and organization on this instance</p>
            </motion.div>

            {/* Alerts */}
            {error && (
                <div className="mb-6 bg-red-50 border border-red-200 rounded-xl p-4 flex items-center gap-3">
                    <AlertTriangle className="h-5 w-5 text-red-600 shrink-0" />
                    <span className="text-red-800">{error}</span>
                    <button onClick={() => setError('')} className="ml-auto" aria-label="Dismiss error">
                        <X className="h-4 w-4 text-red-600" />
                    </button>
                </div>
            )}

            {success && (
                <div className="mb-6 bg-green-50 border border-green-200 rounded-xl p-4 flex items-center gap-3">
                    <Check className="h-5 w-5 text-green-600 shrink-0" />
                    <span className="text-green-800">{success}</span>
                    <button onClick={() => setSuccess('')} className="ml-auto" aria-label="Dismiss message">
                        <X className="h-4 w-4 text-green-600" />
                    </button>
                </div>
            )}

            {/* Tabs */}
            <div className="flex gap-2 mb-8 border-b border-slate-200 overflow-x-auto">
                {tabs.map(({ id, label, icon: Icon }) => (
                    <button
                        key={id}
                        onClick={() => setActiveTab(id)}
                        className={`px-4 py-3 font-medium border-b-2 transition-colors whitespace-nowrap ${
                            activeTab === id
                                ? 'text-primary-600 border-primary-600'
                                : 'text-slate-500 border-transparent hover:text-slate-700'
                        }`}
                    >
                        <Icon className="h-4 w-4 inline mr-2" />
                        {label}
                    </button>
                ))}
            </div>

            {/* Overview Tab */}
            {activeTab === 'overview' && stats && (
                <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} className="space-y-6">
                    <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                        <StatCard label="Total Users" value={stats.total_users} sub={`+${stats.users_today} today`} icon={Users} />
                        <StatCard label="Active Users" value={stats.active_users} sub={`${stats.verified_users} verified · ${stats.admin_users} admins`} icon={Users} color="green" />
                        <StatCard label="Total Links" value={stats.total_links} sub={`+${stats.links_today} today`} icon={Link2} />
                        <StatCard label="Active Links" value={stats.active_links} icon={Link2} color="green" />
                        <StatCard label="Total Clicks" value={stats.total_clicks} sub={`+${stats.clicks_today} today`} icon={BarChart2} color="blue" />
                        <StatCard label="Organizations" value={stats.total_orgs} icon={Building2} color="blue" />
                        <StatCard label="Blocked URLs" value={stats.blocked_links_count} icon={Ban} color="red" />
                        <StatCard label="Blocked Domains" value={stats.blocked_domains_count} icon={Globe} color="red" />
                        <StatCard label="Blocked Email Domains" value={stats.blocked_email_domains_count} icon={MailCheck} color="red" />
                        <StatCard label="Disabled Users" value={stats.disabled_users_count} icon={Users} color="red" />
                    </div>

                    {stats.suspicious_links_count > 0 && (
                        <button
                            onClick={() => {
                                setLinkSuspiciousOnly(true);
                                setLinkStatus('live');
                                setLinkUserFilter(null);
                                setLinksPage(1);
                                setActiveTab('links');
                            }}
                            className="w-full text-left bg-red-50 border border-red-200 rounded-xl p-4 flex items-center gap-3 hover:bg-red-100 transition-colors"
                        >
                            <ShieldAlert className="h-6 w-6 text-red-600 shrink-0" />
                            <div>
                                <div className="font-semibold text-red-800">
                                    {stats.suspicious_links_count.toLocaleString()} suspicious link{stats.suspicious_links_count === 1 ? '' : 's'} live
                                </div>
                                <div className="text-sm text-red-600">
                                    Point at executable files or raw IP addresses. Click to review and take down.
                                </div>
                            </div>
                            <ChevronRight className="h-5 w-5 text-red-400 ml-auto" />
                        </button>
                    )}

                    <div className="bg-white rounded-xl border border-slate-200 p-6">
                        <h3 className="text-lg font-semibold mb-4 flex items-center gap-2">
                            <BarChart2 className="h-5 w-5" />
                            Last 30 days
                        </h3>
                        {activity.length > 0 ? (
                            <div className="h-72">
                                <ResponsiveContainer width="100%" height="100%">
                                    <ComposedChart data={activity} margin={{ top: 4, right: 8, left: -16, bottom: 0 }}>
                                        <CartesianGrid strokeDasharray="3 3" stroke="#e2e8f0" />
                                        <XAxis
                                            dataKey="date"
                                            tick={{ fontSize: 11, fill: '#64748b' }}
                                            tickFormatter={(v: string) => v.slice(5)}
                                        />
                                        <YAxis yAxisId="clicks" tick={{ fontSize: 11, fill: '#64748b' }} allowDecimals={false} />
                                        <YAxis yAxisId="counts" orientation="right" tick={{ fontSize: 11, fill: '#64748b' }} allowDecimals={false} />
                                        <ChartTooltip />
                                        <Legend />
                                        <Area yAxisId="clicks" type="monotone" dataKey="clicks" name="Clicks" stroke="#2563eb" fill="#3b82f6" fillOpacity={0.15} />
                                        <Line yAxisId="counts" type="monotone" dataKey="new_links" name="New links" stroke="#16a34a" dot={false} />
                                        <Line yAxisId="counts" type="monotone" dataKey="new_users" name="New users" stroke="#9333ea" dot={false} />
                                    </ComposedChart>
                                </ResponsiveContainer>
                            </div>
                        ) : (
                            <p className="text-sm text-slate-500">No activity data yet.</p>
                        )}
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

            {/* Users Tab */}
            {activeTab === 'users' && (
                <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} className="space-y-4">
                    <div className="flex flex-wrap gap-3 items-center">
                        <SearchInput
                            value={userSearch}
                            placeholder="Search email, name, or bio username…"
                            onChange={(v) => { setUserSearch(v); setUsersPage(1); }}
                        />
                        <select
                            value={userStatus}
                            onChange={(e) => { setUserStatus(e.target.value); setUsersPage(1); }}
                            aria-label="Filter users"
                            className="px-3 py-2 border border-slate-300 rounded-lg text-sm bg-white"
                        >
                            <option value="all">All users</option>
                            <option value="active">Active</option>
                            <option value="deleted">Deleted</option>
                            <option value="disabled">Disabled</option>
                            <option value="admins">Admins</option>
                            <option value="unverified">Unverified</option>
                        </select>
                        <span className="text-sm text-slate-500 ml-auto">{usersTotal.toLocaleString()} users</span>
                    </div>

                    <div className="bg-white rounded-xl border border-slate-200 overflow-x-auto">
                        <table className="w-full min-w-[900px]">
                            <thead className="bg-slate-50 border-b border-slate-200">
                                <tr>
                                    <Th>ID</Th>
                                    <Th>Email</Th>
                                    <Th>Status</Th>
                                    <Th>Role</Th>
                                    <Th className="text-right">Links</Th>
                                    <Th className="text-right">Clicks</Th>
                                    <Th className="text-right">Keys</Th>
                                    <Th className="text-right">Orgs</Th>
                                    <Th>Joined</Th>
                                    <Th className="text-right">Actions</Th>
                                </tr>
                            </thead>
                            <tbody className="divide-y divide-slate-200">
                                {users.map((user) => (
                                    <tr key={user.id} className={user.deleted_at ? 'bg-red-50' : user.disabled_at ? 'bg-amber-50' : ''}>
                                        <td className="px-4 py-3 text-sm text-slate-600">{user.id}</td>
                                        <td className="px-4 py-3">
                                            <div className="text-sm font-medium text-slate-900">{user.email}</div>
                                            <div className="text-xs text-slate-500">
                                                {user.display_name}
                                                {user.bio_username && (
                                                    <span className="ml-1">
                                                        @{user.bio_username}{user.bio_enabled ? '' : ' (bio off)'}
                                                    </span>
                                                )}
                                            </div>
                                        </td>
                                        <td className="px-4 py-3">
                                            {user.deleted_at ? (
                                                <Badge color="red">Deleted</Badge>
                                            ) : user.disabled_at ? (
                                                <Badge color="red" title={user.disabled_reason ?? 'Disabled'}>Disabled</Badge>
                                            ) : user.email_verified ? (
                                                <Badge color="green">Verified</Badge>
                                            ) : (
                                                <Badge color="yellow">Unverified</Badge>
                                            )}
                                        </td>
                                        <td className="px-4 py-3">
                                            {user.is_admin && <Badge color="primary">Admin</Badge>}
                                        </td>
                                        <td className="px-4 py-3 text-sm text-right">
                                            <button
                                                onClick={() => showUserLinks(user)}
                                                className="text-primary-600 hover:text-primary-800 font-medium"
                                                title="Show this user's links"
                                            >
                                                {user.links_count.toLocaleString()}
                                            </button>
                                        </td>
                                        <td className="px-4 py-3 text-sm text-right text-slate-600">{user.total_clicks.toLocaleString()}</td>
                                        <td className="px-4 py-3 text-sm text-right text-slate-600">
                                            <span title={`${user.api_keys_count} API keys · ${user.passkeys_count} passkeys`}>
                                                <KeyRound className="h-3.5 w-3.5 inline mr-1 text-slate-400" />
                                                {user.api_keys_count + user.passkeys_count}
                                            </span>
                                        </td>
                                        <td className="px-4 py-3 text-sm text-right text-slate-600">{user.orgs_owned}</td>
                                        <td className="px-4 py-3 text-sm text-slate-500">{formatDate(user.created_at)}</td>
                                        <td className="px-4 py-3 text-right space-x-2 whitespace-nowrap">
                                            {user.deleted_at ? (
                                                <button
                                                    onClick={() => restoreUser(user.id)}
                                                    className="text-green-600 hover:text-green-800 text-sm font-medium"
                                                >
                                                    Restore
                                                </button>
                                            ) : (
                                                <>
                                                    {user.disabled_at && (
                                                        <button
                                                            onClick={() => enableUser(user.id)}
                                                            className="text-green-600 hover:text-green-800 text-sm font-medium"
                                                            title={user.disabled_reason ?? 'Enable user'}
                                                        >
                                                            Enable
                                                        </button>
                                                    )}
                                                    {!user.email_verified && (
                                                        <button
                                                            onClick={() => verifyUserEmail(user.id)}
                                                            className="text-green-600 hover:text-green-800 text-sm font-medium"
                                                            title="Mark email as verified"
                                                        >
                                                            <MailCheck className="h-4 w-4 inline" /> Verify
                                                        </button>
                                                    )}
                                                    <button
                                                        onClick={() => toggleAdmin(user)}
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
                                {users.length === 0 && (
                                    <tr>
                                        <td colSpan={10} className="px-4 py-8 text-center text-sm text-slate-500">
                                            No users match this filter.
                                        </td>
                                    </tr>
                                )}
                            </tbody>
                        </table>
                    </div>

                    <Pagination page={usersPage} total={usersTotal} onChange={setUsersPage} />
                </motion.div>
            )}

            {/* Links Tab */}
            {activeTab === 'links' && (
                <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} className="space-y-4">
                    <div className="flex flex-wrap gap-3 items-center">
                        <SearchInput
                            value={linkSearch}
                            placeholder="Search code, URL, title, or owner email…"
                            onChange={(v) => { setLinkSearch(v); setLinksPage(1); }}
                        />
                        <select
                            value={linkStatus}
                            onChange={(e) => { setLinkStatus(e.target.value); setLinksPage(1); }}
                            aria-label="Filter links"
                            className="px-3 py-2 border border-slate-300 rounded-lg text-sm bg-white"
                        >
                            <option value="all">All links</option>
                            <option value="live">Live</option>
                            <option value="deleted">Deleted</option>
                        </select>
                        <select
                            value={linkSort}
                            onChange={(e) => { setLinkSort(e.target.value); setLinksPage(1); }}
                            aria-label="Sort links"
                            className="px-3 py-2 border border-slate-300 rounded-lg text-sm bg-white"
                        >
                            <option value="created_desc">Newest first</option>
                            <option value="created_asc">Oldest first</option>
                            <option value="clicks_desc">Most clicks</option>
                        </select>
                        <button
                            onClick={() => { setLinkSuspiciousOnly((v) => !v); setLinksPage(1); }}
                            aria-pressed={linkSuspiciousOnly}
                            className={`inline-flex items-center gap-1.5 px-3 py-2 rounded-lg text-sm font-medium border transition-colors ${
                                linkSuspiciousOnly
                                    ? 'bg-red-600 text-white border-red-600'
                                    : 'bg-white text-red-700 border-red-200 hover:bg-red-50'
                            }`}
                        >
                            <ShieldAlert className="h-4 w-4" />
                            Suspicious only
                        </button>
                        {linkUserFilter && (
                            <span className="inline-flex items-center gap-1 px-3 py-1.5 bg-primary-50 text-primary-700 rounded-full text-sm">
                                {linkUserFilter.email}
                                <button
                                    onClick={() => { setLinkUserFilter(null); setLinksPage(1); }}
                                    aria-label="Clear user filter"
                                >
                                    <X className="h-3.5 w-3.5" />
                                </button>
                            </span>
                        )}
                        <span className="text-sm text-slate-500 ml-auto">{linksTotal.toLocaleString()} links</span>
                    </div>

                    {selectedLinkIds.size > 0 && (
                        <div className="flex items-center gap-3 bg-slate-800 text-white rounded-xl px-4 py-3">
                            <span className="text-sm font-medium">{selectedLinkIds.size} selected</span>
                            <button
                                onClick={bulkDeleteSelectedLinks}
                                className="inline-flex items-center gap-1.5 bg-red-600 hover:bg-red-700 px-3 py-1.5 rounded-lg text-sm font-medium transition-colors"
                            >
                                <Trash2 className="h-4 w-4" /> Delete selected
                            </button>
                            <button
                                onClick={bulkRestoreSelectedLinks}
                                className="inline-flex items-center gap-1.5 bg-slate-600 hover:bg-slate-500 px-3 py-1.5 rounded-lg text-sm font-medium transition-colors"
                            >
                                <RotateCcw className="h-4 w-4" /> Restore selected
                            </button>
                            <button
                                onClick={() => setSelectedLinkIds(new Set())}
                                className="ml-auto text-sm text-slate-300 hover:text-white"
                            >
                                Clear
                            </button>
                        </div>
                    )}

                    <div className="bg-white rounded-xl border border-slate-200 overflow-x-auto">
                        <table className="w-full min-w-[1050px]">
                            <thead className="bg-slate-50 border-b border-slate-200">
                                <tr>
                                    <th className="px-4 py-3 w-10">
                                        <input
                                            type="checkbox"
                                            aria-label="Select all links on this page"
                                            checked={links.length > 0 && selectedLinkIds.size === links.length}
                                            ref={(el) => {
                                                if (el) el.indeterminate = selectedLinkIds.size > 0 && selectedLinkIds.size < links.length;
                                            }}
                                            onChange={toggleSelectAllLinks}
                                        />
                                    </th>
                                    <Th>Code</Th>
                                    <Th>Destination</Th>
                                    <Th>Owner</Th>
                                    <Th className="text-right">Clicks</Th>
                                    <Th>Flags</Th>
                                    <Th>Status</Th>
                                    <Th>Created</Th>
                                    <Th className="text-right">Actions</Th>
                                </tr>
                            </thead>
                            <tbody className="divide-y divide-slate-200">
                                {links.map((link) => (
                                    <tr
                                        key={link.id}
                                        className={
                                            link.deleted_at ? 'bg-red-50'
                                            : link.suspicious ? 'bg-red-50/40'
                                            : selectedLinkIds.has(link.id) ? 'bg-primary-50/50'
                                            : ''
                                        }
                                    >
                                        <td className="px-4 py-3">
                                            <input
                                                type="checkbox"
                                                aria-label={`Select link ${link.code}`}
                                                checked={selectedLinkIds.has(link.id)}
                                                onChange={() => toggleLinkSelected(link.id)}
                                            />
                                        </td>
                                        <td className="px-4 py-3 whitespace-nowrap">
                                            <span className="font-mono text-sm text-slate-900">/{link.code}</span>
                                            <button
                                                onClick={() => copyShortUrl(link.code)}
                                                className="ml-2 text-slate-400 hover:text-slate-600 align-middle"
                                                title="Copy short URL"
                                                aria-label={`Copy short URL for ${link.code}`}
                                            >
                                                <Copy className="h-3.5 w-3.5 inline" />
                                            </button>
                                        </td>
                                        <td className="px-4 py-3 max-w-[320px]">
                                            {link.title && <div className="text-sm font-medium text-slate-900 truncate">{link.title}</div>}
                                            <a
                                                href={link.original_url}
                                                target="_blank"
                                                rel="noopener noreferrer nofollow"
                                                className="text-xs text-slate-500 hover:text-primary-600 truncate block"
                                                title={link.original_url}
                                            >
                                                {link.original_url}
                                                <ExternalLink className="h-3 w-3 inline ml-1" />
                                            </a>
                                            {link.suspicious && (
                                                <span className="mt-1 inline-flex items-center gap-1 text-xs font-medium text-red-700">
                                                    <ShieldAlert className="h-3.5 w-3.5" />
                                                    {link.suspicion_reason ?? 'Suspicious'}
                                                </span>
                                            )}
                                        </td>
                                        <td className="px-4 py-3 text-sm text-slate-600 whitespace-nowrap">
                                            {link.user_email ?? <span className="text-slate-400">anonymous</span>}
                                            {link.org_id != null && <Badge color="blue">org</Badge>}
                                        </td>
                                        <td className="px-4 py-3 text-sm text-right text-slate-600">
                                            {link.click_count.toLocaleString()}
                                            {link.max_clicks != null && <span className="text-slate-400"> / {link.max_clicks}</span>}
                                        </td>
                                        <td className="px-4 py-3 whitespace-nowrap space-x-1">
                                            {link.has_password && <IconFlag title="Password protected"><Lock className="h-3.5 w-3.5" /></IconFlag>}
                                            {link.burn_after_reading && <IconFlag title="Burn after reading"><Flame className="h-3.5 w-3.5" /></IconFlag>}
                                            {link.is_pinned && <IconFlag title="Pinned"><Pin className="h-3.5 w-3.5" /></IconFlag>}
                                            {link.safe_link_interstitial && <IconFlag title="Safe-link interstitial"><Shield className="h-3.5 w-3.5" /></IconFlag>}
                                            {link.bio_visible && <IconFlag title="Shown on bio page"><Globe className="h-3.5 w-3.5" /></IconFlag>}
                                        </td>
                                        <td className="px-4 py-3">
                                            {link.deleted_at ? (
                                                <Badge color="red">Deleted</Badge>
                                            ) : link.is_active ? (
                                                <Badge color="green">Active</Badge>
                                            ) : (
                                                <Badge color="yellow" title={link.inactive_reason ?? undefined}>Inactive</Badge>
                                            )}
                                        </td>
                                        <td className="px-4 py-3 text-sm text-slate-500 whitespace-nowrap">{formatDate(link.created_at)}</td>
                                        <td className="px-4 py-3 text-right whitespace-nowrap space-x-2">
                                            {link.deleted_at ? (
                                                <button
                                                    onClick={() => restoreLink(link)}
                                                    className="text-green-600 hover:text-green-800 text-sm font-medium"
                                                >
                                                    <RotateCcw className="h-4 w-4 inline" /> Restore
                                                </button>
                                            ) : (
                                                <>
                                                    <button
                                                        onClick={() => blockDomainFromLink(link)}
                                                        className="text-red-600 hover:text-red-800 text-sm font-medium"
                                                        title="Block this destination's domain and delete the link"
                                                    >
                                                        <Ban className="h-4 w-4 inline" /> Block domain
                                                    </button>
                                                    <button
                                                        onClick={() => deleteLink(link)}
                                                        className="text-red-600 hover:text-red-800 text-sm font-medium"
                                                    >
                                                        Delete
                                                    </button>
                                                </>
                                            )}
                                        </td>
                                    </tr>
                                ))}
                                {links.length === 0 && (
                                    <tr>
                                        <td colSpan={9} className="px-4 py-8 text-center text-sm text-slate-500">
                                            No links match this filter.
                                        </td>
                                    </tr>
                                )}
                            </tbody>
                        </table>
                    </div>

                    <Pagination page={linksPage} total={linksTotal} onChange={setLinksPage} />
                </motion.div>
            )}

            {/* Organizations Tab */}
            {activeTab === 'orgs' && (
                <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} className="space-y-4">
                    <div className="flex flex-wrap gap-3 items-center">
                        <SearchInput
                            value={orgSearch}
                            placeholder="Search name or slug…"
                            onChange={(v) => { setOrgSearch(v); setOrgsPage(1); }}
                        />
                        <span className="text-sm text-slate-500 ml-auto">{orgsTotal.toLocaleString()} organizations</span>
                    </div>

                    <div className="bg-white rounded-xl border border-slate-200 overflow-x-auto">
                        <table className="w-full min-w-[700px]">
                            <thead className="bg-slate-50 border-b border-slate-200">
                                <tr>
                                    <Th>ID</Th>
                                    <Th>Name</Th>
                                    <Th>Owner</Th>
                                    <Th className="text-right">Members</Th>
                                    <Th className="text-right">Links</Th>
                                    <Th>Created</Th>
                                </tr>
                            </thead>
                            <tbody className="divide-y divide-slate-200">
                                {orgs.map((org) => (
                                    <tr key={org.id}>
                                        <td className="px-4 py-3 text-sm text-slate-600">{org.id}</td>
                                        <td className="px-4 py-3">
                                            <div className="text-sm font-medium text-slate-900">{org.name}</div>
                                            <div className="text-xs text-slate-500 font-mono">{org.slug}</div>
                                        </td>
                                        <td className="px-4 py-3 text-sm text-slate-600">{org.owner_email ?? `user #${org.owner_id}`}</td>
                                        <td className="px-4 py-3 text-sm text-right text-slate-600">{org.member_count}</td>
                                        <td className="px-4 py-3 text-sm text-right text-slate-600">{org.links_count}</td>
                                        <td className="px-4 py-3 text-sm text-slate-500">{formatDate(org.created_at)}</td>
                                    </tr>
                                ))}
                                {orgs.length === 0 && (
                                    <tr>
                                        <td colSpan={6} className="px-4 py-8 text-center text-sm text-slate-500">
                                            No organizations found.
                                        </td>
                                    </tr>
                                )}
                            </tbody>
                        </table>
                    </div>

                    <Pagination page={orgsPage} total={orgsTotal} onChange={setOrgsPage} />
                </motion.div>
            )}

            {/* Blocked Content Tab */}
            {activeTab === 'blocked' && (
                <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} className="space-y-8">
                    {/* Block URL */}
                    <div className="bg-white rounded-xl border border-slate-200 p-6">
                        <h3 className="text-lg font-semibold mb-4 flex items-center gap-2">
                            <Ban className="h-5 w-5 text-red-600" />
                            Block URL
                        </h3>
                        <div className="flex flex-wrap gap-3">
                            <input
                                type="url"
                                value={newBlockedUrl}
                                onChange={(e) => setNewBlockedUrl(e.target.value)}
                                placeholder="https://example.com/malicious"
                                className="flex-1 min-w-[220px] px-4 py-2 border border-slate-300 rounded-lg focus:ring-2 focus:ring-primary-500"
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
                                            aria-label={`Unblock ${link.url}`}
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
                        <div className="flex flex-wrap gap-3">
                            <input
                                type="text"
                                value={newBlockedDomain}
                                onChange={(e) => setNewBlockedDomain(e.target.value)}
                                placeholder="malicious-domain.com"
                                className="flex-1 min-w-[220px] px-4 py-2 border border-slate-300 rounded-lg focus:ring-2 focus:ring-primary-500"
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
                                            aria-label={`Unblock ${domain.domain}`}
                                        >
                                            <Trash2 className="h-4 w-4" />
                                        </button>
                                    </div>
                                ))}
                            </div>
                        )}
                    </div>

                    {/* Block Email Domain */}
                    <div className="bg-white rounded-xl border border-slate-200 p-6">
                        <h3 className="text-lg font-semibold mb-2 flex items-center gap-2">
                            <MailCheck className="h-5 w-5 text-red-600" />
                            Block Email Domain
                        </h3>
                        <p className="text-sm text-slate-500 mb-4">
                            Rejects future signups/resets for this email domain and disables existing matching users without deleting their data.
                        </p>
                        <div className="flex flex-wrap gap-3">
                            <input
                                type="text"
                                value={newBlockedEmailDomain}
                                onChange={(e) => setNewBlockedEmailDomain(e.target.value)}
                                placeholder="throwaway-mail.com"
                                className="flex-1 min-w-[220px] px-4 py-2 border border-slate-300 rounded-lg focus:ring-2 focus:ring-primary-500"
                            />
                            <input
                                type="text"
                                value={newBlockedEmailDomainReason}
                                onChange={(e) => setNewBlockedEmailDomainReason(e.target.value)}
                                placeholder="Reason (optional)"
                                className="w-48 px-4 py-2 border border-slate-300 rounded-lg focus:ring-2 focus:ring-primary-500"
                            />
                            <button
                                onClick={blockEmailDomain}
                                className="bg-red-600 text-white px-4 py-2 rounded-lg hover:bg-red-700 transition-colors flex items-center gap-2"
                            >
                                <Plus className="h-4 w-4" />
                                Block
                            </button>
                        </div>

                        {blockedEmailDomains.length > 0 && (
                            <div className="mt-4 space-y-2">
                                {blockedEmailDomains.map((domain) => (
                                    <div key={domain.id} className="flex items-center justify-between bg-red-50 p-3 rounded-lg">
                                        <div>
                                            <span className="font-mono text-sm text-red-800">{domain.domain}</span>
                                            {domain.reason && <span className="ml-2 text-xs text-red-600">({domain.reason})</span>}
                                        </div>
                                        <button
                                            onClick={() => unblockEmailDomain(domain.id)}
                                            className="text-red-600 hover:text-red-800"
                                            aria-label={`Unblock email domain ${domain.domain}`}
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
        </div>
    );
}

function Th({ children, className = '' }: { children?: React.ReactNode; className?: string }) {
    return (
        <th className={`text-left px-4 py-3 text-xs font-semibold text-slate-500 uppercase ${className}`}>
            {children}
        </th>
    );
}

function Badge({
    children,
    color,
    title,
}: {
    children: React.ReactNode;
    color: 'green' | 'yellow' | 'red' | 'primary' | 'blue';
    title?: string;
}) {
    const colors = {
        green: 'bg-green-100 text-green-700',
        yellow: 'bg-yellow-100 text-yellow-700',
        red: 'bg-red-100 text-red-700',
        primary: 'bg-primary-100 text-primary-700',
        blue: 'bg-blue-100 text-blue-700',
    };
    return (
        <span title={title} className={`px-2 py-1 text-xs font-medium rounded-full ${colors[color]}`}>
            {children}
        </span>
    );
}

function IconFlag({ children, title }: { children: React.ReactNode; title: string }) {
    return (
        <span title={title} className="inline-flex items-center text-slate-400" aria-label={title}>
            {children}
        </span>
    );
}

function SearchInput({
    value,
    placeholder,
    onChange,
}: {
    value: string;
    placeholder: string;
    onChange: (value: string) => void;
}) {
    // Local state + debounce so each keystroke doesn't fire a request. The
    // parent only ever changes `value` through onChange, so no sync-back is
    // needed after mount.
    const [draft, setDraft] = useState(value);

    useEffect(() => {
        if (draft === value) return;
        const t = setTimeout(() => onChange(draft), 300);
        return () => clearTimeout(t);
    }, [draft, value, onChange]);

    return (
        <div className="relative flex-1 min-w-[240px] max-w-md">
            <Search className="h-4 w-4 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2" />
            <input
                type="search"
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                placeholder={placeholder}
                className="w-full pl-9 pr-4 py-2 border border-slate-300 rounded-lg text-sm focus:ring-2 focus:ring-primary-500"
            />
        </div>
    );
}

function Pagination({
    page,
    total,
    onChange,
}: {
    page: number;
    total: number;
    onChange: (page: number) => void;
}) {
    const pages = Math.max(1, Math.ceil(total / PER_PAGE));
    if (pages <= 1) return null;

    return (
        <div className="flex items-center justify-end gap-3 text-sm text-slate-600">
            <button
                onClick={() => onChange(page - 1)}
                disabled={page <= 1}
                className="p-2 rounded-lg border border-slate-200 disabled:opacity-40 hover:bg-slate-50"
                aria-label="Previous page"
            >
                <ChevronLeft className="h-4 w-4" />
            </button>
            <span>Page {page} of {pages}</span>
            <button
                onClick={() => onChange(page + 1)}
                disabled={page >= pages}
                className="p-2 rounded-lg border border-slate-200 disabled:opacity-40 hover:bg-slate-50"
                aria-label="Next page"
            >
                <ChevronRight className="h-4 w-4" />
            </button>
        </div>
    );
}

function StatCard({
    label,
    value,
    sub,
    icon: Icon,
    color = 'slate',
}: {
    label: string;
    value: number;
    sub?: string;
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
            {sub && <div className="text-xs text-slate-400 mt-1">{sub}</div>}
        </div>
    );
}
