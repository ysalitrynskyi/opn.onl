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
                        <div className="flex items-center gap-6">
                            <a 
                                href="https://github.com/sponsors/ysalitrynskyi" 
                                target="_blank"
                                rel="noreferrer"
                                className="inline-flex items-center gap-2 text-pink-500 hover:text-pink-600 text-sm font-medium"
                            >
                                <svg className="h-4 w-4" viewBox="0 0 16 16" fill="currentColor">
                                    <path fillRule="evenodd" d="M4.25 2.5c-1.336 0-2.75 1.164-2.75 3 0 2.15 1.58 4.144 3.365 5.682A20.565 20.565 0 008 13.393a20.561 20.561 0 003.135-2.211C12.92 9.644 14.5 7.65 14.5 5.5c0-1.836-1.414-3-2.75-3-1.373 0-2.609.986-3.029 2.456a.75.75 0 01-1.442 0C6.859 3.486 5.623 2.5 4.25 2.5zM8 14.25l-.345.666-.002-.001-.006-.003-.018-.01a7.643 7.643 0 01-.31-.17 22.075 22.075 0 01-3.434-2.414C2.045 10.731 0 8.35 0 5.5 0 2.836 2.086 1 4.25 1 5.797 1 7.153 1.802 8 3.02 8.847 1.802 10.203 1 11.75 1 13.914 1 16 2.836 16 5.5c0 2.85-2.045 5.231-3.885 6.818a22.08 22.08 0 01-3.744 2.584l-.018.01-.006.003h-.002L8 14.25zm0 0l.345.666a.752.752 0 01-.69 0L8 14.25z"/>
                                </svg>
                                Sponsor
                            </a>
                            <a href="https://github.com/ysalitrynskyi/opn.onl" target="_blank" rel="noreferrer" className="text-slate-400 hover:text-slate-600">
                                <Github className="h-5 w-5" />
                            </a>
                        </div>
                    </div>
                </div>
            </footer>
        </div>
    );
}
