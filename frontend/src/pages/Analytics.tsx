import { useEffect, useState } from 'react';
import { useParams, useNavigate, Link } from 'react-router-dom';
import { ArrowLeft, Globe, Clock, MousePointer, TrendingUp, RefreshCw } from 'lucide-react';
import { motion } from 'framer-motion';
import { AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts';
import { API_ENDPOINTS, authFetch } from '../config/api';
import SEO from '../components/SEO';
import logger from '../utils/logger';

interface DayStats {
    date: string;
    count: number;
}

interface CountryStats {
    country: string;
    count: number;
    percentage: number;
}

interface CityStats {
    city: string;
    country: string | null;
    count: number;
    percentage: number;
}

interface DeviceStats {
    device: string;
    count: number;
    percentage: number;
}

interface BrowserStats {
    browser: string;
    count: number;
    percentage: number;
}

interface OsStats {
    os: string;
    count: number;
    percentage: number;
}

interface RefererStats {
    referer: string;
    count: number;
    percentage: number;
}

interface RecentClick {
    id: number;
    timestamp: string;
    country: string | null;
    city: string | null;
    device: string | null;
    browser: string | null;
    os: string | null;
    referer: string | null;
}

interface LinkStats {
    link_id: number;
    code: string;
    original_url: string;
    total_clicks: number;
    unique_visitors: number;
    clicks_by_day: DayStats[];
    clicks_by_country: CountryStats[];
    clicks_by_city: CityStats[];
    clicks_by_device: DeviceStats[];
    clicks_by_browser: BrowserStats[];
    clicks_by_os: OsStats[];
    clicks_by_referer: RefererStats[];
    recent_clicks: RecentClick[];
}

// Editorial cobalt — a single accent rendered at descending opacity for ranked
// bars, never a rainbow. Index 0 is the strongest, fading down the list.
const ACCENT = 'oklch(0.502 0.176 263)';
const barOpacity = (i: number) => Math.max(0.25, 1 - i * 0.085);

function Skeleton() {
    return (
        <div className="max-w-6xl mx-auto px-4 sm:px-6 lg:px-8 py-10 animate-pulse">
            <div className="h-8 w-64 rounded bg-line2/70 mb-8" />
            <div className="grid grid-cols-1 md:grid-cols-3 gap-px bg-line border border-line rounded-2xl overflow-hidden mb-8">
                {[1, 2, 3].map(i => (
                    <div key={i} className="bg-surface p-6">
                        <div className="h-4 w-24 rounded bg-line2/70 mb-3" />
                        <div className="h-8 w-16 rounded bg-line" />
                    </div>
                ))}
            </div>
            <div className="h-80 rounded-2xl border border-line bg-surface" />
        </div>
    );
}

function StatCard({ title, value, icon: Icon }: { title: string; value: string | number; icon: React.ComponentType<{ className?: string }>; }) {
    return (
        <div className="bg-surface p-6">
            <div className="flex items-center justify-between mb-2">
                <span className="font-mono text-xs uppercase tracking-[0.14em] text-faint">{title}</span>
                <Icon className="h-4 w-4 text-faint" />
            </div>
            <div className="font-display text-3xl font-extrabold text-ink tracking-tight tabular-nums">
                {typeof value === 'number' ? value.toLocaleString() : value}
            </div>
        </div>
    );
}

function StatsTable<T extends object>({ title, data, labelKey, valueKey }: { title: string; data: T[]; labelKey: keyof T; valueKey: keyof T }) {
    if (data.length === 0) return null;

    const sortedData = [...data].sort((a, b) => Number(b[valueKey]) - Number(a[valueKey])).slice(0, 10);
    const total = data.reduce((sum, item) => sum + Number(item[valueKey]), 0);

    return (
        <div className="rounded-2xl border border-line2 bg-surface p-6 shadow-subtle">
            <h3 className="font-display text-lg font-bold text-ink tracking-tight mb-4">{title}</h3>
            <div className="space-y-3">
                {sortedData.map((item, i) => {
                    const value = Number(item[valueKey]);
                    const pct = total > 0 ? (value / total) * 100 : 0;
                    return (
                        <div key={i} className="flex items-center justify-between gap-3">
                            <div className="flex items-center gap-3 min-w-0 flex-1">
                                <span className="truncate text-sm text-ink">{String(item[labelKey] ?? 'Unknown')}</span>
                            </div>
                            <div className="flex items-center gap-3 flex-shrink-0">
                                <span className="font-mono text-sm text-muted tabular-nums">{value.toLocaleString()}</span>
                                <div className="w-24 h-1.5 rounded-full bg-line overflow-hidden">
                                    <div
                                        className="h-full rounded-full"
                                        style={{ width: `${pct}%`, backgroundColor: ACCENT, opacity: barOpacity(i) }}
                                    />
                                </div>
                                <span className="w-12 text-right font-mono text-xs text-faint tabular-nums">
                                    {pct.toFixed(1)}%
                                </span>
                            </div>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}

export default function Analytics() {
    const { id } = useParams<{ id: string }>();
    const navigate = useNavigate();
    const [stats, setStats] = useState<LinkStats | null>(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState('');
    const [days, setDays] = useState(30);

    const fetchStats = async () => {
        try {
            setLoading(true);
            const res = await authFetch(`${API_ENDPOINTS.linkStats(Number(id))}?days=${days}`);

            if (res.ok) {
                const data = await res.json();
                setStats(data);
            } else if (res.status === 403) {
                setError('You do not have permission to view this link\'s analytics.');
            } else if (res.status === 404) {
                setError('Link not found.');
            } else {
                setError('Failed to load analytics.');
            }
        } catch (error) {
            logger.error('Failed to fetch stats', error);
            setError('Network error. Please try again.');
        } finally {
            setLoading(false);
        }
    };

    useEffect(() => {
        const token = localStorage.getItem('token');
        if (!token) {
            navigate('/login');
            return;
        }
        fetchStats();
    }, [id, navigate, days]);

    if (loading && !stats) {
        return <Skeleton />;
    }

    if (error) {
        return (
            <div className="max-w-6xl mx-auto px-4 sm:px-6 lg:px-8 py-10">
                <SEO title="Analytics" noIndex />
                <Link to="/dashboard" className="inline-flex items-center gap-2 text-sm text-muted transition-colors hover:text-ink mb-8">
                    <ArrowLeft className="h-4 w-4" />
                    Back to Dashboard
                </Link>
                <div className="rounded-2xl border border-danger/30 bg-danger/5 p-6 text-danger">
                    {error}
                </div>
            </div>
        );
    }

    if (!stats) return null;

    // Calculate today and this week clicks
    const today = new Date().toISOString().split('T')[0];
    const todayClicks = stats.clicks_by_day.find(d => d.date === today)?.count || 0;

    const weekAgo = new Date();
    weekAgo.setDate(weekAgo.getDate() - 7);
    const weekClicks = stats.clicks_by_day
        .filter(d => new Date(d.date) >= weekAgo)
        .reduce((sum, d) => sum + d.count, 0);

    // Format chart data
    const chartData = stats.clicks_by_day.map(d => ({
        date: new Date(d.date).toLocaleDateString('en-US', { month: 'short', day: 'numeric' }),
        clicks: d.count
    }));

    return (
        <div className="max-w-6xl mx-auto px-4 sm:px-6 lg:px-8 py-10">
            <SEO title={`Analytics — /${stats.code}`} noIndex />
            <div className="flex items-center justify-between mb-8">
                <Link to="/dashboard" className="inline-flex items-center gap-2 text-sm text-muted transition-colors hover:text-ink">
                    <ArrowLeft className="h-4 w-4" />
                    Back to Dashboard
                </Link>
                <div className="flex items-center gap-2">
                    <select
                        value={days}
                        onChange={(e) => setDays(Number(e.target.value))}
                        aria-label="Time range"
                        className="rounded-lg border border-line2 bg-surface px-3 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500"
                    >
                        <option value={7}>Last 7 days</option>
                        <option value={30}>Last 30 days</option>
                        <option value={90}>Last 90 days</option>
                        <option value={365}>Last year</option>
                    </select>
                    <button
                        onClick={fetchStats}
                        className="rounded-lg border border-line2 bg-surface p-2 text-muted transition-colors hover:text-ink hover:border-ink/30"
                        title="Refresh"
                        aria-label="Refresh"
                    >
                        <RefreshCw className={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
                    </button>
                </div>
            </div>

            <motion.div
                initial={{ opacity: 0, y: 12 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.4, ease: [0.16, 1, 0.3, 1] }}
                className="mb-8"
            >
                <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary-600">Analytics</p>
                <h1 className="mt-2 font-display text-3xl sm:text-4xl font-extrabold text-ink tracking-tight">
                    <span className="font-mono text-muted">opn.onl/</span><span className="font-mono">{stats.code}</span>
                </h1>
                <p className="mt-2 truncate max-w-2xl font-mono text-sm text-faint">{stats.original_url}</p>
            </motion.div>

            {/* Key Metrics */}
            <div className="grid grid-cols-2 md:grid-cols-4 gap-px bg-line border border-line rounded-2xl overflow-hidden mb-8">
                <StatCard title="Total Clicks" value={stats.total_clicks} icon={MousePointer} />
                <StatCard title="Unique Visitors" value={stats.unique_visitors} icon={Globe} />
                <StatCard title="Today" value={todayClicks} icon={Clock} />
                <StatCard title="This Week" value={weekClicks} icon={TrendingUp} />
            </div>

            {/* Clicks Over Time Chart */}
            {chartData.length > 0 && (
                <motion.div
                    initial={{ opacity: 0, y: 12 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: 0.05 }}
                    className="rounded-2xl border border-line2 bg-surface p-6 shadow-subtle mb-8"
                >
                    <h3 className="font-display text-lg font-bold text-ink tracking-tight mb-4">Clicks over time</h3>
                    <ResponsiveContainer width="100%" height={300}>
                        <AreaChart data={chartData}>
                            <defs>
                                <linearGradient id="colorClicks" x1="0" y1="0" x2="0" y2="1">
                                    <stop offset="5%" stopColor={ACCENT} stopOpacity={0.22} />
                                    <stop offset="95%" stopColor={ACCENT} stopOpacity={0} />
                                </linearGradient>
                            </defs>
                            <CartesianGrid strokeDasharray="3 3" stroke="oklch(0.916 0.008 266)" />
                            <XAxis dataKey="date" stroke="oklch(0.598 0.014 266)" fontSize={12} />
                            <YAxis stroke="oklch(0.598 0.014 266)" fontSize={12} />
                            <Tooltip
                                contentStyle={{
                                    backgroundColor: 'oklch(0.999 0.001 266)',
                                    border: '1px solid oklch(0.852 0.010 266)',
                                    borderRadius: '12px',
                                    fontSize: '13px'
                                }}
                            />
                            <Area
                                type="monotone"
                                dataKey="clicks"
                                stroke={ACCENT}
                                strokeWidth={2}
                                fillOpacity={1}
                                fill="url(#colorClicks)"
                            />
                        </AreaChart>
                    </ResponsiveContainer>
                </motion.div>
            )}

            {/* Location Stats */}
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-8">
                <motion.div initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.1 }}>
                    <StatsTable title="Top Countries" data={stats.clicks_by_country} labelKey="country" valueKey="count" />
                </motion.div>
                <motion.div initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.12 }}>
                    <StatsTable title="Top Cities" data={stats.clicks_by_city} labelKey="city" valueKey="count" />
                </motion.div>
            </div>

            {/* Device & Browser Stats */}
            <div className="grid grid-cols-1 lg:grid-cols-3 gap-6 mb-8">
                <motion.div initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.14 }}>
                    <StatsTable title="Devices" data={stats.clicks_by_device} labelKey="device" valueKey="count" />
                </motion.div>
                <motion.div initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.16 }}>
                    <StatsTable title="Browsers" data={stats.clicks_by_browser} labelKey="browser" valueKey="count" />
                </motion.div>
                <motion.div initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.18 }}>
                    <StatsTable title="Operating Systems" data={stats.clicks_by_os} labelKey="os" valueKey="count" />
                </motion.div>
            </div>

            {/* Referrers */}
            <motion.div initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.2 }} className="mb-8">
                <StatsTable title="Top Referrers" data={stats.clicks_by_referer} labelKey="referer" valueKey="count" />
            </motion.div>

            {/* Recent Clicks */}
            {stats.recent_clicks.length > 0 && (
                <motion.div
                    initial={{ opacity: 0, y: 12 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: 0.22 }}
                    className="rounded-2xl border border-line2 bg-surface shadow-subtle overflow-hidden"
                >
                    <div className="px-6 py-5 border-b border-line">
                        <h3 className="font-display text-lg font-bold text-ink tracking-tight">Recent clicks</h3>
                    </div>
                    <div className="overflow-x-auto">
                        <table className="w-full">
                            <thead>
                                <tr className="border-b border-line">
                                    <th className="text-left px-6 py-3 font-mono text-xs font-medium uppercase tracking-[0.12em] text-faint">Time</th>
                                    <th className="text-left px-6 py-3 font-mono text-xs font-medium uppercase tracking-[0.12em] text-faint">Location</th>
                                    <th className="text-left px-6 py-3 font-mono text-xs font-medium uppercase tracking-[0.12em] text-faint">Device</th>
                                    <th className="text-left px-6 py-3 font-mono text-xs font-medium uppercase tracking-[0.12em] text-faint">Browser</th>
                                    <th className="text-left px-6 py-3 font-mono text-xs font-medium uppercase tracking-[0.12em] text-faint">Referrer</th>
                                </tr>
                            </thead>
                            <tbody className="divide-y divide-line">
                                {stats.recent_clicks.slice(0, 20).map((click) => (
                                    <tr key={click.id} className="transition-colors hover:bg-primary-50/40">
                                        <td className="px-6 py-4 text-sm text-muted whitespace-nowrap">
                                            {new Date(click.timestamp).toLocaleString()}
                                        </td>
                                        <td className="px-6 py-4 text-sm text-ink">
                                            {click.city && click.country
                                                ? `${click.city}, ${click.country}`
                                                : click.country || 'Unknown'}
                                        </td>
                                        <td className="px-6 py-4 text-sm text-muted">
                                            {click.device || 'Unknown'}
                                        </td>
                                        <td className="px-6 py-4 text-sm text-muted">
                                            {click.browser ? `${click.browser}${click.os ? ` / ${click.os}` : ''}` : 'Unknown'}
                                        </td>
                                        <td className="px-6 py-4 max-w-xs truncate font-mono text-sm text-faint">
                                            {click.referer || 'Direct'}
                                        </td>
                                    </tr>
                                ))}
                            </tbody>
                        </table>
                    </div>
                </motion.div>
            )}

            {/* No data message */}
            {stats.total_clicks === 0 && (
                <motion.div
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    className="rounded-2xl border border-dashed border-line2 bg-paper py-16 text-center"
                >
                    <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-full border border-line bg-surface">
                        <MousePointer className="h-6 w-6 text-faint" />
                    </div>
                    <h3 className="font-display text-lg font-bold text-ink">No clicks yet</h3>
                    <p className="mt-1 text-muted">Share your link to start seeing analytics.</p>
                </motion.div>
            )}
        </div>
    );
}
