import { useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { motion } from 'framer-motion';
import {
    ArrowRight, BarChart2, Lock, Shield, Zap, QrCode, Clock, Check, Copy, CornerDownRight
} from 'lucide-react';
import { API_ENDPOINTS, authFetch } from '../config/api';
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
            navigate('/register', { state: { pendingUrl: url } });
            return;
        }

        try {
            const res = await authFetch(API_ENDPOINTS.links, {
                method: 'POST',
                body: JSON.stringify({ original_url: url }),
            });
            const data = await res.json();
            if (res.ok) {
                setShortUrl(data.short_url);
            } else {
                setError(data.error || 'Failed to create link');
            }
        } catch {
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
        { icon: Zap, title: 'Rust-fast redirects', desc: 'An Axum + Redis core resolves links in microseconds, built to index billions of rows.' },
        { icon: Shield, title: 'Privacy by default', desc: 'No cross-site tracking, no third-party pixels. The data your links generate stays yours.' },
        { icon: BarChart2, title: 'Honest analytics', desc: 'Clicks, geography, devices and referrers — first-party, in real time, no sampling.' },
        { icon: QrCode, title: 'Branded QR codes', desc: 'Every link ships with a QR — add your brand colour and logo, export PNG or SVG.' },
        { icon: Lock, title: 'Password & limits', desc: 'Gate sensitive links behind a password, cap total clicks, or schedule a window.' },
        { icon: Clock, title: 'Expiring links', desc: 'Set a start and an end. Campaigns turn themselves off when they should.' },
    ];

    const steps = [
        { n: '01', title: 'Paste a long URL', desc: 'Drop in any link — query strings, UTM tags and all.' },
        { n: '02', title: 'Shape it', desc: 'Add a custom alias, a password, an expiry or a click cap. Optional, always.' },
        { n: '03', title: 'Share & measure', desc: 'Copy the short link, generate a QR, and watch the clicks land live.' },
    ];

    const ease = [0.16, 1, 0.3, 1] as const;

    return (
        <>
            <SEO />
            <main>
                {/* ===== Hero ===== */}
                <section className="relative border-b border-line overflow-hidden">
                    <div className="absolute inset-0 bg-grid-pattern [mask-image:radial-gradient(120%_90%_at_50%_0%,black,transparent)]" />
                    <img src="/bg-network.png" alt="" aria-hidden="true" className="pointer-events-none absolute -top-12 right-0 hidden lg:block w-2/3 max-w-3xl opacity-60 [mask-image:linear-gradient(to_left,black,transparent_80%)]" />
                    <div className="relative max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 pt-20 pb-24 lg:pt-28 lg:pb-32">
                        <div className="grid lg:grid-cols-12 gap-12 lg:gap-8 items-center">
                            {/* Left: copy + instrument */}
                            <div className="lg:col-span-7">
                                <motion.p
                                    initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.5, ease }}
                                    className="font-mono text-xs uppercase tracking-[0.2em] text-primary-600"
                                >
                                    Open source · self-hostable · AGPL-3.0
                                </motion.p>

                                <motion.h1
                                    initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.6, ease, delay: 0.05 }}
                                    className="mt-5 font-display font-extrabold text-ink tracking-tightest leading-[0.98] text-[clamp(2.6rem,6vw,4.75rem)]"
                                >
                                    Short links that
                                    <br />
                                    answer to <span className="text-primary-600">you.</span>
                                </motion.h1>

                                <motion.p
                                    initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.6, ease, delay: 0.12 }}
                                    className="mt-6 text-lg sm:text-xl text-muted leading-relaxed max-w-[52ch]"
                                >
                                    A privacy-first URL shortener you actually own. Shorten, protect and
                                    measure every link — on your own server, with no one watching over your shoulder.
                                </motion.p>

                                {/* Shortener instrument */}
                                <motion.div
                                    initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.6, ease, delay: 0.18 }}
                                    className="mt-9 max-w-xl"
                                >
                                    <form onSubmit={handleSubmit}>
                                        <div className="flex flex-col sm:flex-row items-stretch gap-2 rounded-2xl border border-line2 bg-surface p-2 shadow-card focus-within:border-primary-500 transition-colors">
                                            <div className="flex flex-1 items-center gap-3 px-3 min-w-0">
                                                <CornerDownRight className="h-4 w-4 text-faint shrink-0" aria-hidden="true" />
                                                <label htmlFor="url-input" className="sr-only">Enter URL to shorten</label>
                                                <input
                                                    id="url-input"
                                                    type="url"
                                                    placeholder="https://your-very-long-link.com/…"
                                                    className="flex-1 min-w-0 bg-transparent py-2.5 font-mono text-sm text-ink outline-none placeholder:text-faint"
                                                    value={url}
                                                    onChange={(e) => setUrl(e.target.value)}
                                                    required
                                                    aria-label="URL to shorten"
                                                />
                                            </div>
                                            <button
                                                type="submit"
                                                disabled={loading}
                                                className="inline-flex items-center justify-center gap-2 rounded-xl bg-primary-600 px-6 py-3 font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-70"
                                            >
                                                {loading ? (
                                                    <span className="h-5 w-5 rounded-full border-2 border-white/30 border-t-white animate-spin" role="status" aria-label="Creating link" />
                                                ) : (
                                                    <>Shorten <ArrowRight className="h-4 w-4" aria-hidden="true" /></>
                                                )}
                                            </button>
                                        </div>
                                    </form>

                                    {shortUrl && (
                                        <motion.div
                                            initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }}
                                            className="mt-3 flex items-center justify-between gap-4 rounded-xl border border-success/30 bg-success/5 px-4 py-3"
                                        >
                                            <div className="flex items-center gap-2 min-w-0">
                                                <Check className="h-4 w-4 text-success shrink-0" aria-hidden="true" />
                                                <a href={shortUrl} target="_blank" rel="noreferrer" className="truncate font-mono text-sm text-ink hover:text-primary-600">{shortUrl}</a>
                                            </div>
                                            <button onClick={handleCopy} className="inline-flex items-center gap-1.5 rounded-lg bg-ink px-3 py-1.5 text-xs font-semibold text-white hover:opacity-90">
                                                <Copy className="h-3.5 w-3.5" /> {copied ? 'Copied' : 'Copy'}
                                            </button>
                                        </motion.div>
                                    )}
                                    {error && (
                                        <motion.div initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} role="alert"
                                            className="mt-3 rounded-xl border border-danger/30 bg-danger/5 px-4 py-3 text-sm text-danger">
                                            {error}
                                        </motion.div>
                                    )}

                                    <p className="mt-4 text-sm text-faint">
                                        By shortening a link you accept our{' '}
                                        <Link to="/terms" className="text-muted underline decoration-line2 underline-offset-2 hover:text-ink">Terms</Link>{' '}and{' '}
                                        <Link to="/privacy" className="text-muted underline decoration-line2 underline-offset-2 hover:text-ink">Privacy Policy</Link>.
                                    </p>
                                </motion.div>
                            </div>

                            {/* Right: product peek */}
                            <motion.div
                                initial={{ opacity: 0, y: 24 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.7, ease, delay: 0.25 }}
                                className="lg:col-span-5"
                            >
                                <div className="rounded-2xl border border-line2 bg-surface shadow-lift overflow-hidden">
                                    <div className="flex items-center gap-1.5 border-b border-line px-4 py-3">
                                        <span className="h-2.5 w-2.5 rounded-full bg-line2" />
                                        <span className="h-2.5 w-2.5 rounded-full bg-line2" />
                                        <span className="h-2.5 w-2.5 rounded-full bg-line2" />
                                        <span className="ml-2 font-mono text-xs text-faint">dashboard</span>
                                    </div>
                                    <div className="p-5 space-y-4">
                                        <div className="rounded-xl border border-line p-4">
                                            <p className="truncate font-mono text-xs text-faint">https://example.com/2026/spring-campaign?utm_source=…</p>
                                            <div className="mt-2 flex items-center gap-2">
                                                <span className="font-mono text-base font-semibold text-ink">opn.onl/</span>
                                                <span className="font-mono text-base font-semibold text-primary-600">spring</span>
                                            </div>
                                            <div className="mt-4 grid grid-cols-3 gap-3 border-t border-line pt-4">
                                                {[['1,284', 'clicks'], ['37', 'today'], ['18', 'countries']].map(([v, l]) => (
                                                    <div key={l}>
                                                        <div className="font-mono text-lg font-bold text-ink tabular-nums">{v}</div>
                                                        <div className="text-xs text-faint">{l}</div>
                                                    </div>
                                                ))}
                                            </div>
                                        </div>
                                        <div className="flex items-end justify-between gap-1.5 h-16 px-1" aria-hidden="true">
                                            {[28, 44, 36, 62, 50, 78, 58, 92, 70, 100, 84, 66].map((h, i) => (
                                                <div key={i} className="flex-1 rounded-t bg-primary-600/15" style={{ height: `${h}%` }}>
                                                    <div className="w-full rounded-t bg-primary-600" style={{ height: i === 9 ? '100%' : '0' }} />
                                                </div>
                                            ))}
                                        </div>
                                    </div>
                                </div>
                            </motion.div>
                        </div>
                    </div>
                </section>

                {/* ===== Trust strip ===== */}
                <section className="border-b border-line bg-surface">
                    <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-5">
                        <div className="flex flex-wrap items-center justify-center gap-x-8 gap-y-2 font-mono text-xs uppercase tracking-[0.18em] text-faint">
                            <span>Built with Rust</span><span className="text-line2">/</span>
                            <span>React 19</span><span className="text-line2">/</span>
                            <span>PostgreSQL</span><span className="text-line2">/</span>
                            <span>Self-hosted</span><span className="text-line2">/</span>
                            <span>No tracking</span>
                        </div>
                    </div>
                </section>

                {/* ===== Features ===== */}
                <section className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-20 lg:py-28">
                    <div className="max-w-2xl">
                        <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary-600">Everything, no upsell</p>
                        <h2 className="mt-4 font-display text-3xl sm:text-4xl font-extrabold text-ink tracking-tight">
                            The whole toolkit, free forever.
                        </h2>
                        <p className="mt-4 text-lg text-muted max-w-[58ch]">
                            No "pro" tier hiding the useful features. Everything below ships in the open-source core.
                        </p>
                    </div>

                    <div className="mt-12 grid sm:grid-cols-2 lg:grid-cols-3 border-t border-l border-line">
                        {features.map(({ icon: Icon, title, desc }) => (
                            <div key={title} className="group border-b border-r border-line p-7 transition-colors hover:bg-primary-50/40">
                                <Icon className="h-5 w-5 text-muted transition-colors group-hover:text-primary-600" strokeWidth={1.75} aria-hidden="true" />
                                <h3 className="mt-5 font-display text-lg font-bold text-ink">{title}</h3>
                                <p className="mt-2 text-[15px] leading-relaxed text-muted">{desc}</p>
                            </div>
                        ))}
                    </div>

                    <div className="mt-8">
                        <Link to="/features" className="inline-flex items-center gap-1.5 font-medium text-primary-600 hover:text-primary-700">
                            See every feature <ArrowRight className="h-4 w-4" />
                        </Link>
                    </div>
                </section>

                {/* ===== Self-host callout ===== */}
                <section className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 pb-20 lg:pb-28">
                    <div className="relative rounded-4xl bg-ink text-white overflow-hidden">
                        <img src="/bg-network.png" alt="" aria-hidden="true" className="pointer-events-none absolute inset-0 h-full w-full object-cover opacity-20 [mask-image:radial-gradient(120%_120%_at_100%_0%,black,transparent_75%)]" />
                        <div className="relative grid lg:grid-cols-2">
                            <div className="p-8 sm:p-12 lg:p-14">
                                <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary-300">Your server, your rules</p>
                                <h2 className="mt-4 font-display text-3xl sm:text-4xl font-extrabold tracking-tight text-white">
                                    Up and running in one command.
                                </h2>
                                <p className="mt-4 text-white/70 leading-relaxed max-w-[48ch]">
                                    Bring your own domain and host the whole stack — backend, frontend, database and
                                    cache — behind a single Docker Compose file. No vendor, no lock-in.
                                </p>
                                <div className="mt-7 flex flex-wrap gap-3">
                                    <a href="https://github.com/ysalitrynskyi/opn.onl" target="_blank" rel="noreferrer"
                                        className="inline-flex items-center gap-2 rounded-xl bg-white px-5 py-3 font-semibold text-ink hover:bg-white/90">
                                        Read the docs <ArrowRight className="h-4 w-4" />
                                    </a>
                                    <Link to="/register" className="inline-flex items-center gap-2 rounded-xl border border-white/20 px-5 py-3 font-semibold text-white hover:bg-white/10">
                                        Try the hosted app
                                    </Link>
                                </div>
                            </div>
                            <div className="border-t lg:border-t-0 lg:border-l border-white/10 p-8 sm:p-12 lg:p-14 flex items-center">
                                <pre className="w-full font-mono text-sm leading-7 text-white/90 overflow-x-auto">
<span className="text-white/40"># clone & launch</span>{'\n'}
<span className="text-primary-300">git</span> clone https://github.com/ysalitrynskyi/opn.onl{'\n'}
<span className="text-primary-300">cd</span> opn.onl{'\n'}
<span className="text-primary-300">docker</span> compose up -d{'\n\n'}
<span className="text-success">✓</span> <span className="text-white/50">api, web, postgres, redis — live</span>
                                </pre>
                            </div>
                        </div>
                    </div>
                </section>

                {/* ===== How it works ===== */}
                <section className="border-t border-line bg-surface">
                    <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-20 lg:py-28">
                        <h2 className="font-display text-3xl sm:text-4xl font-extrabold text-ink tracking-tight max-w-xl">
                            From long to shareable in three steps.
                        </h2>
                        <div className="mt-14 grid md:grid-cols-3 gap-px bg-line border border-line rounded-2xl overflow-hidden">
                            {steps.map((s) => (
                                <div key={s.n} className="bg-surface p-8">
                                    <div className="font-mono text-sm font-semibold text-primary-600">{s.n}</div>
                                    <h3 className="mt-4 font-display text-xl font-bold text-ink">{s.title}</h3>
                                    <p className="mt-2 text-muted leading-relaxed">{s.desc}</p>
                                </div>
                            ))}
                        </div>
                    </div>
                </section>

                {/* ===== Developers / API + MCP ===== */}
                <section className="border-t border-line">
                    <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-20 lg:py-28">
                        <div className="grid lg:grid-cols-2 gap-12 lg:gap-16 items-center">
                            <div>
                                <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary-600">For developers &amp; agents</p>
                                <h2 className="mt-4 font-display text-3xl sm:text-4xl font-extrabold text-ink tracking-tight">
                                    Built to be built on.
                                </h2>
                                <p className="mt-4 text-lg text-muted max-w-[52ch]">
                                    A clean REST API and an official MCP server ship with every account. Automate your
                                    links from code — or let an AI assistant like Claude do it for you. Hosted or
                                    self-hosted, same keys.
                                </p>
                                <ul className="mt-7 grid grid-cols-2 gap-x-6 gap-y-3 max-w-md">
                                    {['REST API + API keys', 'Official MCP server', 'OpenAPI / Swagger', 'Self-host ready'].map((t) => (
                                        <li key={t} className="flex items-center gap-2 text-[15px] text-ink">
                                            <Check className="h-4 w-4 text-primary-600 shrink-0" aria-hidden="true" /> {t}
                                        </li>
                                    ))}
                                </ul>
                                <div className="mt-8">
                                    <Link to="/developers" className="inline-flex items-center gap-1.5 font-medium text-primary-600 hover:text-primary-700">
                                        Explore the developer docs <ArrowRight className="h-4 w-4" />
                                    </Link>
                                </div>
                            </div>

                            {/* code peek */}
                            <div className="overflow-hidden rounded-2xl border border-line2 bg-surface shadow-lift">
                                <div className="flex items-center gap-1.5 border-b border-line px-4 py-3">
                                    <span className="h-2.5 w-2.5 rounded-full bg-line2" />
                                    <span className="h-2.5 w-2.5 rounded-full bg-line2" />
                                    <span className="h-2.5 w-2.5 rounded-full bg-line2" />
                                    <span className="ml-2 font-mono text-xs text-faint">opn-mcp + api</span>
                                </div>
                                <div className="p-5 space-y-4">
                                    <pre className="overflow-x-auto font-mono text-[13px] leading-7 text-ink">
<span className="text-faint"># shorten from anywhere</span>{'\n'}
<span className="text-primary-600">curl</span> -X POST l.opn.onl/links \{'\n'}
{'  '}-H <span className="text-emerald-600">"Authorization: Bearer opn_•••"</span> \{'\n'}
{'  '}-d <span className="text-emerald-600">{'\'{"original_url":"https://…"}\''}</span>
                                    </pre>
                                    <div className="flex flex-wrap items-center gap-1.5 border-t border-line pt-4 font-mono text-[11px]">
                                        <span className="text-faint">mcp tools:</span>
                                        {['shorten_url', 'list_links', 'get_link_stats', 'get_qr_code'].map((t) => (
                                            <span key={t} className="rounded-md bg-primary-50 px-2 py-0.5 text-primary-700">{t}</span>
                                        ))}
                                        <span className="text-faint">+3</span>
                                    </div>
                                </div>
                            </div>
                        </div>
                    </div>
                </section>

                {/* ===== CTA ===== */}
                <section className="relative overflow-hidden max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-20 lg:py-28">
                    <img src="/bg-contours.png" alt="" aria-hidden="true" className="pointer-events-none absolute inset-0 h-full w-full object-cover opacity-[0.35] [mask-image:radial-gradient(90%_120%_at_15%_100%,black,transparent_70%)]" />
                    <div className="relative flex flex-col items-start gap-6 sm:flex-row sm:items-end sm:justify-between border-t border-line pt-14">
                        <div className="max-w-2xl">
                            <h2 className="font-display text-3xl sm:text-5xl font-extrabold text-ink tracking-tightest leading-[1.02]">
                                Own your links.<br />Start in seconds.
                            </h2>
                            <p className="mt-5 text-lg text-muted max-w-[50ch]">
                                Free, open source, and yours to keep. Create an account or host it yourself today.
                            </p>
                        </div>
                        <div className="flex flex-col sm:flex-row gap-3 shrink-0">
                            <Link to="/register" className="inline-flex items-center justify-center gap-2 rounded-xl bg-primary-600 px-7 py-3.5 font-semibold text-white hover:bg-primary-700">
                                Create free account <ArrowRight className="h-4 w-4" />
                            </Link>
                            <Link to="/features" className="inline-flex items-center justify-center rounded-xl border border-line2 bg-surface px-7 py-3.5 font-semibold text-ink hover:border-ink/30">
                                Explore features
                            </Link>
                        </div>
                    </div>
                    <p className="mt-10 max-w-[70ch] text-xs leading-relaxed text-faint">
                        Disclaimer: opn.onl is not responsible for content accessible through shortened links.
                        We actively remove malicious links when reported.
                        <Link to="/terms" className="ml-1 underline decoration-line2 underline-offset-2 hover:text-muted">Terms apply</Link>.
                    </p>
                </section>
            </main>
        </>
    );
}
