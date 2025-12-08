import { useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { motion } from 'framer-motion';
import { 
    ArrowRight, BarChart2, Link2, Lock, Shield, Zap,
    QrCode, Clock, Check, Sparkles
} from 'lucide-react';
import { API_ENDPOINTS, getAuthHeaders } from '../config/api';
import SEO from '../components/SEO';

export default function Home() {
    const [url, setUrl] = useState('');
    const [shortUrl, setShortUrl] = useState('');
    const [loading, setLoading] = useState(false);
    const [copied, setCopied] = useState(false);
    const [error, setError] = useState('');
    const navigate = useNavigate();

    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        setLoading(true);
        setError('');
        setShortUrl('');

        const token = localStorage.getItem('token');
        
        if (!token) {
            // Not logged in - redirect to register
            navigate('/register', { state: { pendingUrl: url } });
            return;
        }

        try {
            const res = await fetch(API_ENDPOINTS.links, {
                method: 'POST',
                headers: getAuthHeaders(),
                body: JSON.stringify({ original_url: url }),
            });

            const data = await res.json();

            if (res.ok) {
                setShortUrl(data.short_url);
            } else {
                setError(data.error || 'Failed to create link');
            }
        } catch (err) {
            setError('Network error. Please try again.');
        } finally {
            setLoading(false);
        }
    };

    const handleCopy = async () => {
        await navigator.clipboard.writeText(shortUrl);
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
    };

    const features = [
        {
            icon: <Zap className="h-6 w-6 text-amber-500" />,
            title: "High Performance",
            desc: "Built with Rust and optional Redis caching for fast redirects."
        },
        {
            icon: <Shield className="h-6 w-6 text-emerald-500" />,
            title: "Privacy First",
            desc: "We don't track your users across the web. Your data is yours."
        },
        {
            icon: <BarChart2 className="h-6 w-6 text-blue-500" />,
            title: "Detailed Analytics",
            desc: "Get insights into who is clicking your links and from where."
        },
        {
            icon: <QrCode className="h-6 w-6 text-violet-500" />,
            title: "QR Codes",
            desc: "Generate QR codes for every link. Perfect for print materials."
        },
        {
            icon: <Lock className="h-6 w-6 text-rose-500" />,
            title: "Password Protection",
            desc: "Secure sensitive links with passwords for added privacy."
        },
        {
            icon: <Clock className="h-6 w-6 text-cyan-500" />,
            title: "Link Expiration",
            desc: "Set expiration dates for time-sensitive content and offers."
        }
    ];

    const stats = [
        { value: "∞", label: "Unlimited Links" },
        { value: "100%", label: "Free Forever" },
        { value: "Open", label: "Source Code" }
    ];

    return (
        <>
        <SEO />
        <main className="space-y-24 pb-24">
            {/* Hero Section */}
            <section className="relative overflow-hidden">
                {/* Background patterns */}
                <div className="absolute inset-0 bg-hero-pattern" />
                
                {/* Floating orbs */}
                <div className="bg-orb bg-orb-primary w-96 h-96 -top-48 -left-48" style={{ animationDelay: '0s' }} />
                <div className="bg-orb bg-orb-emerald w-80 h-80 top-1/2 -right-40" style={{ animationDelay: '2s' }} />
                <div className="bg-orb bg-orb-primary w-64 h-64 -bottom-32 left-1/3" style={{ animationDelay: '4s' }} />
                
                <div className="relative max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-20 sm:py-32">
                    <motion.div
                        initial={{ opacity: 0, y: 20 }}
                        animate={{ opacity: 1, y: 0 }}
                        className="text-center space-y-8"
                    >
                        <div className="inline-flex items-center gap-2 px-4 py-2 rounded-full bg-primary-100 text-primary-700 text-sm font-medium">
                            <span className="relative flex h-2 w-2">
                                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-primary-400 opacity-75"></span>
                                <span className="relative inline-flex rounded-full h-2 w-2 bg-primary-500"></span>
                            </span>
                            100% Free & Open Source
                        </div>

                        <h1 className="text-5xl sm:text-7xl font-extrabold text-slate-900 tracking-tight max-w-4xl mx-auto leading-[1.1]">
                            Shorten links,{' '}
                            <span className="text-transparent bg-clip-text bg-gradient-to-r from-primary-600 to-blue-600">
                                expand your reach.
                            </span>
                        </h1>

                        <p className="text-xl text-slate-600 max-w-2xl mx-auto leading-relaxed">
                            The open-source URL shortener designed for privacy, performance, and control.
                            Track clicks, manage links, and share with confidence.
                        </p>

                        {/* URL Shortener Form */}
                        <div className="max-w-2xl mx-auto mt-10">
                            <form onSubmit={handleSubmit} className="relative group">
                                <div className="absolute -inset-1 bg-gradient-to-r from-primary-600 to-blue-600 rounded-2xl blur opacity-20 group-hover:opacity-30 transition duration-500"></div>
                                <div className="relative bg-white rounded-2xl shadow-xl p-2 border border-slate-100">
                                    <div className="flex flex-col sm:flex-row items-stretch sm:items-center gap-3">
                                        <div className="flex-1 flex items-center gap-3 px-4">
                                            <Link2 className="h-5 w-5 text-slate-400 flex-shrink-0" aria-hidden="true" />
                                            <label htmlFor="url-input" className="sr-only">Enter URL to shorten</label>
                                            <input
                                                id="url-input"
                                                type="url"
                                                placeholder="Paste your long link here..."
                                                className="flex-1 py-3 outline-none text-slate-900 placeholder:text-slate-400 min-w-0 bg-white"
                                                value={url}
                                                onChange={(e) => setUrl(e.target.value)}
                                                required
                                                aria-required="true"
                                                aria-label="URL to shorten"
                                            />
                                        </div>
                                        <button
                                            type="submit"
                                            disabled={loading}
                                            className="bg-primary-600 text-white px-8 py-3.5 rounded-xl font-semibold hover:bg-primary-700 transition-all flex items-center justify-center gap-2 shadow-lg shadow-primary-500/25 hover:shadow-primary-500/40 disabled:opacity-70"
                                            aria-label={loading ? 'Creating short link...' : 'Shorten URL'}
                                        >
                                            {loading ? (
                                                <div className="h-5 w-5 border-2 border-white/30 border-t-white rounded-full animate-spin" role="status" aria-label="Loading" />
                                            ) : (
                                                <>
                                                    Shorten
                                                    <ArrowRight className="h-4 w-4" aria-hidden="true" />
                                                </>
                                            )}
                                        </button>
                                    </div>
                                </div>
                            </form>

                            {/* Success Result */}
                            {shortUrl && (
                                <motion.div
                                    initial={{ opacity: 0, y: 10 }}
                                    animate={{ opacity: 1, y: 0 }}
                                    className="mt-4 bg-emerald-50 border border-emerald-200 rounded-xl p-4"
                                >
                                    <div className="flex flex-col sm:flex-row items-center justify-between gap-4">
                                        <div className="flex items-center gap-2">
                                            <Check className="h-5 w-5 text-emerald-600" aria-hidden="true" />
                                            <span className="font-medium text-emerald-700" role="status">Link created!</span>
                                        </div>
                                        <div className="flex items-center gap-3">
                                            <a 
                                                href={shortUrl} 
                                                target="_blank" 
                                                rel="noreferrer"
                                                className="text-primary-600 font-medium hover:underline"
                                            >
                                                {shortUrl}
                                            </a>
                                            <button
                                                onClick={handleCopy}
                                                className="px-4 py-2 bg-emerald-600 text-white rounded-lg text-sm font-medium hover:bg-emerald-700"
                                            >
                                                {copied ? 'Copied!' : 'Copy'}
                                            </button>
                                        </div>
                                    </div>
                                </motion.div>
                            )}

                            {/* Error */}
                            {error && (
                                <motion.div
                                    initial={{ opacity: 0, y: 10 }}
                                    animate={{ opacity: 1, y: 0 }}
                                    className="mt-4 bg-red-50 border border-red-200 rounded-xl p-4 text-red-700 text-sm"
                                    role="alert"
                                    aria-live="polite"
                                >
                                    {error}
                                </motion.div>
                            )}

                            <p className="mt-4 text-sm text-slate-500">
                                By using our service you accept our{' '}
                                <Link to="/terms" className="underline hover:text-slate-700">Terms of Service</Link>
                                {' '}and{' '}
                                <Link to="/privacy" className="underline hover:text-slate-700">Privacy Policy</Link>.
                            </p>
                        </div>

                        {/* Stats */}
                        <motion.div
                            initial={{ opacity: 0, y: 20 }}
                            animate={{ opacity: 1, y: 0 }}
                            transition={{ delay: 0.3 }}
                            className="flex flex-wrap justify-center gap-8 sm:gap-16 pt-8"
                        >
                            {stats.map((stat, i) => (
                                <div key={i} className="text-center">
                                    <div className="text-3xl font-bold text-slate-900">{stat.value}</div>
                                    <div className="text-sm text-slate-500">{stat.label}</div>
                                </div>
                            ))}
                        </motion.div>
                    </motion.div>
                </div>
            </section>

            {/* Features Grid */}
            <section className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                <motion.div
                    initial={{ opacity: 0, y: 20 }}
                    whileInView={{ opacity: 1, y: 0 }}
                    viewport={{ once: true }}
                    className="text-center mb-16"
                >
                    <h2 className="text-3xl sm:text-4xl font-bold text-slate-900 mb-4">
                        Everything you need to manage links
                    </h2>
                    <p className="text-lg text-slate-600 max-w-2xl mx-auto">
                        Powerful features, simple interface. No premium tiers, no limits.
                    </p>
                </motion.div>

                <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-8">
                    {features.map((feature, i) => (
                        <motion.div 
                            key={i}
                            initial={{ opacity: 0, y: 20 }}
                            whileInView={{ opacity: 1, y: 0 }}
                            viewport={{ once: true }}
                            transition={{ delay: i * 0.1 }}
                            className="bg-white p-8 rounded-2xl border border-slate-100 shadow-sm hover:shadow-lg transition-shadow group"
                        >
                            <div className="h-12 w-12 bg-slate-50 rounded-xl flex items-center justify-center mb-6 group-hover:scale-110 transition-transform">
                                {feature.icon}
                            </div>
                            <h3 className="text-xl font-bold text-slate-900 mb-3">{feature.title}</h3>
                            <p className="text-slate-600 leading-relaxed">
                                {feature.desc}
                            </p>
                        </motion.div>
                    ))}
                </div>

                <motion.div
                    initial={{ opacity: 0, y: 20 }}
                    whileInView={{ opacity: 1, y: 0 }}
                    viewport={{ once: true }}
                    className="text-center mt-12"
                >
                    <Link
                        to="/features"
                        className="inline-flex items-center gap-2 text-primary-600 font-medium hover:text-primary-700"
                    >
                        View all features
                        <ArrowRight className="h-4 w-4" />
                    </Link>
                </motion.div>
            </section>

            {/* How It Works */}
            <section className="bg-slate-50 bg-dots-pattern py-24 relative">
                <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                    <motion.div
                        initial={{ opacity: 0, y: 20 }}
                        whileInView={{ opacity: 1, y: 0 }}
                        viewport={{ once: true }}
                        className="text-center mb-16"
                    >
                        <h2 className="text-3xl sm:text-4xl font-bold text-slate-900 mb-4">
                            Simple as 1-2-3
                        </h2>
                        <p className="text-lg text-slate-600">
                            Get started in seconds, no credit card required.
                        </p>
                    </motion.div>

                    <div className="grid md:grid-cols-3 gap-8">
                        {[
                            { step: "1", title: "Paste your link", desc: "Enter any long URL you want to shorten" },
                            { step: "2", title: "Customize (optional)", desc: "Add a custom alias, password, or expiration" },
                            { step: "3", title: "Share everywhere", desc: "Copy your short link and share it anywhere" }
                        ].map((item, i) => (
                            <motion.div
                                key={i}
                                initial={{ opacity: 0, y: 20 }}
                                whileInView={{ opacity: 1, y: 0 }}
                                viewport={{ once: true }}
                                transition={{ delay: i * 0.15 }}
                                className="text-center"
                            >
                                <div className="h-16 w-16 bg-primary-600 text-white rounded-2xl flex items-center justify-center mx-auto mb-6 text-2xl font-bold">
                                    {item.step}
                                </div>
                                <h3 className="text-xl font-bold text-slate-900 mb-2">{item.title}</h3>
                                <p className="text-slate-600">{item.desc}</p>
                            </motion.div>
                        ))}
                    </div>
                </div>
            </section>

            {/* CTA Section */}
            <section className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                <motion.div
                    initial={{ opacity: 0, y: 20 }}
                    whileInView={{ opacity: 1, y: 0 }}
                    viewport={{ once: true }}
                    className="bg-slate-900 rounded-3xl overflow-hidden relative"
                >
                    <div className="absolute inset-0 bg-gradient-to-br from-primary-900/50 to-slate-900"></div>
                    <div className="absolute inset-0 bg-[url('data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iNjAiIGhlaWdodD0iNjAiIHZpZXdCb3g9IjAgMCA2MCA2MCIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj48ZyBmaWxsPSJub25lIiBmaWxsLXJ1bGU9ImV2ZW5vZGQiPjxnIGZpbGw9IiNmZmYiIGZpbGwtb3BhY2l0eT0iMC4wNSI+PHBhdGggZD0iTTM2IDM0djItSDI0di0yaDEyek0zNiAyNHYySDI0di0yaDEyeiIvPjwvZz48L2c+PC9zdmc+')] opacity-30" />
                    
                    <div className="relative px-6 py-20 sm:px-12 sm:py-28 text-center">
                        <motion.div
                            initial={{ scale: 0.9, opacity: 0 }}
                            whileInView={{ scale: 1, opacity: 1 }}
                            viewport={{ once: true }}
                            className="inline-flex items-center gap-2 px-4 py-2 rounded-full bg-white/10 text-white/90 text-sm font-medium mb-6"
                        >
                            <Sparkles className="h-4 w-4" />
                            Free forever, no limits
                        </motion.div>
                        
                        <h2 className="text-3xl sm:text-5xl font-bold text-white mb-6">
                            Ready to get started?
                        </h2>
                        <p className="text-slate-300 text-lg max-w-2xl mx-auto mb-10">
                            A privacy-focused URL shortener you can trust.
                            Free, open source, and takes seconds to sign up.
                        </p>
                        <div className="flex flex-col sm:flex-row items-center justify-center gap-4">
                            <Link
                                to="/register"
                                className="w-full sm:w-auto bg-white text-slate-900 px-8 py-4 rounded-xl font-bold hover:bg-slate-100 transition-colors"
                            >
                                Create free account
                            </Link>
                            <Link
                                to="/features"
                                className="w-full sm:w-auto bg-slate-800 text-white px-8 py-4 rounded-xl font-bold hover:bg-slate-700 transition-colors border border-slate-700"
                            >
                                Explore features
                            </Link>
                        </div>
                        
                        {/* Legal disclaimer */}
                        <p className="mt-8 text-xs text-slate-400 max-w-xl mx-auto">
                            Disclaimer: opn.onl is not responsible for content accessible through shortened links. 
                            We actively remove malicious links when reported. 
                            <Link to="/terms" className="underline hover:text-white ml-1">Terms apply</Link>.
                        </p>
                    </div>
                </motion.div>
            </section>
        </main>
        </>
    );
}
