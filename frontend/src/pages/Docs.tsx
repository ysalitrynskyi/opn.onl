import { motion } from 'framer-motion';
import { 
    Code, Terminal, Server, Key, Link2, BarChart2, 
    Shield, Clock, Folder, Tag, Users, Globe, 
    Mail, Github, BookOpen, Zap, Database, Lock
} from 'lucide-react';
import SEO from '../components/SEO';

const API_BASE = import.meta.env.VITE_API_URL || 'https://l.opn.onl';

export default function Docs() {
    return (
        <div className="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8 py-12">
            <SEO 
                title="Documentation" 
                description="Learn how to use the opn.onl API to create and manage short links programmatically."
                keywords="opn.onl api, url shortener api, link shortening api, documentation"
            />

            <motion.div
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
                className="mb-12"
            >
                <h1 className="text-4xl font-bold text-slate-900 mb-4">Documentation</h1>
                <p className="text-xl text-slate-600">
                    Learn how to use opn.onl to shorten links, track analytics, and integrate with your applications.
                </p>
            </motion.div>

            {/* Quick Links */}
            <div className="grid sm:grid-cols-3 gap-4 mb-12">
                <a href="#api" className="p-4 bg-primary-50 border border-primary-200 rounded-xl hover:bg-primary-100 transition-colors">
                    <Code className="h-6 w-6 text-primary-600 mb-2" />
                    <h3 className="font-semibold text-slate-900">API Reference</h3>
                    <p className="text-sm text-slate-600">RESTful API endpoints</p>
                </a>
                <a href="#self-host" className="p-4 bg-emerald-50 border border-emerald-200 rounded-xl hover:bg-emerald-100 transition-colors">
                    <Server className="h-6 w-6 text-emerald-600 mb-2" />
                    <h3 className="font-semibold text-slate-900">Self-Hosting</h3>
                    <p className="text-sm text-slate-600">Run your own instance</p>
                </a>
                <a href="#support" className="p-4 bg-amber-50 border border-amber-200 rounded-xl hover:bg-amber-100 transition-colors">
                    <Mail className="h-6 w-6 text-amber-600 mb-2" />
                    <h3 className="font-semibold text-slate-900">Get Help</h3>
                    <p className="text-sm text-slate-600">Contact for support</p>
                </a>
            </div>

            {/* Getting Started */}
            <section className="mb-12">
                <h2 className="text-2xl font-bold text-slate-900 mb-6 flex items-center gap-2">
                    <Zap className="h-6 w-6 text-primary-600" />
                    Getting Started
                </h2>
                <div className="prose prose-slate max-w-none">
                    <p>
                        opn.onl provides a simple REST API for creating and managing short links. 
                        You can use it directly via HTTP requests or integrate it into your applications.
                    </p>
                    <div className="bg-slate-900 rounded-xl p-4 overflow-x-auto">
                        <code className="text-green-400 text-sm">
                            # Create a short link (no authentication required)<br/>
                            curl -X POST {API_BASE}/links \<br/>
                            &nbsp;&nbsp;-H "Content-Type: application/json" \<br/>
                            &nbsp;&nbsp;-d '{`{"original_url": "https://example.com/very-long-url"}`}'
                        </code>
                    </div>
                </div>
            </section>

            {/* API Reference */}
            <section id="api" className="mb-12">
                <h2 className="text-2xl font-bold text-slate-900 mb-6 flex items-center gap-2">
                    <Code className="h-6 w-6 text-primary-600" />
                    API Reference
                </h2>
                
                <div className="space-y-6">
                    {/* Authentication */}
                    <div className="bg-white border border-slate-200 rounded-xl overflow-hidden">
                        <div className="p-4 bg-slate-50 border-b border-slate-200">
                            <h3 className="font-semibold text-slate-900 flex items-center gap-2">
                                <Key className="h-5 w-5" />
                                Authentication
                            </h3>
                        </div>
                        <div className="p-4 space-y-3">
                            <p className="text-slate-600">
                                Most endpoints require authentication via Bearer token. Get your token by logging in:
                            </p>
                            <div className="bg-slate-900 rounded-lg p-3 overflow-x-auto">
                                <code className="text-green-400 text-sm">
                                    POST {API_BASE}/auth/login<br/>
                                    {`{"email": "user@example.com", "password": "yourpassword"}`}
                                </code>
                            </div>
                            <p className="text-slate-600">
                                Include the token in subsequent requests:
                            </p>
                            <div className="bg-slate-900 rounded-lg p-3 overflow-x-auto">
                                <code className="text-green-400 text-sm">
                                    Authorization: Bearer your-jwt-token
                                </code>
                            </div>
                        </div>
                    </div>

                    {/* Create Link */}
                    <div className="bg-white border border-slate-200 rounded-xl overflow-hidden">
                        <div className="p-4 bg-slate-50 border-b border-slate-200">
                            <div className="flex items-center gap-2">
                                <span className="px-2 py-1 bg-green-100 text-green-700 rounded text-xs font-bold">POST</span>
                                <code className="text-slate-700">/links</code>
                            </div>
                            <p className="text-sm text-slate-600 mt-1">Create a new short link</p>
                        </div>
                        <div className="p-4 space-y-3">
                            <h4 className="font-medium text-slate-900">Request Body</h4>
                            <div className="bg-slate-900 rounded-lg p-3 overflow-x-auto">
                                <code className="text-green-400 text-sm whitespace-pre">{`{
  "original_url": "https://example.com/page",  // Required
  "custom_alias": "my-link",                    // Optional (5-50 chars)
  "title": "My Link Title",                     // Optional, private
  "password": "secret123",                      // Optional
  "expires_at": "2025-12-31T23:59:59Z",        // Optional
  "notes": "Internal notes"                     // Optional
}`}</code>
                            </div>
                            <h4 className="font-medium text-slate-900">Response</h4>
                            <div className="bg-slate-900 rounded-lg p-3 overflow-x-auto">
                                <code className="text-green-400 text-sm whitespace-pre">{`{
  "id": 123,
  "code": "abc123",
  "short_url": "${API_BASE}/abc123",
  "original_url": "https://example.com/page",
  "click_count": 0,
  "created_at": "2025-01-01T00:00:00Z"
}`}</code>
                            </div>
                        </div>
                    </div>

                    {/* Get Links */}
                    <div className="bg-white border border-slate-200 rounded-xl overflow-hidden">
                        <div className="p-4 bg-slate-50 border-b border-slate-200">
                            <div className="flex items-center gap-2">
                                <span className="px-2 py-1 bg-blue-100 text-blue-700 rounded text-xs font-bold">GET</span>
                                <code className="text-slate-700">/links</code>
                            </div>
                            <p className="text-sm text-slate-600 mt-1">List your links (requires auth)</p>
                        </div>
                        <div className="p-4 space-y-3">
                            <h4 className="font-medium text-slate-900">Query Parameters</h4>
                            <ul className="text-sm text-slate-600 space-y-1">
                                <li><code className="bg-slate-100 px-1 rounded">search</code> - Search by URL or code</li>
                                <li><code className="bg-slate-100 px-1 rounded">folder_id</code> - Filter by folder</li>
                                <li><code className="bg-slate-100 px-1 rounded">tag_id</code> - Filter by tag</li>
                                <li><code className="bg-slate-100 px-1 rounded">limit</code> - Number of results (default: 50)</li>
                                <li><code className="bg-slate-100 px-1 rounded">offset</code> - Pagination offset</li>
                            </ul>
                        </div>
                    </div>

                    {/* Other Endpoints */}
                    <div className="bg-white border border-slate-200 rounded-xl overflow-hidden">
                        <div className="p-4 bg-slate-50 border-b border-slate-200">
                            <h3 className="font-semibold text-slate-900">Other Endpoints</h3>
                        </div>
                        <div className="divide-y divide-slate-100">
                            <div className="p-4 flex items-center gap-3">
                                <span className="px-2 py-1 bg-blue-100 text-blue-700 rounded text-xs font-bold">GET</span>
                                <code className="text-slate-700">/links/{'{id}'}</code>
                                <span className="text-slate-500 text-sm">Get link details</span>
                            </div>
                            <div className="p-4 flex items-center gap-3">
                                <span className="px-2 py-1 bg-yellow-100 text-yellow-700 rounded text-xs font-bold">PUT</span>
                                <code className="text-slate-700">/links/{'{id}'}</code>
                                <span className="text-slate-500 text-sm">Update a link</span>
                            </div>
                            <div className="p-4 flex items-center gap-3">
                                <span className="px-2 py-1 bg-red-100 text-red-700 rounded text-xs font-bold">DELETE</span>
                                <code className="text-slate-700">/links/{'{id}'}</code>
                                <span className="text-slate-500 text-sm">Delete a link</span>
                            </div>
                            <div className="p-4 flex items-center gap-3">
                                <span className="px-2 py-1 bg-blue-100 text-blue-700 rounded text-xs font-bold">GET</span>
                                <code className="text-slate-700">/links/{'{id}'}/stats</code>
                                <span className="text-slate-500 text-sm">Get link analytics</span>
                            </div>
                            <div className="p-4 flex items-center gap-3">
                                <span className="px-2 py-1 bg-blue-100 text-blue-700 rounded text-xs font-bold">GET</span>
                                <code className="text-slate-700">/links/{'{id}'}/qr</code>
                                <span className="text-slate-500 text-sm">Generate QR code</span>
                            </div>
                            <div className="p-4 flex items-center gap-3">
                                <span className="px-2 py-1 bg-blue-100 text-blue-700 rounded text-xs font-bold">GET</span>
                                <code className="text-slate-700">/links/export</code>
                                <span className="text-slate-500 text-sm">Export links as CSV</span>
                            </div>
                        </div>
                    </div>

                    {/* Swagger */}
                    <div className="bg-primary-50 border border-primary-200 rounded-xl p-4">
                        <p className="text-primary-800">
                            <BookOpen className="inline h-5 w-5 mr-2" />
                            Full interactive API documentation is available at{' '}
                            <a href={`${API_BASE}/swagger-ui/`} target="_blank" rel="noreferrer" className="font-semibold underline">
                                {API_BASE}/swagger-ui/
                            </a>
                        </p>
                    </div>
                </div>
            </section>

            {/* Self-Hosting */}
            <section id="self-host" className="mb-12">
                <h2 className="text-2xl font-bold text-slate-900 mb-6 flex items-center gap-2">
                    <Server className="h-6 w-6 text-primary-600" />
                    Self-Hosting Guide
                </h2>
                
                <div className="prose prose-slate max-w-none">
                    <p>
                        opn.onl is fully open-source and can be self-hosted on your own infrastructure. 
                        The stack consists of a Rust backend and React frontend.
                    </p>

                    <h3 className="flex items-center gap-2 mt-8">
                        <Database className="h-5 w-5" />
                        Requirements
                    </h3>
                    <ul>
                        <li>Docker and Docker Compose</li>
                        <li>PostgreSQL database</li>
                        <li>Redis (optional, for caching)</li>
                        <li>SMTP server (for email verification)</li>
                    </ul>

                    <h3 className="flex items-center gap-2 mt-8">
                        <Terminal className="h-5 w-5" />
                        Quick Start with Docker
                    </h3>
                    <div className="bg-slate-900 rounded-xl p-4 overflow-x-auto not-prose">
                        <code className="text-green-400 text-sm">
                            # Clone the repository<br/>
                            git clone https://github.com/ysalitrynskyi/opn.onl.git<br/>
                            cd opn.onl<br/><br/>
                            # Copy environment file<br/>
                            cp .env.example .env<br/><br/>
                            # Edit .env with your settings<br/>
                            nano .env<br/><br/>
                            # Start with Docker Compose<br/>
                            docker-compose up -d
                        </code>
                    </div>

                    <h3 className="flex items-center gap-2 mt-8">
                        <Lock className="h-5 w-5" />
                        Environment Variables
                    </h3>
                    <div className="bg-white border border-slate-200 rounded-xl overflow-x-auto not-prose">
                        <table className="w-full text-sm">
                            <thead className="bg-slate-50">
                                <tr>
                                    <th className="text-left p-3 font-semibold">Variable</th>
                                    <th className="text-left p-3 font-semibold">Description</th>
                                    <th className="text-left p-3 font-semibold">Default</th>
                                </tr>
                            </thead>
                            <tbody className="divide-y divide-slate-100">
                                <tr><td className="p-3"><code>DATABASE_URL</code></td><td className="p-3">PostgreSQL connection string</td><td className="p-3">Required</td></tr>
                                <tr><td className="p-3"><code>JWT_SECRET</code></td><td className="p-3">Secret for JWT tokens</td><td className="p-3">Required</td></tr>
                                <tr><td className="p-3"><code>BASE_URL</code></td><td className="p-3">API base URL</td><td className="p-3">http://localhost:3000</td></tr>
                                <tr><td className="p-3"><code>FRONTEND_URL</code></td><td className="p-3">Frontend URL</td><td className="p-3">http://localhost:5173</td></tr>
                                <tr><td className="p-3"><code>REDIS_URL</code></td><td className="p-3">Redis connection (optional)</td><td className="p-3">-</td></tr>
                                <tr><td className="p-3"><code>SMTP_HOST</code></td><td className="p-3">SMTP server hostname</td><td className="p-3">-</td></tr>
                                <tr><td className="p-3"><code>MIN_ALIAS_LENGTH</code></td><td className="p-3">Minimum custom alias length</td><td className="p-3">5</td></tr>
                                <tr><td className="p-3"><code>ENABLE_URL_SANITIZATION</code></td><td className="p-3">Block malicious URLs</td><td className="p-3">true</td></tr>
                            </tbody>
                        </table>
                    </div>

                    <p className="mt-6">
                        For complete setup instructions, see the{' '}
                        <a href="https://github.com/ysalitrynskyi/opn.onl" target="_blank" rel="noreferrer" className="font-semibold">
                            GitHub repository
                        </a>.
                    </p>
                </div>
            </section>

            {/* Features */}
            <section className="mb-12">
                <h2 className="text-2xl font-bold text-slate-900 mb-6 flex items-center gap-2">
                    <Zap className="h-6 w-6 text-primary-600" />
                    Features
                </h2>
                
                <div className="grid sm:grid-cols-2 gap-4">
                    {[
                        { icon: Link2, title: 'Custom Aliases', desc: 'Create memorable short links' },
                        { icon: BarChart2, title: 'Analytics', desc: 'Track clicks, locations, devices' },
                        { icon: Shield, title: 'Password Protection', desc: 'Secure sensitive links' },
                        { icon: Clock, title: 'Expiration Dates', desc: 'Auto-expire links' },
                        { icon: Folder, title: 'Folders', desc: 'Organize your links' },
                        { icon: Tag, title: 'Tags', desc: 'Label and filter links' },
                        { icon: Users, title: 'Teams', desc: 'Collaborate with others' },
                        { icon: Globe, title: 'GeoIP Analytics', desc: 'See where clicks come from' },
                    ].map((feature, i) => (
                        <div key={i} className="flex items-start gap-3 p-4 bg-slate-50 rounded-xl">
                            <feature.icon className="h-5 w-5 text-primary-600 flex-shrink-0 mt-0.5" />
                            <div>
                                <h4 className="font-medium text-slate-900">{feature.title}</h4>
                                <p className="text-sm text-slate-600">{feature.desc}</p>
                            </div>
                        </div>
                    ))}
                </div>
            </section>

            {/* Support */}
            <section id="support" className="mb-12">
                <h2 className="text-2xl font-bold text-slate-900 mb-6 flex items-center gap-2">
                    <Mail className="h-6 w-6 text-primary-600" />
                    Get Help
                </h2>
                
                <div className="bg-gradient-to-br from-primary-50 to-indigo-50 border border-primary-200 rounded-2xl p-8">
                    <div className="max-w-2xl">
                        <h3 className="text-xl font-bold text-slate-900 mb-4">Need Custom Development?</h3>
                        <p className="text-slate-600 mb-6">
                            Whether you need help setting up your own instance, custom integrations, 
                            or additional features - I'm here to help!
                        </p>
                        <div className="flex flex-col sm:flex-row gap-4">
                            <a 
                                href="mailto:ys@opn.onl" 
                                className="inline-flex items-center justify-center gap-2 px-6 py-3 bg-primary-600 text-white rounded-xl font-semibold hover:bg-primary-700 transition-colors"
                            >
                                <Mail className="h-5 w-5" />
                                Contact: ys@opn.onl
                            </a>
                            <a 
                                href="https://github.com/ysalitrynskyi/opn.onl" 
                                target="_blank"
                                rel="noreferrer"
                                className="inline-flex items-center justify-center gap-2 px-6 py-3 bg-slate-900 text-white rounded-xl font-semibold hover:bg-slate-800 transition-colors"
                            >
                                <Github className="h-5 w-5" />
                                GitHub
                            </a>
                        </div>
                        <p className="text-sm text-slate-500 mt-4">
                            Built with ❤️ by <a href="https://github.com/ysalitrynskyi" className="underline">Yevhen Salitrynskyi</a>
                        </p>
                    </div>
                </div>
            </section>
        </div>
    );
}



