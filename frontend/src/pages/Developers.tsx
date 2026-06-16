import { useState } from 'react';
import { Link } from 'react-router-dom';
import { motion } from 'framer-motion';
import {
    ArrowRight, Terminal, Code2, Cpu, Server, Copy, Check,
    Github, BookOpen, KeyRound, Boxes,
} from 'lucide-react';
import SEO from '../components/SEO';

const API_BASE = 'https://l.opn.onl';
const MCP_REPO = 'https://github.com/ysalitrynskyi/opn-mcp';
const REGISTRY_URL = 'https://registry.modelcontextprotocol.io';
const SWAGGER_URL = `${API_BASE}/swagger-ui`;
const OPENAPI_URL = `${API_BASE}/api-docs/openapi.json`;

const ease = [0.16, 1, 0.3, 1] as const;

/* ── Copy-to-clipboard button ─────────────────────────────── */
function CopyButton({ text, light = false }: { text: string; light?: boolean }) {
    const [copied, setCopied] = useState(false);
    const onCopy = async () => {
        try {
            await navigator.clipboard.writeText(text);
            setCopied(true);
            setTimeout(() => setCopied(false), 1800);
        } catch {
            /* clipboard unavailable — no-op */
        }
    };
    return (
        <button
            onClick={onCopy}
            aria-label={copied ? 'Copied' : 'Copy to clipboard'}
            className={`inline-flex items-center gap-1.5 rounded-lg px-2.5 py-1.5 text-xs font-semibold transition-colors ${
                light
                    ? 'bg-white/10 text-white/80 hover:bg-white/20 hover:text-white'
                    : 'bg-ink/5 text-muted hover:bg-ink/10 hover:text-ink'
            }`}
        >
            {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
            {copied ? 'Copied' : 'Copy'}
        </button>
    );
}

/* ── Framed code block with a terminal chrome header ──────── */
function CodeBlock({ label, code }: { label: string; code: string }) {
    return (
        <div className="overflow-hidden rounded-2xl border border-line2 bg-surface shadow-card">
            <div className="flex items-center justify-between border-b border-line px-4 py-2.5">
                <div className="flex items-center gap-1.5">
                    <span className="h-2.5 w-2.5 rounded-full bg-line2" />
                    <span className="h-2.5 w-2.5 rounded-full bg-line2" />
                    <span className="h-2.5 w-2.5 rounded-full bg-line2" />
                    <span className="ml-2 font-mono text-xs text-faint">{label}</span>
                </div>
                <CopyButton text={code} />
            </div>
            <pre className="overflow-x-auto px-5 py-4 font-mono text-[13px] leading-6 text-ink">{code}</pre>
        </div>
    );
}

/* ── HTTP method pill ─────────────────────────────────────── */
const METHOD_STYLES: Record<string, string> = {
    GET: 'text-primary-700 bg-primary-100',
    POST: 'text-success bg-success/10',
    PUT: 'text-warning bg-warning/10',
    DELETE: 'text-danger bg-danger/10',
};
function Method({ verb }: { verb: keyof typeof METHOD_STYLES }) {
    return (
        <span className={`inline-block w-[58px] shrink-0 rounded-md py-1 text-center font-mono text-[11px] font-bold tracking-wide ${METHOD_STYLES[verb]}`}>
            {verb}
        </span>
    );
}

/* ── Data ─────────────────────────────────────────────────── */
const endpoints: { verb: keyof typeof METHOD_STYLES; path: string; desc: string }[] = [
    { verb: 'POST', path: '/links', desc: 'Shorten a URL — custom alias, password, expiry, click cap, routing.' },
    { verb: 'GET', path: '/links', desc: 'List every link on your account, with live click counts.' },
    { verb: 'PUT', path: '/links/:id', desc: 'Update a link — destination, alias, limits or rules.' },
    { verb: 'DELETE', path: '/links/:id', desc: 'Delete a link and stop its redirect.' },
    { verb: 'GET', path: '/links/:id/stats', desc: 'First-party click analytics — geo, device, referrer, timeline.' },
    { verb: 'GET', path: '/links/:id/qr', desc: 'Branded QR for a link — brand colour + logo, PNG or SVG.' },
    { verb: 'POST', path: '/links/health-check', desc: 'Check a destination is reachable before you share it.' },
];

const mcpTools = [
    'shorten_url', 'list_links', 'get_link_stats', 'update_link',
    'delete_link', 'get_qr_code', 'check_url_health',
];

const API_QUICKSTART = `# Create a key under Settings → API Keys, then:
export OPN_API_KEY="opn_your_api_key"

curl -X POST ${API_BASE}/links \\
  -H "Authorization: Bearer $OPN_API_KEY" \\
  -H "Content-Type: application/json" \\
  -d '{
        "original_url": "https://example.com/a/very/long/link",
        "custom_code": "launch"
      }'

# → { "short_url": "${API_BASE}/launch", "code": "launch" }`;

const MCP_CONFIG = `{
  "mcpServers": {
    "opn": {
      "command": "npx",
      "args": ["-y", "opn-mcp"],
      "env": {
        "OPN_API_KEY": "opn_your_api_key"
      }
    }
  }
}`;

export default function Developers() {
    return (
        <>
            <SEO
                title="Developers — REST API & MCP server"
                description="Build on opn.onl with a REST API and an official MCP server. Create an API key, shorten and manage links from your code, or let an AI assistant do it — on the hosted service or your own self-hosted instance."
                keywords="opn.onl api, url shortener api, mcp server, model context protocol, rest api, api keys, link shortener integration, self-hosted, claude mcp, developer"
                url="/developers"
            />
            <main>
                {/* ===== Hero ===== */}
                <section className="relative overflow-hidden bg-ink text-white">
                    <img src="/bg-network.png" alt="" aria-hidden="true" className="pointer-events-none absolute inset-0 h-full w-full object-cover opacity-20 [mask-image:radial-gradient(120%_110%_at_80%_0%,black,transparent_72%)]" />
                    <div className="relative max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 pt-20 pb-24 lg:pt-28 lg:pb-28">
                        <div className="grid lg:grid-cols-12 gap-12 lg:gap-10 items-center">
                            <div className="lg:col-span-6">
                                <motion.p
                                    initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.5, ease }}
                                    className="font-mono text-xs uppercase tracking-[0.2em] text-primary-300"
                                >
                                    Build on opn.onl
                                </motion.p>
                                <motion.h1
                                    initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.6, ease, delay: 0.05 }}
                                    className="mt-5 font-display font-extrabold tracking-tightest leading-[0.98] text-[clamp(2.4rem,5.5vw,4.25rem)]"
                                >
                                    Your links,
                                    <br />
                                    now <span className="text-primary-300">programmable.</span>
                                </motion.h1>
                                <motion.p
                                    initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.6, ease, delay: 0.12 }}
                                    className="mt-6 text-lg sm:text-xl text-white/70 leading-relaxed max-w-[52ch]"
                                >
                                    A clean REST API and an official MCP server. Shorten, measure and manage
                                    links from your own code — or hand the keys to your AI assistant. Same API,
                                    hosted or self-hosted.
                                </motion.p>
                                <motion.div
                                    initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.6, ease, delay: 0.18 }}
                                    className="mt-9 flex flex-wrap gap-3"
                                >
                                    <Link to="/settings" className="inline-flex items-center gap-2 rounded-xl bg-white px-6 py-3.5 font-semibold text-ink hover:bg-white/90">
                                        <KeyRound className="h-4 w-4" /> Create an API key
                                    </Link>
                                    <a href={MCP_REPO} target="_blank" rel="noreferrer" className="inline-flex items-center gap-2 rounded-xl border border-white/20 px-6 py-3.5 font-semibold text-white hover:bg-white/10">
                                        <Github className="h-4 w-4" /> The MCP server
                                    </a>
                                </motion.div>
                            </div>

                            {/* Hero terminal */}
                            <motion.div
                                initial={{ opacity: 0, y: 24 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.7, ease, delay: 0.25 }}
                                className="lg:col-span-6"
                            >
                                <div className="overflow-hidden rounded-2xl border border-white/10 bg-white/[0.04] shadow-lift backdrop-blur-sm">
                                    <div className="flex items-center gap-1.5 border-b border-white/10 px-4 py-3">
                                        <span className="h-2.5 w-2.5 rounded-full bg-white/20" />
                                        <span className="h-2.5 w-2.5 rounded-full bg-white/20" />
                                        <span className="h-2.5 w-2.5 rounded-full bg-white/20" />
                                        <span className="ml-2 font-mono text-xs text-white/40">shorten.sh</span>
                                    </div>
                                    <pre className="overflow-x-auto px-5 py-5 font-mono text-[13px] leading-7 text-white/90">
<span className="text-white/40"># one POST. one short link.</span>{'\n'}
<span className="text-primary-300">curl</span> -X POST {API_BASE}/links \{'\n'}
{'  '}-H <span className="text-emerald-300">"Authorization: Bearer opn_•••"</span> \{'\n'}
{'  '}-H <span className="text-emerald-300">"Content-Type: application/json"</span> \{'\n'}
{'  '}-d <span className="text-emerald-300">{'\'{"original_url":"https://example.com"}\''}</span>{'\n\n'}
<span className="text-success">→</span> <span className="text-white/50">{'{ "short_url": "'}</span><span className="text-primary-300">{API_BASE}/abc123</span><span className="text-white/50">{'" }'}</span>
                                    </pre>
                                </div>
                            </motion.div>
                        </div>
                    </div>
                </section>

                {/* ===== Two ways in ===== */}
                <section className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-20 lg:py-28">
                    <div className="max-w-2xl">
                        <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary-600">Two ways in</p>
                        <h2 className="mt-4 font-display text-3xl sm:text-4xl font-extrabold text-ink tracking-tight">
                            Write code, or speak to it.
                        </h2>
                        <p className="mt-4 text-lg text-muted max-w-[58ch]">
                            Every account ships with both. A plain HTTPS API for your services, and a
                            Model Context Protocol server so assistants like Claude can manage links for you.
                        </p>
                    </div>

                    <div className="mt-12 grid md:grid-cols-2 gap-px bg-line border border-line rounded-3xl overflow-hidden">
                        {/* REST API */}
                        <div className="bg-surface p-8 sm:p-10">
                            <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-primary-100 text-primary-700">
                                <Code2 className="h-6 w-6" />
                            </div>
                            <h3 className="mt-6 font-display text-xl font-bold text-ink">REST API</h3>
                            <p className="mt-2 text-[15px] leading-relaxed text-muted">
                                JSON over HTTPS, authenticated with a personal API key. Create and manage links,
                                pull analytics, generate QR codes. Fully documented with OpenAPI.
                            </p>
                            <div className="mt-5 flex flex-wrap gap-x-5 gap-y-2 text-sm">
                                <a href={SWAGGER_URL} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1.5 font-medium text-primary-600 hover:text-primary-700">
                                    <BookOpen className="h-4 w-4" /> Swagger UI
                                </a>
                                <a href={OPENAPI_URL} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1.5 font-medium text-primary-600 hover:text-primary-700">
                                    OpenAPI spec <ArrowRight className="h-3.5 w-3.5" />
                                </a>
                            </div>
                        </div>
                        {/* MCP */}
                        <div className="bg-surface p-8 sm:p-10">
                            <div className="flex h-12 w-12 items-center justify-center rounded-xl bg-primary-100 text-primary-700">
                                <Cpu className="h-6 w-6" />
                            </div>
                            <h3 className="mt-6 font-display text-xl font-bold text-ink">MCP server</h3>
                            <p className="mt-2 text-[15px] leading-relaxed text-muted">
                                The <span className="font-mono text-[13px] text-ink">opn-mcp</span> server exposes your
                                account to AI assistants over the Model Context Protocol — seven tools, zero glue code.
                                Listed in the official MCP registry.
                            </p>
                            <div className="mt-5 flex flex-wrap gap-x-5 gap-y-2 text-sm">
                                <a href={MCP_REPO} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1.5 font-medium text-primary-600 hover:text-primary-700">
                                    <Github className="h-4 w-4" /> GitHub
                                </a>
                                <a href="https://www.npmjs.com/package/opn-mcp" target="_blank" rel="noreferrer" className="inline-flex items-center gap-1.5 font-medium text-primary-600 hover:text-primary-700">
                                    npm <ArrowRight className="h-3.5 w-3.5" />
                                </a>
                                <a href={REGISTRY_URL} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1.5 font-medium text-primary-600 hover:text-primary-700">
                                    <Boxes className="h-4 w-4" /> Registry
                                </a>
                            </div>
                        </div>
                    </div>
                </section>

                {/* ===== API quickstart ===== */}
                <section className="border-t border-line bg-surface">
                    <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-20 lg:py-28">
                        <div className="grid lg:grid-cols-2 gap-12 lg:gap-16 items-center">
                            <div>
                                <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary-600">REST in 60 seconds</p>
                                <h2 className="mt-4 font-display text-3xl sm:text-4xl font-extrabold text-ink tracking-tight">
                                    A key and a <span className="text-primary-600">curl</span>.
                                </h2>
                                <ol className="mt-8 space-y-6">
                                    {[
                                        ['01', 'Create an API key', <>Open <Link to="/settings" className="text-primary-600 underline decoration-line2 underline-offset-2 hover:text-primary-700">Settings → API Keys</Link> and generate a token. It looks like <span className="font-mono text-[13px] text-ink">opn_…</span> and is shown once.</>],
                                        ['02', 'Send it as a Bearer token', <>Pass <span className="font-mono text-[13px] text-ink">Authorization: Bearer opn_…</span> on every request. The same key works for every endpoint your account can reach.</>],
                                        ['03', 'Shorten, measure, automate', <>Create links, read analytics and render QR codes straight from your code, CI or cron.</>],
                                    ].map(([n, title, body]) => (
                                        <li key={n as string} className="flex gap-5">
                                            <span className="font-mono text-sm font-semibold text-primary-600">{n}</span>
                                            <div>
                                                <h3 className="font-display text-lg font-bold text-ink">{title}</h3>
                                                <p className="mt-1.5 text-[15px] leading-relaxed text-muted">{body}</p>
                                            </div>
                                        </li>
                                    ))}
                                </ol>
                            </div>
                            <CodeBlock label="bash" code={API_QUICKSTART} />
                        </div>
                    </div>
                </section>

                {/* ===== MCP setup ===== */}
                <section className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-20 lg:py-28">
                    <div className="relative rounded-4xl bg-ink text-white overflow-hidden">
                        <img src="/bg-network.png" alt="" aria-hidden="true" className="pointer-events-none absolute inset-0 h-full w-full object-cover opacity-20 [mask-image:radial-gradient(120%_120%_at_0%_0%,black,transparent_75%)]" />
                        <div className="relative grid lg:grid-cols-2">
                            <div className="p-8 sm:p-12 lg:p-14">
                                <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary-300">Let your assistant drive</p>
                                <h2 className="mt-4 font-display text-3xl sm:text-4xl font-extrabold tracking-tight text-white">
                                    Add the MCP server in one block.
                                </h2>
                                <p className="mt-4 text-white/70 leading-relaxed max-w-[48ch]">
                                    Drop this into your MCP client config (Claude Desktop, Cursor, and friends).
                                    <span className="text-white"> npx</span> fetches <span className="font-mono text-sm text-white">opn-mcp</span> — no install,
                                    no build. Then just ask: “shorten this and show me the clicks.”
                                </p>
                                <div className="mt-6 flex flex-wrap items-center gap-x-2 gap-y-2 font-mono text-xs text-white/50">
                                    {mcpTools.map((t) => (
                                        <span key={t} className="rounded-md border border-white/10 bg-white/[0.04] px-2.5 py-1 text-white/75">{t}</span>
                                    ))}
                                </div>
                            </div>
                            <div className="border-t lg:border-t-0 lg:border-l border-white/10 p-6 sm:p-10 lg:p-12 flex flex-col justify-center gap-4">
                                <div className="overflow-hidden rounded-2xl border border-white/10 bg-black/20">
                                    <div className="flex items-center justify-between border-b border-white/10 px-4 py-2.5">
                                        <span className="font-mono text-xs text-white/40">claude_desktop_config.json</span>
                                        <CopyButton text={MCP_CONFIG} light />
                                    </div>
                                    <pre className="overflow-x-auto px-5 py-4 font-mono text-[13px] leading-6 text-white/90">{MCP_CONFIG}</pre>
                                </div>
                                <p className="text-sm text-white/55">
                                    Self-hosting? Add <span className="font-mono text-[12px] text-white/80">"OPN_BASE_URL": "https://links.yourdomain.com"</span> to
                                    point it at your own instance.
                                </p>
                            </div>
                        </div>
                    </div>
                </section>

                {/* ===== Endpoint reference ===== */}
                <section className="border-t border-line bg-surface">
                    <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-20 lg:py-28">
                        <div className="max-w-2xl">
                            <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary-600">The surface</p>
                            <h2 className="mt-4 font-display text-3xl sm:text-4xl font-extrabold text-ink tracking-tight">
                                Endpoints you’ll reach for.
                            </h2>
                            <p className="mt-4 text-lg text-muted max-w-[58ch]">
                                The essentials are below. The full contract — every field, every response —
                                lives in the <a href={SWAGGER_URL} target="_blank" rel="noreferrer" className="text-primary-600 underline decoration-line2 underline-offset-2 hover:text-primary-700">interactive API reference</a>.
                            </p>
                        </div>

                        <div className="mt-12 overflow-hidden rounded-2xl border border-line bg-paper">
                            {endpoints.map((e, i) => (
                                <div
                                    key={e.verb + e.path}
                                    className={`flex flex-col sm:flex-row sm:items-center gap-2 sm:gap-4 px-5 py-4 ${i !== 0 ? 'border-t border-line' : ''}`}
                                >
                                    <div className="flex items-center gap-3 sm:w-[280px] sm:shrink-0">
                                        <Method verb={e.verb} />
                                        <code className="font-mono text-sm font-semibold text-ink">{e.path}</code>
                                    </div>
                                    <p className="text-sm text-muted leading-relaxed">{e.desc}</p>
                                </div>
                            ))}
                        </div>
                    </div>
                </section>

                {/* ===== Self-host ===== */}
                <section className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-20 lg:py-28">
                    <div className="flex flex-col gap-8 sm:flex-row sm:items-start border-t border-line pt-14">
                        <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl bg-primary-100 text-primary-700">
                            <Server className="h-6 w-6" />
                        </div>
                        <div className="max-w-[60ch]">
                            <h2 className="font-display text-2xl sm:text-3xl font-extrabold text-ink tracking-tight">
                                Same API on your own metal.
                            </h2>
                            <p className="mt-4 text-lg text-muted leading-relaxed">
                                Nothing here is tied to our servers. Host opn.onl yourself and point your code at
                                your own base URL, or set <span className="font-mono text-sm text-ink">OPN_BASE_URL</span> for the MCP server.
                                Same keys, same endpoints, same tools — your infrastructure.
                            </p>
                            <div className="mt-6 flex flex-wrap gap-x-5 gap-y-2 text-sm">
                                <Link to="/docs" className="inline-flex items-center gap-1.5 font-medium text-primary-600 hover:text-primary-700">
                                    <BookOpen className="h-4 w-4" /> Self-hosting docs
                                </Link>
                                <a href="https://github.com/ysalitrynskyi/opn.onl" target="_blank" rel="noreferrer" className="inline-flex items-center gap-1.5 font-medium text-primary-600 hover:text-primary-700">
                                    <Github className="h-4 w-4" /> Source on GitHub
                                </a>
                            </div>
                        </div>
                    </div>
                </section>

                {/* ===== CTA ===== */}
                <section className="border-t border-line bg-surface">
                    <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-20 lg:py-28">
                        <div className="flex flex-col items-start gap-6 sm:flex-row sm:items-end sm:justify-between">
                            <div className="max-w-2xl">
                                <h2 className="font-display text-3xl sm:text-5xl font-extrabold text-ink tracking-tightest leading-[1.02]">
                                    Ship your first<br />short link in a minute.
                                </h2>
                                <p className="mt-5 text-lg text-muted max-w-[50ch]">
                                    Generate a key, copy the curl, and you’re live. Free, open source, and yours to host.
                                </p>
                            </div>
                            <div className="flex flex-col sm:flex-row gap-3 shrink-0">
                                <Link to="/settings" className="inline-flex items-center justify-center gap-2 rounded-xl bg-primary-600 px-7 py-3.5 font-semibold text-white hover:bg-primary-700">
                                    <KeyRound className="h-4 w-4" /> Create an API key
                                </Link>
                                <a href={SWAGGER_URL} target="_blank" rel="noreferrer" className="inline-flex items-center justify-center gap-2 rounded-xl border border-line2 bg-surface px-7 py-3.5 font-semibold text-ink hover:border-ink/30">
                                    <Terminal className="h-4 w-4" /> API reference
                                </a>
                            </div>
                        </div>
                    </div>
                </section>
            </main>
        </>
    );
}
