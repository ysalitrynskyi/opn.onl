import { useEffect, useState } from 'react';
import { useParams, useNavigate, Link } from 'react-router-dom';
import { ArrowLeft, Globe, Clock, MousePointer, TrendingUp, RefreshCw } from 'lucide-react';
import { motion } from 'framer-motion';
import { AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer } from 'recharts';
import { API_ENDPOINTS, authFetch } from '../config/api';
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

const COLORS = ['#3b82f6', '#10b981', '#f59e0b', '#ef4444', '#8b5cf6', '#ec4899', '#14b8a6', '#f97316'];

function Skeleton() {
    return (
        <div className="max-w-6xl mx-auto px-4 sm:px-6 lg:px-8 py-8 animate-pulse">
            <div className="h-8 bg-slate-200 rounded w-64 mb-8" />
            <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-8">
                {[1, 2, 3].map(i => (
                    <div key={i} className="bg-white p-6 rounded-xl border border-slate-200">
                        <div className="h-4 bg-slate-200 rounded w-24 mb-2" />
                        <div className="h-8 bg-slate-200 rounded w-16" />
                    </div>
                ))}
            </div>
            <div className="bg-white p-6 rounded-xl border border-slate-200 h-80" />
        </div>
    );
}

function StatCard({ title, value, icon: Icon, trend }: { title: string; value: string | number; icon: React.ComponentType<{className?: string}>; trend?: string }) {
    return (
        <motion.div 
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            className="bg-white p-6 rounded-xl border border-slate-200 shadow-sm"
        >
            <div className="flex items-center justify-between mb-2">
                <span className="text-slate-500 text-sm font-medium">{title}</span>
                <Icon className="h-5 w-5 text-slate-400" />
            </div>
            <div className="text-3xl font-bold text-slate-900">{typeof value === 'number' ? value.toLocaleString() : value}</div>
            {trend && <div className="text-sm text-green-600 mt-1">{trend}</div>}
        </motion.div>
    );
}

function StatsTable({ title, data, labelKey, valueKey }: { title: string; data: any[]; labelKey: string; valueKey: string }) {
    if (data.length === 0) return null;
    
    const sortedData = [...data].sort((a, b) => b[valueKey] - a[valueKey]).slice(0, 10);
    const total = data.reduce((sum, item) => sum + item[valueKey], 0);
    
    return (
        <div className="bg-white p-6 rounded-xl border border-slate-200 shadow-sm">
            <h3 className="text-lg font-semibold text-slate-900 mb-4">{title}</h3>
            <div className="space-y-3">
                {sortedData.map((item, i) => (
                    <div key={i} className="flex items-center justify-between">
                        <div className="flex items-center gap-3 min-w-0 flex-1">
                            <div 
                                className="w-3 h-3 rounded-full flex-shrink-0" 
                                style={{ backgroundColor: COLORS[i % COLORS.length] }}
                            />
                            <span className="text-slate-700 truncate">{item[labelKey] || 'Unknown'}</span>
                        </div>
                        <div className="flex items-center gap-3 flex-shrink-0">
                            <span className="text-slate-500 text-sm">{item[valueKey].toLocaleString()}</span>
                            <div className="w-24 bg-slate-100 rounded-full h-2">
                                <div 
                                    className="h-2 rounded-full" 
                                    style={{ 
                                        width: `${(item[valueKey] / total) * 100}%`,
                                        backgroundColor: COLORS[i % COLORS.length]
                                    }}
                                />
                            </div>
                            <span className="text-slate-400 text-xs w-12 text-right">
                                {((item[valueKey] / total) * 100).toFixed(1)}%
                            </span>
                        </div>
                    </div>
                ))}
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
            <div className="max-w-6xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
                <Link to="/dashboard" className="inline-flex items-center gap-2 text-slate-600 hover:text-slate-900 mb-8">
                    <ArrowLeft className="h-4 w-4" />
                    Back to Dashboard
                </Link>
                <div className="bg-red-50 border border-red-200 rounded-xl p-6 text-red-700">
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
        <div className="max-w-6xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
            <div className="flex items-center justify-between mb-8">
                <Link to="/dashboard" className="inline-flex items-center gap-2 text-slate-600 hover:text-slate-900">
                    <ArrowLeft className="h-4 w-4" />
                    Back to Dashboard
                </Link>
                <div className="flex items-center gap-3">
                    <select
                        value={days}
                        onChange={(e) => setDays(Number(e.target.value))}
                        className="border border-slate-300 rounded-lg px-3 py-2 text-sm focus:ring-primary-500 focus:border-primary-500"
                    >
                        <option value={7}>Last 7 days</option>
                        <option value={30}>Last 30 days</option>
                        <option value={90}>Last 90 days</option>
                        <option value={365}>Last year</option>
                    </select>
                    <button
                        onClick={fetchStats}
                        className="p-2 text-slate-500 hover:text-slate-700 hover:bg-slate-100 rounded-lg"
                        title="Refresh"
                    >
                        <RefreshCw className={`h-4 w-4 ${loading ? 'animate-spin' : ''}`} />
                    </button>
                </div>
            </div>

            <motion.div
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
                className="mb-8"
            >
                <h1 className="text-2xl font-bold text-slate-900 mb-2">
                    Analytics for /{stats.code}
                </h1>
                <p className="text-slate-500 text-sm truncate max-w-2xl">{stats.original_url}</p>
            </motion.div>

            {/* Key Metrics */}
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
                <StatCard 
                    title="Total Clicks" 
                    value={stats.total_clicks} 
                    icon={MousePointer}
                />
                <StatCard 
                    title="Unique Visitors" 
                    value={stats.unique_visitors} 
                    icon={Globe}
                />
                <StatCard 
                    title="Today" 
                    value={todayClicks} 
                    icon={Clock}
                />
                <StatCard 
                    title="This Week" 
                    value={weekClicks} 
                    icon={TrendingUp}
                />
            </div>

            {/* Clicks Over Time Chart */}
            {chartData.length > 0 && (
                <motion.div 
                    initial={{ opacity: 0, y: 20 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: 0.1 }}
                    className="bg-white p-6 rounded-xl border border-slate-200 shadow-sm mb-8"
                >
                    <h3 className="text-lg font-semibold text-slate-900 mb-4">Clicks Over Time</h3>
                    <ResponsiveContainer width="100%" height={300}>
                        <AreaChart data={chartData}>
                            <defs>
                                <linearGradient id="colorClicks" x1="0" y1="0" x2="0" y2="1">
                                    <stop offset="5%" stopColor="#3b82f6" stopOpacity={0.3}/>
                                    <stop offset="95%" stopColor="#3b82f6" stopOpacity={0}/>
                                </linearGradient>
                            </defs>
                            <CartesianGrid strokeDasharray="3 3" stroke="#e2e8f0" />
                            <XAxis dataKey="date" stroke="#94a3b8" fontSize={12} />
                            <YAxis stroke="#94a3b8" fontSize={12} />
                            <Tooltip 
                                contentStyle={{ 
                                    backgroundColor: 'white', 
                                    border: '1px solid #e2e8f0',
                                    borderRadius: '8px'
                                }}
                            />
                            <Area 
                                type="monotone" 
                                dataKey="clicks" 
                                stroke="#3b82f6" 
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
                <motion.div 
                    initial={{ opacity: 0, y: 20 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: 0.2 }}
                >
                    <StatsTable 
                        title="Top Countries" 
                        data={stats.clicks_by_country} 
                        labelKey="country" 
                        valueKey="count" 
                    />
                </motion.div>
                <motion.div 
                    initial={{ opacity: 0, y: 20 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: 0.25 }}
                >
                    <StatsTable 
                        title="Top Cities" 
                        data={stats.clicks_by_city} 
                        labelKey="city" 
                        valueKey="count" 
                    />
                </motion.div>
            </div>

            {/* Device & Browser Stats */}
            <div className="grid grid-cols-1 lg:grid-cols-3 gap-6 mb-8">
                <motion.div 
                    initial={{ opacity: 0, y: 20 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: 0.3 }}
                >
                    <StatsTable 
                        title="Devices" 
                        data={stats.clicks_by_device} 
                        labelKey="device" 
                        valueKey="count" 
                    />
                </motion.div>
                <motion.div 
                    initial={{ opacity: 0, y: 20 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: 0.35 }}
                >
                    <StatsTable 
                        title="Browsers" 
                        data={stats.clicks_by_browser} 
                        labelKey="browser" 
                        valueKey="count" 
                    />
                </motion.div>
                <motion.div 
                    initial={{ opacity: 0, y: 20 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: 0.4 }}
                >
                    <StatsTable 
                        title="Operating Systems" 
                        data={stats.clicks_by_os} 
                        labelKey="os" 
                        valueKey="count" 
                    />
                </motion.div>
            </div>

            {/* Referrers */}
            <motion.div 
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ delay: 0.45 }}
                className="mb-8"
            >
                <StatsTable 
                    title="Top Referrers" 
                    data={stats.clicks_by_referer} 
                    labelKey="referer" 
                    valueKey="count" 
                />
            </motion.div>

            {/* Recent Clicks */}
            {stats.recent_clicks.length > 0 && (
                <motion.div 
                    initial={{ opacity: 0, y: 20 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: 0.5 }}
                    className="bg-white rounded-xl border border-slate-200 shadow-sm overflow-hidden"
                >
                    <div className="p-6 border-b border-slate-200">
                        <h3 className="text-lg font-semibold text-slate-900">Recent Clicks</h3>
                    </div>
                    <div className="overflow-x-auto">
                        <table className="w-full">
                            <thead className="bg-slate-50">
                                <tr>
                                    <th className="text-left px-6 py-3 text-xs font-semibold text-slate-500 uppercase">Time</th>
                                    <th className="text-left px-6 py-3 text-xs font-semibold text-slate-500 uppercase">Location</th>
                                    <th className="text-left px-6 py-3 text-xs font-semibold text-slate-500 uppercase">Device</th>
                                    <th className="text-left px-6 py-3 text-xs font-semibold text-slate-500 uppercase">Browser</th>
                                    <th className="text-left px-6 py-3 text-xs font-semibold text-slate-500 uppercase">Referrer</th>
                                </tr>
                            </thead>
                            <tbody className="divide-y divide-slate-200">
                                {stats.recent_clicks.slice(0, 20).map((click) => (
                                    <tr key={click.id} className="hover:bg-slate-50">
                                        <td className="px-6 py-4 text-sm text-slate-600 whitespace-nowrap">
                                            {new Date(click.timestamp).toLocaleString()}
                                        </td>
                                        <td className="px-6 py-4 text-sm text-slate-600">
                                            {click.city && click.country 
                                                ? `${click.city}, ${click.country}`
                                                : click.country || 'Unknown'}
                                        </td>
                                        <td className="px-6 py-4 text-sm text-slate-600">
                                            {click.device || 'Unknown'}
                                        </td>
                                        <td className="px-6 py-4 text-sm text-slate-600">
                                            {click.browser ? `${click.browser}${click.os ? ` / ${click.os}` : ''}` : 'Unknown'}
                                        </td>
                                        <td className="px-6 py-4 text-sm text-slate-500 max-w-xs truncate">
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
                    className="text-center py-16 bg-slate-50 rounded-xl border-2 border-dashed border-slate-200"
                >
                    <MousePointer className="h-12 w-12 text-slate-300 mx-auto mb-4" />
                    <h3 className="text-lg font-semibold text-slate-700 mb-1">No clicks yet</h3>
                    <p className="text-slate-500">Share your link to start seeing analytics!</p>
                </motion.div>
            )}
        </div>
    );
}
