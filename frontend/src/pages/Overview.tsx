import { useCallback, useEffect, useRef, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { Calendar, Clock, Link2, MousePointer, TrendingUp, Zap } from 'lucide-react';
import { motion } from 'framer-motion';
import { Area, AreaChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts';
import { API_ENDPOINTS, authFetch, failureMessage } from '../config/api';
import type { DashboardStats } from '../config/api';
import SEO from '../components/SEO';
import logger from '../utils/logger';
import { formatDayBucketLabel, zeroFillUtcDays } from '../utils/dayBuckets';
import { pluralize } from '../utils/plural';
import { safeLocalStorage } from '../utils/storage';

// Same accent as the link analytics chart.
const ACCENT = 'oklch(0.502 0.176 263)';
const barOpacity = (i: number) => Math.max(0.25, 1 - i * 0.085);

function Skeleton() {
    return (
        <div className="animate-pulse">
            <div className="h-8 w-48 rounded bg-line2/70 mb-8" />
            <div className="grid grid-cols-2 md:grid-cols-3 gap-px bg-line border border-line rounded-2xl overflow-hidden mb-8">
                {[1, 2, 3, 4, 5, 6].map(i => (
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

function StatCard({ title, value, icon: Icon, hint }: {
    title: string;
    value: number;
    icon: React.ComponentType<{ className?: string }>;
    hint?: string;
}) {
    return (
        <div className="bg-surface p-6">
            <div className="flex items-center justify-between mb-2">
                <span className="font-mono text-xs uppercase tracking-[0.14em] text-faint">{title}</span>
                <Icon className="h-4 w-4 text-faint" />
            </div>
            <div className="font-display text-3xl font-extrabold text-ink tracking-tight tabular-nums">
                {value.toLocaleString()}
            </div>
            {hint && <div className="mt-1 text-xs text-faint">{hint}</div>}
        </div>
    );
}

function RankedBars({ title, subtitle, rows }: {
    title: string;
    subtitle: string;
    rows: { label: string; count: number; percentage: number }[];
}) {
    if (rows.length === 0) return null;

    return (
        <div className="rounded-2xl border border-line2 bg-surface p-6 shadow-subtle">
            <div className="mb-4">
                <h3 className="font-display text-lg font-bold text-ink tracking-tight">{title}</h3>
                <p className="mt-1 text-xs text-faint">{subtitle}</p>
            </div>
            <div className="space-y-3">
                {rows.map((row, i) => (
                    <div key={`${row.label}-${i}`} className="flex items-center justify-between gap-3">
                        <span className="truncate text-sm text-ink">{row.label}</span>
                        <div className="flex items-center gap-3 flex-shrink-0">
                            <span className="font-mono text-sm text-muted tabular-nums">{row.count.toLocaleString()}</span>
                            <div className="w-24 h-1.5 rounded-full bg-line overflow-hidden">
                                <div
                                    className="h-full rounded-full"
                                    style={{
                                        width: `${Math.max(0, Math.min(100, row.percentage))}%`,
                                        backgroundColor: ACCENT,
                                        opacity: barOpacity(i),
                                    }}
                                />
                            </div>
                            <span className="w-12 text-right font-mono text-xs text-faint tabular-nums">
                                {row.percentage.toFixed(1)}%
                            </span>
                        </div>
                    </div>
                ))}
            </div>
        </div>
    );
}

export default function Overview() {
    const navigate = useNavigate();
    const [stats, setStats] = useState<DashboardStats | null>(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState('');
    const requestGen = useRef(0);

    const fetchStats = useCallback(async () => {
        const gen = ++requestGen.current;
        setError('');
        setLoading(true);

        try {
            const res = await authFetch(API_ENDPOINTS.dashboardStats);
            if (gen !== requestGen.current) return;

            if (res.ok) {
                const data = await res.json();
                if (gen !== requestGen.current) return;
                setStats(data);
            } else {
                setError('Failed to load overview.');
            }
        } catch (err) {
            if (gen !== requestGen.current) return;
            logger.error('Failed to fetch overview', err);
            setError(failureMessage(err, 'Network error. Please try again.'));
        } finally {
            if (gen === requestGen.current) setLoading(false);
        }
    }, []);

    useEffect(() => {
        const token = safeLocalStorage.getItem('token');
        if (!token) {
            navigate('/login');
            return;
        }
        fetchStats();
    }, [navigate, fetchStats]);

    // The handler already returns one UTC day per row in its window. Fill
    // only the gaps inside that series; the browser clock must not add or
    // drop days. "Today" on the tiles is the server's UTC day.
    const series = stats ? zeroFillUtcDays(stats.clicks_by_day) : [];
    const chartData = series.map(d => ({
        date: formatDayBucketLabel(d.date),
        clicks: d.count,
    }));
    const isEmptyAccount = stats !== null && stats.total_links === 0 && stats.total_clicks === 0;

    return (
        <div className="max-w-6xl mx-auto px-4 sm:px-6 lg:px-8 py-10">
            <SEO title="Overview" noIndex />
            {loading && !stats ? (
                <Skeleton />
            ) : error ? (
                <div role="alert" className="rounded-2xl border border-danger/30 bg-danger/5 p-6 text-danger">
                    <p>{error}</p>
                    <button
                        type="button"
                        onClick={fetchStats}
                        className="mt-4 rounded-lg border border-danger/30 bg-surface px-3 py-1.5 text-sm font-medium text-ink transition-colors hover:border-ink/30"
                    >
                        Retry
                    </button>
                </div>
            ) : stats ? (
                <>
                    <motion.div
                        initial={{ opacity: 0, y: 12 }}
                        animate={{ opacity: 1, y: 0 }}
                        transition={{ duration: 0.4, ease: [0.16, 1, 0.3, 1] }}
                        className="mb-8"
                    >
                        <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary-600">Account</p>
                        <h1 className="mt-2 font-display text-3xl sm:text-4xl font-extrabold text-ink tracking-tight">
                            Overview
                        </h1>
                    </motion.div>

                    <div className="grid grid-cols-2 md:grid-cols-3 gap-px bg-line border border-line rounded-2xl overflow-hidden mb-8">
                        <StatCard title="Total Links" value={stats.total_links} icon={Link2} />
                        <StatCard title="Active Links" value={stats.active_links} icon={Zap} />
                        <StatCard title="Total Clicks" value={stats.total_clicks} icon={MousePointer} />
                        <StatCard title="Today" value={stats.clicks_today} icon={Clock} hint="UTC day" />
                        <StatCard title="Last 7 Days" value={stats.clicks_this_week} icon={TrendingUp} />
                        <StatCard title="Last 30 Days" value={stats.clicks_this_month} icon={Calendar} />
                    </div>

                    {isEmptyAccount ? (
                        <div className="rounded-2xl border border-dashed border-line2 bg-paper py-16 text-center">
                            <div className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-full border border-line bg-surface">
                                <MousePointer className="h-6 w-6 text-faint" />
                            </div>
                            <h2 className="font-display text-lg font-bold text-ink">No links yet</h2>
                            <p className="mt-1 text-muted">Create a short link and its clicks will show up here.</p>
                            <Link
                                to="/dashboard"
                                className="mt-4 inline-flex text-sm font-medium text-primary-600 hover:text-primary-700"
                            >
                                Go to Dashboard
                            </Link>
                        </div>
                    ) : (
                        <>
                            {chartData.length > 0 && (
                                <motion.div
                                    initial={{ opacity: 0, y: 12 }}
                                    animate={{ opacity: 1, y: 0 }}
                                    transition={{ delay: 0.05 }}
                                    className="rounded-2xl border border-line2 bg-surface p-6 shadow-subtle mb-8"
                                >
                                    <div className="mb-4">
                                        <h2 className="font-display text-lg font-bold text-ink tracking-tight">Clicks over time</h2>
                                        <p className="mt-1 text-xs text-faint">Last 30 days</p>
                                    </div>
                                    {/* The table is the chart's text alternative. Both read `series`. */}
                                    <div aria-hidden="true">
                                        <ResponsiveContainer width="100%" height={300}>
                                            <AreaChart data={chartData}>
                                                <defs>
                                                    <linearGradient id="overviewClicks" x1="0" y1="0" x2="0" y2="1">
                                                        <stop offset="5%" stopColor={ACCENT} stopOpacity={0.22} />
                                                        <stop offset="95%" stopColor={ACCENT} stopOpacity={0} />
                                                    </linearGradient>
                                                </defs>
                                                <CartesianGrid strokeDasharray="3 3" stroke="oklch(0.916 0.008 266)" />
                                                <XAxis dataKey="date" stroke="oklch(0.546 0.014 266)" fontSize={12} />
                                                <YAxis stroke="oklch(0.546 0.014 266)" fontSize={12} />
                                                <Tooltip
                                                    contentStyle={{
                                                        backgroundColor: 'oklch(0.999 0.001 266)',
                                                        border: '1px solid oklch(0.852 0.010 266)',
                                                        borderRadius: '12px',
                                                        fontSize: '13px',
                                                    }}
                                                />
                                                <Area
                                                    type="monotone"
                                                    dataKey="clicks"
                                                    stroke={ACCENT}
                                                    strokeWidth={2}
                                                    fillOpacity={1}
                                                    fill="url(#overviewClicks)"
                                                />
                                            </AreaChart>
                                        </ResponsiveContainer>
                                    </div>
                                    <table className="sr-only">
                                        <caption>Clicks by day</caption>
                                        <thead>
                                            <tr>
                                                <th scope="col">Day</th>
                                                <th scope="col">Clicks</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {series.map(day => (
                                                <tr key={day.date}>
                                                    <th scope="row">{formatDayBucketLabel(day.date)}</th>
                                                    <td>{day.count.toLocaleString()}</td>
                                                </tr>
                                            ))}
                                        </tbody>
                                    </table>
                                </motion.div>
                            )}

                            {stats.top_links.length > 0 && (
                                <div className="rounded-2xl border border-line2 bg-surface shadow-subtle overflow-hidden mb-8">
                                    <div className="px-6 py-5 border-b border-line">
                                        <h2 className="font-display text-lg font-bold text-ink tracking-tight">Top Links</h2>
                                        <p className="mt-1 text-xs text-faint">All-time</p>
                                    </div>
                                    <ul className="divide-y divide-line">
                                        {stats.top_links.map(link => (
                                            <li key={link.id}>
                                                <Link
                                                    to={`/analytics/${link.id}`}
                                                    className="flex items-center justify-between gap-3 px-6 py-4 transition-colors hover:bg-primary-50/40"
                                                >
                                                    <span className="min-w-0">
                                                        <span className="block font-mono text-sm font-medium text-ink">{link.code}</span>
                                                        <span className="block truncate font-mono text-xs text-faint">{link.original_url}</span>
                                                    </span>
                                                    <span className="flex-shrink-0 font-mono text-sm text-muted tabular-nums">
                                                        {link.click_count.toLocaleString()} {pluralize(link.click_count, 'click')}
                                                    </span>
                                                </Link>
                                            </li>
                                        ))}
                                    </ul>
                                </div>
                            )}

                            <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                                <RankedBars
                                    title="Top Countries"
                                    subtitle="Last 30 days"
                                    rows={stats.top_countries.map(row => ({
                                        label: row.country || 'Unknown',
                                        count: row.count,
                                        percentage: row.percentage,
                                    }))}
                                />
                                <RankedBars
                                    title="Browsers"
                                    subtitle="Last 30 days"
                                    rows={stats.top_browsers.map(row => ({
                                        label: row.browser || 'Unknown',
                                        count: row.count,
                                        percentage: row.percentage,
                                    }))}
                                />
                            </div>
                        </>
                    )}
                </>
            ) : null}
        </div>
    );
}
