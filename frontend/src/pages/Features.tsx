import { Link } from 'react-router-dom';
import { motion } from 'framer-motion';
import {
    Link2, BarChart2, QrCode, Lock, Clock,
    Zap, Shield, Download, Key, Sparkles,
    ArrowRight, Check
} from 'lucide-react';

const features = [
    {
        icon: <Link2 className="h-6 w-6" />,
        title: "Custom Short Links",
        description: "Create memorable, branded short links with custom aliases. Make your links recognizable and trustworthy.",
        color: "text-primary-600",
        bgColor: "bg-primary-100"
    },
    {
        icon: <BarChart2 className="h-6 w-6" />,
        title: "Advanced Analytics",
        description: "Track clicks, geographic data, devices, browsers, and referrers. Understand your audience with detailed insights.",
        color: "text-emerald-600",
        bgColor: "bg-emerald-100"
    },
    {
        icon: <QrCode className="h-6 w-6" />,
        title: "QR Code Generation",
        description: "Automatically generate QR codes for every link. Download and share them anywhere - posters, business cards, products.",
        color: "text-violet-600",
        bgColor: "bg-violet-100"
    },
    {
        icon: <Lock className="h-6 w-6" />,
        title: "Password Protection",
        description: "Secure sensitive links with passwords. Control who can access your content with an extra layer of security.",
        color: "text-amber-600",
        bgColor: "bg-amber-100"
    },
    {
        icon: <Clock className="h-6 w-6" />,
        title: "Link Expiration",
        description: "Set expiration dates on links for time-sensitive content. Perfect for limited-time offers and campaigns.",
        color: "text-rose-600",
        bgColor: "bg-rose-100"
    },
    {
        icon: <Download className="h-6 w-6" />,
        title: "Bulk Operations",
        description: "Create multiple short links at once and export your data to CSV. Manage large campaigns efficiently.",
        color: "text-cyan-600",
        bgColor: "bg-cyan-100"
    },
    {
        icon: <Key className="h-6 w-6" />,
        title: "Passkey Authentication",
        description: "Secure passwordless login using modern WebAuthn passkeys. The most secure way to protect your account.",
        color: "text-indigo-600",
        bgColor: "bg-indigo-100"
    },
    {
        icon: <Zap className="h-6 w-6" />,
        title: "High Performance",
        description: "Built with Rust and optional Redis caching. Optimized for fast, reliable redirects.",
        color: "text-yellow-600",
        bgColor: "bg-yellow-100"
    },
    {
        icon: <Shield className="h-6 w-6" />,
        title: "Privacy First",
        description: "We don't track users across the web. Your data stays yours. No third-party trackers or cookies.",
        color: "text-green-600",
        bgColor: "bg-green-100"
    },
];

const container = {
    hidden: { opacity: 0 },
    show: {
        opacity: 1,
        transition: {
            staggerChildren: 0.1
        }
    }
};

const item = {
    hidden: { opacity: 0, y: 20 },
    show: { opacity: 1, y: 0 }
};

export default function Features() {
    return (
        <div className="pb-24">
            {/* Hero */}
            <section className="relative overflow-hidden bg-gradient-to-b from-slate-900 to-slate-800 text-white py-24">
                <div className="absolute inset-0 bg-[url('data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iNjAiIGhlaWdodD0iNjAiIHZpZXdCb3g9IjAgMCA2MCA2MCIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj48ZyBmaWxsPSJub25lIiBmaWxsLXJ1bGU9ImV2ZW5vZGQiPjxnIGZpbGw9IiNmZmYiIGZpbGwtb3BhY2l0eT0iMC4wNSI+PHBhdGggZD0iTTM2IDM0djItSDI0di0yaDEyek0zNiAyNHYySDI0di0yaDEyeiIvPjwvZz48L2c+PC9zdmc+')] opacity-40" />
                <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 relative">
                    <motion.div 
                        initial={{ opacity: 0, y: 20 }}
                        animate={{ opacity: 1, y: 0 }}
                        className="text-center"
                    >
                        <div className="inline-flex items-center gap-2 px-4 py-2 rounded-full bg-white/10 text-white/90 text-sm font-medium mb-6">
                            <Sparkles className="h-4 w-4" />
                            All features included free
                        </div>
                        <h1 className="text-4xl sm:text-6xl font-extrabold tracking-tight mb-6">
                            Everything you need to<br />
                            <span className="text-transparent bg-clip-text bg-gradient-to-r from-primary-400 to-cyan-400">
                                manage your links
                            </span>
                        </h1>
                        <p className="text-xl text-slate-300 max-w-2xl mx-auto mb-10">
                            Powerful features, simple interface. Get detailed analytics, customize your links, 
                            and maintain complete control over your data.
                        </p>
                        <Link
                            to="/register"
                            className="inline-flex items-center gap-2 bg-white text-slate-900 px-8 py-4 rounded-xl font-bold hover:bg-slate-100 transition-colors"
                        >
                            Get started for free
                            <ArrowRight className="h-4 w-4" />
                        </Link>
                    </motion.div>
                </div>
            </section>

            {/* Features Grid */}
            <section className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-24">
                <motion.div
                    variants={container}
                    initial="hidden"
                    whileInView="show"
                    viewport={{ once: true }}
                    className="grid md:grid-cols-2 lg:grid-cols-3 gap-8"
                >
                    {features.map((feature, index) => (
                        <motion.div
                            key={index}
                            variants={item}
                            className="bg-white p-8 rounded-2xl border border-slate-100 shadow-sm hover:shadow-lg transition-shadow group"
                        >
                            <div className={`h-14 w-14 ${feature.bgColor} rounded-xl flex items-center justify-center mb-6 group-hover:scale-110 transition-transform`}>
                                <span className={feature.color}>{feature.icon}</span>
                            </div>
                            <h3 className="text-xl font-bold text-slate-900 mb-3">{feature.title}</h3>
                            <p className="text-slate-600 leading-relaxed">{feature.description}</p>
                        </motion.div>
                    ))}
                </motion.div>
            </section>

            {/* Comparison */}
            <section className="bg-slate-50 py-24">
                <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                    <motion.div
                        initial={{ opacity: 0, y: 20 }}
                        whileInView={{ opacity: 1, y: 0 }}
                        viewport={{ once: true }}
                        className="text-center mb-16"
                    >
                        <h2 className="text-3xl sm:text-4xl font-bold text-slate-900 mb-4">
                            Why choose opn.onl?
                        </h2>
                        <p className="text-lg text-slate-600 max-w-2xl mx-auto">
                            Unlike other URL shorteners, we prioritize your privacy and give you full control.
                        </p>
                    </motion.div>

                    <div className="grid md:grid-cols-2 gap-8 max-w-4xl mx-auto">
                        <motion.div
                            initial={{ opacity: 0, x: -20 }}
                            whileInView={{ opacity: 1, x: 0 }}
                            viewport={{ once: true }}
                            className="bg-white p-8 rounded-2xl shadow-sm border border-slate-200"
                        >
                            <h3 className="text-lg font-bold text-slate-900 mb-6">Other URL Shorteners</h3>
                            <ul className="space-y-4">
                                {[
                                    'Track users across websites',
                                    'Sell your data to advertisers',
                                    'Limited free tier features',
                                    'Closed source, no transparency',
                                    'Expensive enterprise plans'
                                ].map((item, i) => (
                                    <li key={i} className="flex items-start gap-3 text-slate-600">
                                        <X className="h-5 w-5 text-red-500 flex-shrink-0 mt-0.5" />
                                        {item}
                                    </li>
                                ))}
                            </ul>
                        </motion.div>

                        <motion.div
                            initial={{ opacity: 0, x: 20 }}
                            whileInView={{ opacity: 1, x: 0 }}
                            viewport={{ once: true }}
                            className="bg-gradient-to-br from-primary-600 to-primary-700 p-8 rounded-2xl shadow-lg text-white"
                        >
                            <h3 className="text-lg font-bold mb-6">opn.onl</h3>
                            <ul className="space-y-4">
                                {[
                                    'Privacy-first, no tracking scripts',
                                    'Your data stays yours forever',
                                    'All features free, no limits',
                                    'Open source and transparent',
                                    'Self-host option available'
                                ].map((item, i) => (
                                    <li key={i} className="flex items-start gap-3">
                                        <Check className="h-5 w-5 text-primary-200 flex-shrink-0 mt-0.5" />
                                        {item}
                                    </li>
                                ))}
                            </ul>
                        </motion.div>
                    </div>
                </div>
            </section>

            {/* CTA */}
            <section className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-24">
                <motion.div
                    initial={{ opacity: 0, y: 20 }}
                    whileInView={{ opacity: 1, y: 0 }}
                    viewport={{ once: true }}
                    className="bg-slate-900 rounded-3xl p-8 sm:p-16 text-center relative overflow-hidden"
                >
                    <div className="absolute inset-0 bg-gradient-to-br from-primary-900/30 to-transparent" />
                    <div className="relative">
                        <h2 className="text-3xl sm:text-4xl font-bold text-white mb-4">
                            Ready to get started?
                        </h2>
                        <p className="text-slate-300 text-lg max-w-2xl mx-auto mb-8">
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
                            <a
                                href="https://github.com/ysalitrynskyi/opn.onl"
                                target="_blank"
                                rel="noreferrer"
                                className="w-full sm:w-auto border border-slate-600 text-white px-8 py-4 rounded-xl font-bold hover:bg-slate-800 transition-colors"
                            >
                                View on GitHub
                            </a>
                        </div>
                    </div>
                </motion.div>
            </section>
        </div>
    );
}

function X({ className }: { className?: string }) {
    return (
        <svg className={className} viewBox="0 0 20 20" fill="currentColor">
            <path fillRule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clipRule="evenodd" />
        </svg>
    );
}

