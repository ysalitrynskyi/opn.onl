import { useState, useEffect } from 'react';
import { Link, Outlet, useNavigate, useLocation } from 'react-router-dom';
import { Github, LogOut, Menu, X, User, Settings, LayoutDashboard, Shield } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import Logo from './Logo';

export default function Layout() {
    const navigate = useNavigate();
    const location = useLocation();
    const [token, setToken] = useState<string | null>(null);
    const [isAdmin, setIsAdmin] = useState(false);
    const [mobileMenuOpen, setMobileMenuOpen] = useState(false);
    const [userMenuOpen, setUserMenuOpen] = useState(false);

    useEffect(() => {
        setToken(localStorage.getItem('token'));
        setIsAdmin(localStorage.getItem('is_admin') === 'true');
    }, [location]);

    useEffect(() => {
        setMobileMenuOpen(false);
        setUserMenuOpen(false);
    }, [location.pathname]);

    const handleLogout = () => {
        localStorage.removeItem('token');
        localStorage.removeItem('is_admin');
        setToken(null);
        setIsAdmin(false);
        navigate('/login');
    };

    const navLinks = [
        { href: '/features', label: 'Features' },
        { href: '/pricing', label: 'Pricing' },
        { href: '/developers', label: 'Developers' },
        { href: '/docs', label: 'Docs' },
        { href: '/faq', label: 'FAQ' },
    ];

    return (
        <div className="min-h-screen flex flex-col bg-paper">
            <header className="sticky top-0 z-50 border-b border-line bg-surface/85 backdrop-blur-md">
                <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                    <div className="h-16 flex items-center justify-between">
                        <Link to="/" className="transition-opacity hover:opacity-80">
                            <Logo />
                        </Link>

                        <nav className="hidden md:flex items-center gap-8">
                            {navLinks.map(link => (
                                <Link
                                    key={link.href}
                                    to={link.href}
                                    className={`text-sm font-medium transition-colors ${
                                        location.pathname === link.href
                                            ? 'text-primary-600'
                                            : 'text-muted hover:text-ink'
                                    }`}
                                >
                                    {link.label}
                                </Link>
                            ))}
                        </nav>

                        <div className="hidden md:flex items-center gap-5">
                            <a
                                href="https://github.com/ysalitrynskyi/opn.onl"
                                target="_blank"
                                rel="noreferrer"
                                aria-label="View source on GitHub"
                                className="text-faint transition-colors hover:text-ink"
                            >
                                <Github className="h-5 w-5" />
                            </a>

                            {token ? (
                                <div className="relative">
                                    <button
                                        onClick={() => setUserMenuOpen(!userMenuOpen)}
                                        aria-label="Account menu"
                                        className="flex h-9 w-9 items-center justify-center rounded-full bg-primary-50 ring-1 ring-line transition-colors hover:ring-primary-300"
                                    >
                                        <User className="h-4 w-4 text-primary-600" />
                                    </button>

                                    <AnimatePresence>
                                        {userMenuOpen && (
                                            <>
                                                <div className="fixed inset-0 z-10" onClick={() => setUserMenuOpen(false)} />
                                                <motion.div
                                                    initial={{ opacity: 0, y: 8 }}
                                                    animate={{ opacity: 1, y: 0 }}
                                                    exit={{ opacity: 0, y: 8 }}
                                                    transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
                                                    className="absolute right-0 top-full mt-2 w-52 rounded-xl border border-line bg-surface py-1.5 shadow-lift z-20"
                                                >
                                                    <Link
                                                        to="/dashboard"
                                                        onClick={(e) => {
                                                            if (location.pathname === '/dashboard') { e.preventDefault(); window.location.reload(); }
                                                        }}
                                                        className="flex items-center gap-2.5 px-4 py-2 text-sm text-ink hover:bg-primary-50/60"
                                                    >
                                                        <LayoutDashboard className="h-4 w-4 text-muted" /> Dashboard
                                                    </Link>
                                                    <Link to="/settings" className="flex items-center gap-2.5 px-4 py-2 text-sm text-ink hover:bg-primary-50/60">
                                                        <Settings className="h-4 w-4 text-muted" /> Settings
                                                    </Link>
                                                    {isAdmin && (
                                                        <Link to="/admin" className="flex items-center gap-2.5 px-4 py-2 text-sm font-medium text-primary-700 hover:bg-primary-50/60">
                                                            <Shield className="h-4 w-4" /> Admin Panel
                                                        </Link>
                                                    )}
                                                    <hr className="my-1.5 border-line" />
                                                    <button onClick={handleLogout} className="flex w-full items-center gap-2.5 px-4 py-2 text-sm text-danger hover:bg-danger/5">
                                                        <LogOut className="h-4 w-4" /> Log out
                                                    </button>
                                                </motion.div>
                                            </>
                                        )}
                                    </AnimatePresence>
                                </div>
                            ) : (
                                <div className="flex items-center gap-2">
                                    <Link to="/login" className="rounded-lg px-3 py-2 text-sm font-medium text-muted transition-colors hover:text-ink">
                                        Log in
                                    </Link>
                                    <Link to="/register" className="rounded-lg bg-primary-600 px-4 py-2 text-sm font-semibold text-white transition-colors hover:bg-primary-700">
                                        Sign up
                                    </Link>
                                </div>
                            )}
                        </div>

                        <button
                            onClick={() => setMobileMenuOpen(!mobileMenuOpen)}
                            aria-label="Toggle menu"
                            className="md:hidden p-2 text-muted hover:text-ink"
                        >
                            {mobileMenuOpen ? <X className="h-6 w-6" /> : <Menu className="h-6 w-6" />}
                        </button>
                    </div>
                </div>

                <AnimatePresence>
                    {mobileMenuOpen && (
                        <motion.div
                            initial={{ opacity: 0, height: 0 }}
                            animate={{ opacity: 1, height: 'auto' }}
                            exit={{ opacity: 0, height: 0 }}
                            className="md:hidden overflow-hidden border-t border-line bg-surface"
                        >
                            <div className="px-4 py-4 space-y-1">
                                {navLinks.map(link => (
                                    <Link
                                        key={link.href}
                                        to={link.href}
                                        className={`block rounded-lg px-2 py-2.5 text-base font-medium ${
                                            location.pathname === link.href ? 'text-primary-600' : 'text-muted'
                                        }`}
                                    >
                                        {link.label}
                                    </Link>
                                ))}
                                <hr className="my-2 border-line" />
                                {token ? (
                                    <>
                                        <Link to="/dashboard" onClick={(e) => { if (location.pathname === '/dashboard') { e.preventDefault(); window.location.reload(); } }} className="flex items-center gap-2.5 px-2 py-2.5 text-base font-medium text-ink">
                                            <LayoutDashboard className="h-5 w-5 text-muted" /> Dashboard
                                        </Link>
                                        <Link to="/settings" className="flex items-center gap-2.5 px-2 py-2.5 text-base font-medium text-ink">
                                            <Settings className="h-5 w-5 text-muted" /> Settings
                                        </Link>
                                        <button onClick={handleLogout} className="flex w-full items-center gap-2.5 px-2 py-2.5 text-base font-medium text-danger">
                                            <LogOut className="h-5 w-5" /> Log out
                                        </button>
                                    </>
                                ) : (
                                    <div className="flex flex-col gap-2 pt-1">
                                        <Link to="/login" className="rounded-xl border border-line2 px-4 py-2.5 text-center text-base font-medium text-ink">Log in</Link>
                                        <Link to="/register" className="rounded-xl bg-primary-600 px-4 py-2.5 text-center text-base font-semibold text-white">Sign up</Link>
                                    </div>
                                )}
                                <a href="https://github.com/ysalitrynskyi/opn.onl" target="_blank" rel="noreferrer" className="flex items-center gap-2 px-2 pt-3 text-sm text-faint">
                                    <Github className="h-5 w-5" /> View on GitHub
                                </a>
                            </div>
                        </motion.div>
                    )}
                </AnimatePresence>
            </header>

            <main className="flex-grow">
                <Outlet />
            </main>

            <footer className="border-t border-line bg-surface">
                <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-14">
                    <div className="grid grid-cols-2 md:grid-cols-4 gap-8">
                        <div className="col-span-2 md:col-span-1">
                            <Logo className="mb-4" />
                            <p className="text-sm text-muted leading-relaxed max-w-[28ch]">
                                Open source, privacy-first URL shortening you can host yourself.
                            </p>
                        </div>
                        {[
                            { h: 'Product', links: [['Features', '/features'], ['Pricing', '/pricing'], ['Developers', '/developers'], ['FAQ', '/faq']] },
                            { h: 'Company', links: [['About', '/about'], ['Contact', '/contact']] },
                            { h: 'Legal', links: [['Privacy', '/privacy'], ['Terms', '/terms']] },
                        ].map(col => (
                            <div key={col.h}>
                                <h3 className="font-mono text-xs uppercase tracking-[0.18em] text-faint mb-4">{col.h}</h3>
                                <ul className="space-y-2.5">
                                    {col.links.map(([label, href]) => (
                                        <li key={href}>
                                            <Link to={href} className="text-sm text-muted transition-colors hover:text-ink">{label}</Link>
                                        </li>
                                    ))}
                                </ul>
                            </div>
                        ))}
                    </div>
                    <div className="mt-12 pt-8 border-t border-line flex flex-col sm:flex-row items-center justify-between gap-4">
                        <p className="font-mono text-xs text-faint">© {new Date().getFullYear()} opn.onl — AGPL-3.0</p>
                        <div className="flex items-center gap-6">
                            <a href="https://github.com/sponsors/ysalitrynskyi" target="_blank" rel="noreferrer" className="inline-flex items-center gap-1.5 text-sm font-medium text-rose-500 hover:text-rose-600">
                                <svg className="h-4 w-4" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                                    <path fillRule="evenodd" d="M4.25 2.5c-1.336 0-2.75 1.164-2.75 3 0 2.15 1.58 4.144 3.365 5.682A20.565 20.565 0 008 13.393a20.561 20.561 0 003.135-2.211C12.92 9.644 14.5 7.65 14.5 5.5c0-1.836-1.414-3-2.75-3-1.373 0-2.609.986-3.029 2.456a.75.75 0 01-1.442 0C6.859 3.486 5.623 2.5 4.25 2.5z" />
                                </svg>
                                Sponsor
                            </a>
                            <a href="https://github.com/ysalitrynskyi/opn.onl" target="_blank" rel="noreferrer" aria-label="GitHub" className="text-faint hover:text-ink">
                                <Github className="h-5 w-5" />
                            </a>
                        </div>
                    </div>
                </div>
            </footer>
        </div>
    );
}
