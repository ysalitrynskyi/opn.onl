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
        // Close mobile menu on route change
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
        { href: '/faq', label: 'FAQ' },
    ];

    return (
        <div className="min-h-screen flex flex-col bg-slate-50">
            <header className="bg-white border-b border-slate-200 sticky top-0 z-50">
                <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                    <div className="h-16 flex items-center justify-between">
                        {/* Logo */}
                        <Link to="/" className="hover:opacity-80 transition-opacity">
                            <Logo />
                        </Link>

                        {/* Desktop Navigation */}
                        <nav className="hidden md:flex items-center gap-8">
                            {navLinks.map(link => (
                                <Link
                                    key={link.href}
                                    to={link.href}
                                    className={`text-sm font-medium transition-colors ${
                                        location.pathname === link.href
                                            ? 'text-primary-600'
                                            : 'text-slate-600 hover:text-slate-900'
                                    }`}
                                >
                                    {link.label}
                                </Link>
                            ))}
                        </nav>

                        {/* Desktop Auth */}
                        <div className="hidden md:flex items-center gap-4">
                            <a
                                href="https://github.com/ysalitrynskyi/opn.onl"
                                target="_blank"
                                rel="noreferrer"
                                className="text-slate-500 hover:text-slate-900 transition-colors"
                            >
                                <Github className="h-5 w-5" />
                            </a>
                            
                            {token ? (
                                <div className="relative">
                                    <button
                                        onClick={() => setUserMenuOpen(!userMenuOpen)}
                                        className="flex items-center gap-2 px-3 py-2 rounded-lg hover:bg-slate-100 transition-colors"
                                    >
                                        <div className="h-8 w-8 bg-primary-100 rounded-full flex items-center justify-center">
                                            <User className="h-4 w-4 text-primary-600" />
                                        </div>
                                    </button>
                                    
                                    <AnimatePresence>
                                        {userMenuOpen && (
                                            <>
                                                <div 
                                                    className="fixed inset-0 z-10" 
                                                    onClick={() => setUserMenuOpen(false)} 
                                                />
                                                <motion.div
                                                    initial={{ opacity: 0, y: 10 }}
                                                    animate={{ opacity: 1, y: 0 }}
                                                    exit={{ opacity: 0, y: 10 }}
                                                    className="absolute right-0 top-full mt-2 w-48 bg-white rounded-xl shadow-lg border border-slate-200 py-2 z-20"
                                                >
                                                    <Link
                                                        to="/dashboard"
                                                        onClick={(e) => {
                                                            if (location.pathname === '/dashboard') {
                                                                e.preventDefault();
                                                                window.location.reload();
                                                            }
                                                        }}
                                                        className="flex items-center gap-2 px-4 py-2 text-sm text-slate-700 hover:bg-slate-50"
                                                    >
                                                        <LayoutDashboard className="h-4 w-4" />
                                                        Dashboard
                                                    </Link>
                                                    <Link
                                                        to="/settings"
                                                        className="flex items-center gap-2 px-4 py-2 text-sm text-slate-700 hover:bg-slate-50"
                                                    >
                                                        <Settings className="h-4 w-4" />
                                                        Settings
                                                    </Link>
                                                    {isAdmin && (
                                                        <Link
                                                            to="/admin"
                                                            className="flex items-center gap-2 px-4 py-2 text-sm text-primary-700 hover:bg-primary-50"
                                                        >
                                                            <Shield className="h-4 w-4" />
                                                            Admin Panel
                                                        </Link>
                                                    )}
                                                    <hr className="my-2 border-slate-100" />
                                                    <button
                                                        onClick={handleLogout}
                                                        className="flex items-center gap-2 px-4 py-2 text-sm text-red-600 hover:bg-red-50 w-full"
                                                    >
                                                        <LogOut className="h-4 w-4" />
                                                        Logout
                                                    </button>
                                                </motion.div>
                                            </>
                                        )}
                                    </AnimatePresence>
                                </div>
                            ) : (
                                <div className="flex items-center gap-3">
                                    <Link 
                                        to="/login" 
                                        className="text-sm font-medium text-slate-700 hover:text-primary-600 transition-colors"
                                    >
                                        Log in
                                    </Link>
                                    <Link
                                        to="/register"
                                        className="bg-primary-600 text-white px-4 py-2 rounded-lg text-sm font-medium hover:bg-primary-700 transition-colors"
                                    >
                                        Sign up
                                    </Link>
                                </div>
                            )}
                        </div>

                        {/* Mobile Menu Button */}
                        <button
                            onClick={() => setMobileMenuOpen(!mobileMenuOpen)}
                            className="md:hidden p-2 text-slate-600 hover:text-slate-900"
                        >
                            {mobileMenuOpen ? <X className="h-6 w-6" /> : <Menu className="h-6 w-6" />}
                        </button>
                    </div>
                </div>

                {/* Mobile Menu */}
                <AnimatePresence>
                    {mobileMenuOpen && (
                        <motion.div
                            initial={{ opacity: 0, height: 0 }}
                            animate={{ opacity: 1, height: 'auto' }}
                            exit={{ opacity: 0, height: 0 }}
                            className="md:hidden border-t border-slate-200 bg-white overflow-hidden"
                        >
                            <div className="px-4 py-4 space-y-3">
                                {navLinks.map(link => (
                                    <Link
                                        key={link.href}
                                        to={link.href}
                                        className={`block py-2 text-base font-medium ${
                                            location.pathname === link.href
                                                ? 'text-primary-600'
                                                : 'text-slate-600'
                                        }`}
                                    >
                                        {link.label}
                                    </Link>
                                ))}
                                
                                <hr className="border-slate-200" />
                                
                                {token ? (
                                    <>
                                        <Link
                                            to="/dashboard"
                                            onClick={(e) => {
                                                if (location.pathname === '/dashboard') {
                                                    e.preventDefault();
                                                    window.location.reload();
                                                }
                                            }}
                                            className="flex items-center gap-2 py-2 text-base font-medium text-slate-600"
                                        >
                                            <LayoutDashboard className="h-5 w-5" />
                                            Dashboard
                                        </Link>
                                        <Link
                                            to="/settings"
                                            className="flex items-center gap-2 py-2 text-base font-medium text-slate-600"
                                        >
                                            <Settings className="h-5 w-5" />
                                            Settings
                                        </Link>
                                        <button
                                            onClick={handleLogout}
                                            className="flex items-center gap-2 py-2 text-base font-medium text-red-600 w-full"
                                        >
                                            <LogOut className="h-5 w-5" />
                                            Logout
                                        </button>
                                    </>
                                ) : (
                                    <div className="flex flex-col gap-3 pt-2">
                                        <Link
                                            to="/login"
                                            className="text-center py-2.5 text-base font-medium text-slate-700 border border-slate-300 rounded-lg"
                                        >
                                            Log in
                                        </Link>
                                        <Link
                                            to="/register"
                                            className="text-center py-2.5 text-base font-medium text-white bg-primary-600 rounded-lg"
                                        >
                                            Sign up
                                        </Link>
                                    </div>
                                )}

                                <div className="pt-4">
                                    <a
                                        href="https://github.com/ysalitrynskyi/opn.onl"
                                        target="_blank"
                                        rel="noreferrer"
                                        className="flex items-center gap-2 text-slate-500"
                                    >
                                        <Github className="h-5 w-5" />
                                        <span className="text-sm">View on GitHub</span>
                                    </a>
                                </div>
                            </div>
                        </motion.div>
                    )}
                </AnimatePresence>
            </header>

            <main className="flex-grow">
                <Outlet />
            </main>

            <footer className="bg-white border-t border-slate-200 py-12">
                <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                    <div className="grid grid-cols-2 md:grid-cols-4 gap-8">
                        <div className="col-span-2 md:col-span-1">
                            <Logo className="h-6 mb-4" />
                            <p className="text-sm text-slate-500">
                                Open source, privacy-focused URL shortener for the modern web.
                            </p>
                        </div>
                        <div>
                            <h3 className="text-sm font-semibold text-slate-900 tracking-wider uppercase mb-4">Product</h3>
                            <ul className="space-y-3">
                                <li><Link to="/features" className="text-sm text-slate-600 hover:text-primary-600">Features</Link></li>
                                <li><Link to="/pricing" className="text-sm text-slate-600 hover:text-primary-600">Pricing</Link></li>
                                <li><Link to="/faq" className="text-sm text-slate-600 hover:text-primary-600">FAQ</Link></li>
                            </ul>
                        </div>
                        <div>
                            <h3 className="text-sm font-semibold text-slate-900 tracking-wider uppercase mb-4">Company</h3>
                            <ul className="space-y-3">
                                <li><Link to="/about" className="text-sm text-slate-600 hover:text-primary-600">About</Link></li>
                                <li><Link to="/contact" className="text-sm text-slate-600 hover:text-primary-600">Contact</Link></li>
                            </ul>
                        </div>
                        <div>
                            <h3 className="text-sm font-semibold text-slate-900 tracking-wider uppercase mb-4">Legal</h3>
                            <ul className="space-y-3">
                                <li><Link to="/privacy" className="text-sm text-slate-600 hover:text-primary-600">Privacy</Link></li>
                                <li><Link to="/terms" className="text-sm text-slate-600 hover:text-primary-600">Terms</Link></li>
                            </ul>
                        </div>
                    </div>
                    <div className="mt-12 pt-8 border-t border-slate-100 flex flex-col sm:flex-row items-center justify-between gap-4">
                        <p className="text-sm text-slate-400">
                            &copy; {new Date().getFullYear()} opn.onl. All rights reserved.
                        </p>
                        <div className="flex items-center gap-4">
                            <a href="https://github.com/ysalitrynskyi/opn.onl" className="text-slate-400 hover:text-slate-600">
                                <Github className="h-5 w-5" />
                            </a>
                        </div>
                    </div>
                </div>
            </footer>
        </div>
    );
}
