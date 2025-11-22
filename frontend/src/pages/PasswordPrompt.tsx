import React, { useState } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { Lock, Loader2, ArrowRight, ShieldAlert } from 'lucide-react';
import { motion } from 'framer-motion';
import { API_ENDPOINTS } from '../config/api';

export default function PasswordPrompt() {
    const { code } = useParams<{ code: string }>();
    const navigate = useNavigate();
    const [password, setPassword] = useState('');
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState('');

    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        setLoading(true);
        setError('');

        try {
            const res = await fetch(API_ENDPOINTS.verifyPassword(code!), {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ password }),
            });

            const data = await res.json();

            if (res.ok && data.url) {
                // Redirect to the actual destination
                window.location.href = data.url;
            } else if (res.status === 401) {
                setError('Incorrect password. Please try again.');
            } else if (res.status === 410) {
                setError('This link has expired.');
            } else if (res.status === 404) {
                setError('Link not found.');
            } else {
                setError(data.error || 'Something went wrong. Please try again.');
            }
        } catch (error) {
            console.error('Password verification failed', error);
            setError('Network error. Please check your connection and try again.');
        } finally {
            setLoading(false);
        }
    };

    return (
        <div className="min-h-[80vh] flex items-center justify-center px-4">
            <motion.div
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
                className="w-full max-w-md"
            >
                <div className="bg-white rounded-2xl shadow-xl border border-slate-100 overflow-hidden">
                    {/* Header */}
                    <div className="bg-gradient-to-br from-slate-900 to-slate-800 px-8 py-10 text-center">
                        <motion.div
                            initial={{ scale: 0 }}
                            animate={{ scale: 1 }}
                            transition={{ delay: 0.2, type: 'spring' }}
                            className="h-16 w-16 bg-white/10 rounded-2xl flex items-center justify-center mx-auto mb-4 backdrop-blur-sm"
                        >
                            <Lock className="h-8 w-8 text-white" />
                        </motion.div>
                        <h1 className="text-2xl font-bold text-white mb-2">Password Protected</h1>
                        <p className="text-slate-300 text-sm">
                            This link is protected. Enter the password to continue.
                        </p>
                    </div>

                    {/* Form */}
                    <div className="p-8">
                        <form onSubmit={handleSubmit} className="space-y-6">
                            <div>
                                <label htmlFor="password" className="block text-sm font-medium text-slate-700 mb-2">
                                    Password
                                </label>
                                <div className="relative">
                                    <input
                                        id="password"
                                        type="password"
                                        value={password}
                                        onChange={(e) => setPassword(e.target.value)}
                                        placeholder="Enter the link password"
                                        className="w-full px-4 py-3 border border-slate-300 rounded-xl focus:border-primary-500 focus:ring-2 focus:ring-primary-500/20 outline-none transition-all"
                                        required
                                        autoFocus
                                    />
                                </div>
                            </div>

                            {error && (
                                <motion.div
                                    initial={{ opacity: 0, y: -10 }}
                                    animate={{ opacity: 1, y: 0 }}
                                    className="flex items-center gap-3 p-4 bg-red-50 border border-red-200 rounded-xl text-red-700"
                                >
                                    <ShieldAlert className="h-5 w-5 flex-shrink-0" />
                                    <p className="text-sm">{error}</p>
                                </motion.div>
                            )}

                            <button
                                type="submit"
                                disabled={loading || !password}
                                className="w-full bg-primary-600 text-white py-3 px-4 rounded-xl font-semibold hover:bg-primary-700 transition-colors disabled:opacity-70 disabled:cursor-not-allowed flex items-center justify-center gap-2"
                            >
                                {loading ? (
                                    <Loader2 className="h-5 w-5 animate-spin" />
                                ) : (
                                    <>
                                        Continue
                                        <ArrowRight className="h-4 w-4" />
                                    </>
                                )}
                            </button>
                        </form>

                        <div className="mt-6 pt-6 border-t border-slate-100">
                            <p className="text-center text-sm text-slate-500">
                                Don't know the password?{' '}
                                <button 
                                    onClick={() => navigate('/')}
                                    className="text-primary-600 hover:text-primary-700 font-medium"
                                >
                                    Go to homepage
                                </button>
                            </p>
                        </div>
                    </div>
                </div>

                {/* Security note */}
                <motion.p
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    transition={{ delay: 0.4 }}
                    className="text-center text-xs text-slate-400 mt-6"
                >
                    🔒 Your connection is secure. Password is transmitted over HTTPS.
                </motion.p>
            </motion.div>
        </div>
    );
}

