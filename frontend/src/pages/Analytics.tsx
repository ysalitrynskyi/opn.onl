import { useEffect, useState } from 'react';
import { useParams, useNavigate, Link } from 'react-router-dom';
import { ArrowLeft, Globe, Monitor, Smartphone, Clock, MousePointer, TrendingUp } from 'lucide-react';
import { motion } from 'framer-motion';
import { AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, PieChart, Pie, Cell } from 'recharts';
import { API_ENDPOINTS, getAuthHeaders } from '../config/api';

interface ClickEvent {
    id: number;
    created_at: string;
    ip_address: string | null;
    user_agent: string | null;
    referer: string | null;
    country: string | null;
}

interface LinkStats {
    total_clicks: number;
    events: ClickEvent[];
}

interface ChartData {
    date: string;
    clicks: number;
}

interface DeviceData {
    name: string;
    value: number;
    [key: string]: string | number;
}

const COLORS = ['#3b82f6', '#10b981', '#f59e0b', '#ef4444', '#8b5cf6'];

function parseUserAgent(ua: string | null): { device: string; browser: string } {
    if (!ua) return { device: 'Unknown', browser: 'Unknown' };
    
    let device = 'Desktop';
    if (/mobile/i.test(ua)) device = 'Mobile';
    else if (/tablet|ipad/i.test(ua)) device = 'Tablet';
    
    let browser = 'Other';
    if (/chrome/i.test(ua) && !/edge/i.test(ua)) browser = 'Chrome';
    else if (/firefox/i.test(ua)) browser = 'Firefox';
    else if (/safari/i.test(ua) && !/chrome/i.test(ua)) browser = 'Safari';
    else if (/edge/i.test(ua)) browser = 'Edge';
    
    return { device, browser };
}

function Skeleton() {
    return (
        <div className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8 py-8 animate-pulse">
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

export default function Analytics() {
    const { id } = useParams<{ id: string }>();
    const navigate = useNavigate();
    const [stats, setStats] = useState<LinkStats | null>(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState('');

    useEffect(() => {
        const token = localStorage.getItem('token');
        if (!token) {
            navigate('/login');
            return;
        }
        fetchStats();
    }, [id, navigate]);

    const fetchStats = async () => {
        try {
            const res = await fetch(API_ENDPOINTS.linkStats(Number(id)), {
                headers: getAuthHeaders()
            });
            
            if (res.ok) {
                const data = await res.json();
                setStats(data);
            } else if (res.status === 401) {
                localStorage.removeItem('token');
                navigate('/login');
            } else if (res.status === 403) {
                setError('You do not have permission to view this link\'s analytics.');
            } else if (res.status === 404) {
                setError('Link not found.');
            } else {
                setError('Failed to load analytics.');
            }
        } catch (error) {
            console.error('Failed to fetch stats', error);
            setError('Network error. Please try again.');
        } finally {
            setLoading(false);
        }
    };

    if (loading) {
        return <Skeleton />;
    }

    if (error) {
        return (
            <div className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
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

    // Process data for charts
    const clicksByDate: Record<string, number> = {};
    const deviceCounts: Record<string, number> = { Desktop: 0, Mobile: 0, Tablet: 0 };
    const browserCounts: Record<string, number> = {};
    const refererCounts: Record<string, number> = {};

    stats.events.forEach(event => {
        // Group by date
        const date = new Date(event.created_at).toLocaleDateString();
        clicksByDate[date] = (clicksByDate[date] || 0) + 1;

        // Parse user agent
        const { device, browser } = parseUserAgent(event.user_agent);
        deviceCounts[device] = (deviceCounts[device] || 0) + 1;
        browserCounts[browser] = (browserCounts[browser] || 0) + 1;

        // Track referers
        if (event.referer) {
            try {
                const url = new URL(event.referer);
                refererCounts[url.hostname] = (refererCounts[url.hostname] || 0) + 1;
            } catch {
                refererCounts['Direct'] = (refererCounts['Direct'] || 0) + 1;
            }
        } else {
            refererCounts['Direct'] = (refererCounts['Direct'] || 0) + 1;
        }
    });

    // Convert to chart data
    const chartData: ChartData[] = Object.entries(clicksByDate)
        .map(([date, clicks]) => ({ date, clicks }))
        .sort((a, b) => new Date(a.date).getTime() - new Date(b.date).getTime())
        .slice(-30); // Last 30 days

    const deviceData: DeviceData[] = Object.entries(deviceCounts)
        .filter(([_, value]) => value > 0)
        .map(([name, value]) => ({ name, value }));

    const browserData: DeviceData[] = Object.entries(browserCounts)
        .filter(([_, value]) => value > 0)
        .map(([name, value]) => ({ name, value }))
        .sort((a, b) => b.value - a.value)
        .slice(0, 5);

    const topReferers = Object.entries(refererCounts)
        .sort((a, b) => b[1] - a[1])
        .slice(0, 5);

    const todayClicks = stats.events.filter(e => 
        new Date(e.created_at).toDateString() === new Date().toDateString()
    ).length;

    const weekClicks = stats.events.filter(e => {
        const eventDate = new Date(e.created_at);
        const weekAgo = new Date();
        weekAgo.setDate(weekAgo.getDate() - 7);
        return eventDate >= weekAgo;
    }).length;

    return (
        <div className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
            <Link to="/dashboard" className="inline-flex items-center gap-2 text-slate-600 hover:text-slate-900 mb-8">
                <ArrowLeft className="h-4 w-4" />
                Back to Dashboard
            </Link>

            <motion.div
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
            >
                <h1 className="text-3xl font-bold text-slate-900 mb-2">Link Analytics</h1>
                <p className="text-slate-500 mb-8">Track performance and understand your audience</p>

                {/* Stats Cards */}
                <div className="grid grid-cols-1 sm:grid-cols-3 gap-6 mb-8">
                    <motion.div 
                        initial={{ opacity: 0, y: 20 }}
                        animate={{ opacity: 1, y: 0 }}
                        transition={{ delay: 0.1 }}
                        className="bg-white p-6 rounded-xl shadow-sm border border-slate-200"
                    >
                        <div className="flex items-center gap-3 mb-2">
                            <div className="h-10 w-10 bg-primary-100 rounded-lg flex items-center justify-center">
                                <MousePointer className="h-5 w-5 text-primary-600" />
                            </div>
                            <span className="text-sm font-medium text-slate-500">Total Clicks</span>
                        </div>
                        <p className="text-3xl font-bold text-slate-900">{stats.total_clicks.toLocaleString()}</p>
                    </motion.div>

                    <motion.div 
                        initial={{ opacity: 0, y: 20 }}
                        animate={{ opacity: 1, y: 0 }}
                        transition={{ delay: 0.2 }}
                        className="bg-white p-6 rounded-xl shadow-sm border border-slate-200"
                    >
                        <div className="flex items-center gap-3 mb-2">
                            <div className="h-10 w-10 bg-emerald-100 rounded-lg flex items-center justify-center">
                                <TrendingUp className="h-5 w-5 text-emerald-600" />
                            </div>
                            <span className="text-sm font-medium text-slate-500">Today</span>
                        </div>
                        <p className="text-3xl font-bold text-slate-900">{todayClicks.toLocaleString()}</p>
                    </motion.div>

                    <motion.div 
                        initial={{ opacity: 0, y: 20 }}
                        animate={{ opacity: 1, y: 0 }}
                        transition={{ delay: 0.3 }}
                        className="bg-white p-6 rounded-xl shadow-sm border border-slate-200"
                    >
                        <div className="flex items-center gap-3 mb-2">
                            <div className="h-10 w-10 bg-amber-100 rounded-lg flex items-center justify-center">
                                <Clock className="h-5 w-5 text-amber-600" />
                            </div>
                            <span className="text-sm font-medium text-slate-500">Last 7 Days</span>
                        </div>
                        <p className="text-3xl font-bold text-slate-900">{weekClicks.toLocaleString()}</p>
                    </motion.div>
                </div>

                {/* Click Trend Chart */}
                {chartData.length > 0 && (
                    <motion.div 
                        initial={{ opacity: 0, y: 20 }}
                        animate={{ opacity: 1, y: 0 }}
                        transition={{ delay: 0.4 }}
                        className="bg-white p-6 rounded-xl shadow-sm border border-slate-200 mb-8"
                    >
                        <h2 className="text-lg font-semibold text-slate-900 mb-6">Click Trend</h2>
                        <div className="h-72">
                            <ResponsiveContainer width="100%" height="100%">
                                <AreaChart data={chartData}>
                                    <defs>
                                        <linearGradient id="colorClicks" x1="0" y1="0" x2="0" y2="1">
                                            <stop offset="5%" stopColor="#3b82f6" stopOpacity={0.3}/>
                                            <stop offset="95%" stopColor="#3b82f6" stopOpacity={0}/>
                                        </linearGradient>
                                    </defs>
                                    <CartesianGrid strokeDasharray="3 3" stroke="#e2e8f0" />
                                    <XAxis 
                                        dataKey="date" 
                                        stroke="#94a3b8"
                                        fontSize={12}
                                        tickLine={false}
                                    />
                                    <YAxis 
                                        stroke="#94a3b8"
                                        fontSize={12}
                                        tickLine={false}
                                        axisLine={false}
                                    />
                                    <Tooltip 
                                        contentStyle={{
                                            backgroundColor: '#fff',
                                            border: '1px solid #e2e8f0',
                                            borderRadius: '8px',
                                            boxShadow: '0 4px 6px -1px rgb(0 0 0 / 0.1)'
                                        }}
                                    />
                                    <Area 
                                        type="monotone" 
                                        dataKey="clicks" 
                                        stroke="#3b82f6" 
                                        fillOpacity={1} 
                                        fill="url(#colorClicks)" 
                                        strokeWidth={2}
                                    />
                                </AreaChart>
                            </ResponsiveContainer>
                        </div>
                    </motion.div>
                )}

                {/* Device & Browser Stats */}
                <div className="grid grid-cols-1 md:grid-cols-2 gap-8 mb-8">
                    {deviceData.length > 0 && (
                        <motion.div 
                            initial={{ opacity: 0, y: 20 }}
                            animate={{ opacity: 1, y: 0 }}
                            transition={{ delay: 0.5 }}
                            className="bg-white p-6 rounded-xl shadow-sm border border-slate-200"
                        >
                            <h2 className="text-lg font-semibold text-slate-900 mb-6">Devices</h2>
                            <div className="flex items-center justify-center">
                                <PieChart width={200} height={200}>
                                    <Pie
                                        data={deviceData}
                                        cx={100}
                                        cy={100}
                                        innerRadius={60}
                                        outerRadius={80}
                                        paddingAngle={5}
                                        dataKey="value"
                                    >
                                        {deviceData.map((_, index) => (
                                            <Cell key={`cell-${index}`} fill={COLORS[index % COLORS.length]} />
                                        ))}
                                    </Pie>
                                    <Tooltip />
                                </PieChart>
                            </div>
                            <div className="flex justify-center gap-6 mt-4">
                                {deviceData.map((entry, index) => (
                                    <div key={entry.name} className="flex items-center gap-2">
                                        <div 
                                            className="h-3 w-3 rounded-full" 
                                            style={{ backgroundColor: COLORS[index % COLORS.length] }}
                                        />
                                        <span className="text-sm text-slate-600">
                                            {entry.name} ({entry.value})
                                        </span>
                                    </div>
                                ))}
                            </div>
                        </motion.div>
                    )}

                    {browserData.length > 0 && (
                        <motion.div 
                            initial={{ opacity: 0, y: 20 }}
                            animate={{ opacity: 1, y: 0 }}
                            transition={{ delay: 0.6 }}
                            className="bg-white p-6 rounded-xl shadow-sm border border-slate-200"
                        >
                            <h2 className="text-lg font-semibold text-slate-900 mb-6">Browsers</h2>
                            <div className="space-y-4">
                                {browserData.map((browser, index) => {
                                    const percentage = Math.round((browser.value / stats.total_clicks) * 100);
                                    return (
                                        <div key={browser.name}>
                                            <div className="flex items-center justify-between mb-1">
                                                <span className="text-sm font-medium text-slate-700">{browser.name}</span>
                                                <span className="text-sm text-slate-500">{browser.value} ({percentage}%)</span>
                                            </div>
                                            <div className="h-2 bg-slate-100 rounded-full overflow-hidden">
                                                <motion.div 
                                                    initial={{ width: 0 }}
                                                    animate={{ width: `${percentage}%` }}
                                                    transition={{ delay: 0.7 + index * 0.1, duration: 0.5 }}
                                                    className="h-full rounded-full"
                                                    style={{ backgroundColor: COLORS[index % COLORS.length] }}
                                                />
                                            </div>
                                        </div>
                                    );
                                })}
                            </div>
                        </motion.div>
                    )}
                </div>

                {/* Top Referers */}
                {topReferers.length > 0 && (
                    <motion.div 
                        initial={{ opacity: 0, y: 20 }}
                        animate={{ opacity: 1, y: 0 }}
                        transition={{ delay: 0.7 }}
                        className="bg-white p-6 rounded-xl shadow-sm border border-slate-200 mb-8"
                    >
                        <h2 className="text-lg font-semibold text-slate-900 mb-6">Top Referrers</h2>
                        <div className="space-y-3">
                            {topReferers.map(([referer, count]) => (
                                <div key={referer} className="flex items-center justify-between py-2 border-b border-slate-100 last:border-0">
                                    <div className="flex items-center gap-3">
                                        <div className="h-8 w-8 bg-slate-100 rounded-lg flex items-center justify-center">
                                            <Globe className="h-4 w-4 text-slate-500" />
                                        </div>
                                        <span className="text-slate-700">{referer}</span>
                                    </div>
                                    <span className="text-sm font-medium text-slate-500">{count} clicks</span>
                                </div>
                            ))}
                        </div>
                    </motion.div>
                )}

                {/* Recent Clicks */}
                <motion.div 
                    initial={{ opacity: 0, y: 20 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: 0.8 }}
                    className="bg-white p-6 rounded-xl shadow-sm border border-slate-200"
                >
                    <h2 className="text-lg font-semibold text-slate-900 mb-6">Recent Clicks</h2>
                    {stats.events.length === 0 ? (
                        <p className="text-center text-slate-500 py-8">No clicks recorded yet.</p>
                    ) : (
                        <div className="overflow-x-auto">
                            <table className="w-full">
                                <thead>
                                    <tr className="text-left text-sm text-slate-500 border-b border-slate-200">
                                        <th className="pb-3 font-medium">Time</th>
                                        <th className="pb-3 font-medium">Device</th>
                                        <th className="pb-3 font-medium">Location</th>
                                        <th className="pb-3 font-medium">Referrer</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {stats.events.slice(0, 20).map((event) => {
                                        const { device } = parseUserAgent(event.user_agent);
                                        return (
                                            <tr key={event.id} className="border-b border-slate-100 last:border-0">
                                                <td className="py-3 text-sm text-slate-700">
                                                    {new Date(event.created_at).toLocaleString()}
                                                </td>
                                                <td className="py-3 text-sm text-slate-700">
                                                    <span className="inline-flex items-center gap-1">
                                                        {device === 'Mobile' ? (
                                                            <Smartphone className="h-4 w-4 text-slate-400" />
                                                        ) : (
                                                            <Monitor className="h-4 w-4 text-slate-400" />
                                                        )}
                                                        {device}
                                                    </span>
                                                </td>
                                                <td className="py-3 text-sm text-slate-700">
                                                    {event.country || event.ip_address || 'Unknown'}
                                                </td>
                                                <td className="py-3 text-sm text-slate-500 truncate max-w-xs">
                                                    {event.referer || 'Direct'}
                                                </td>
                                            </tr>
                                        );
                                    })}
                                </tbody>
                            </table>
                        </div>
                    )}
                </motion.div>
            </motion.div>
        </div>
    );
}

