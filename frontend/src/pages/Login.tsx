import { useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { Loader2, Fingerprint, Mail, Send, CheckCircle } from 'lucide-react';
import { motion } from 'framer-motion';
import { API_ENDPOINTS } from '../config/api';
import logger from '../utils/logger';

export default function Login() {
    const [email, setEmail] = useState('');
    const [password, setPassword] = useState('');
    const [loading, setLoading] = useState(false);
    const [passkeyLoading, setPasskeyLoading] = useState(false);
    const [error, setError] = useState('');
    const [needsVerification, setNeedsVerification] = useState(false);
    const [resendLoading, setResendLoading] = useState(false);
    const [resendSuccess, setResendSuccess] = useState(false);
    const showPasskeyOption = true;
    const navigate = useNavigate();

    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        setLoading(true);
        setError('');
        setNeedsVerification(false);

        try {
            const res = await fetch(API_ENDPOINTS.login, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ email, password }),
            });

            const data = await res.json();

            if (!res.ok) throw new Error(data.error || 'Login failed');

            // Check if email needs verification
            if (data.email_verified === false) {
                setNeedsVerification(true);
                localStorage.setItem('token', data.token);
                localStorage.setItem('is_admin', data.is_admin ? 'true' : 'false');
                return;
            }

            localStorage.setItem('token', data.token);
            localStorage.setItem('is_admin', data.is_admin ? 'true' : 'false');
            navigate('/dashboard');
        } catch (err: unknown) {
            setError(err instanceof Error ? err.message : 'Login failed');
        } finally {
            setLoading(false);
        }
    };

    const handleResendVerification = async () => {
        setResendLoading(true);
        setResendSuccess(false);

        try {
            const res = await fetch(API_ENDPOINTS.resendVerification, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ email }),
            });

            if (res.ok) {
                setResendSuccess(true);
                setTimeout(() => setResendSuccess(false), 5000);
            }
        } catch {
            // Silent fail
        } finally {
            setResendLoading(false);
        }
    };

    const handleContinueUnverified = () => {
        navigate('/dashboard');
    };

    const handlePasskeyLogin = async () => {
        if (!email) {
            setError('Please enter your email address first');
            return;
        }

        setPasskeyLoading(true);
        setError('');

        try {
            // Step 1: Start authentication
            const startRes = await fetch(API_ENDPOINTS.passkeyLoginStart, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ username: email }),
            });

            if (!startRes.ok) {
                const data = await startRes.json();
                throw new Error(data.error || 'No passkey found for this account');
            }

            const { options } = await startRes.json();

            // Step 2: Get credential from browser
            const credential = await navigator.credentials.get({
                publicKey: {
                    ...options.publicKey,
                    challenge: base64ToBuffer(options.publicKey.challenge),
                    allowCredentials: options.publicKey.allowCredentials?.map((c: { id: string; type: string }) => ({
                        ...c,
                        id: base64ToBuffer(c.id),
                    })),
                },
            }) as PublicKeyCredential;

            if (!credential) {
                throw new Error('Authentication cancelled');
            }

            const response = credential.response as AuthenticatorAssertionResponse;

            // Step 3: Finish authentication
            const finishRes = await fetch(API_ENDPOINTS.passkeyLoginFinish, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    username: email,
                    credential: {
                        id: credential.id,
                        rawId: bufferToBase64(credential.rawId),
                        response: {
                            authenticatorData: bufferToBase64(response.authenticatorData),
                            clientDataJSON: bufferToBase64(response.clientDataJSON),
                            signature: bufferToBase64(response.signature),
                            userHandle: response.userHandle ? bufferToBase64(response.userHandle) : null,
                        },
                        type: credential.type,
                    },
                }),
            });

            const data = await finishRes.json();

            if (!finishRes.ok) {
                throw new Error(data.error || 'Authentication failed');
            }

            localStorage.setItem('token', data.token);
            navigate('/dashboard');
        } catch (err: unknown) {
            logger.error('Passkey login error:', err);
            setError(err instanceof Error ? err.message : 'Passkey authentication failed');
        } finally {
            setPasskeyLoading(false);
        }
    };

    // Check if WebAuthn is supported
    const isPasskeySupported = typeof window !== 'undefined' && 
        window.PublicKeyCredential !== undefined;

    // Show verification needed screen
    if (needsVerification) {
        return (
            <div className="flex min-h-[80vh] items-center justify-center px-4">
                <motion.div 
                    initial={{ opacity: 0, y: 20 }}
                    animate={{ opacity: 1, y: 0 }}
                    className="w-full max-w-md"
                >
                    <div className="bg-white p-8 rounded-2xl shadow-lg border border-slate-100 text-center">
                        <div className="w-16 h-16 bg-amber-100 rounded-full flex items-center justify-center mx-auto mb-4">
                            <Mail className="w-8 h-8 text-amber-600" aria-hidden="true" />
                        </div>

                        <h2 className="text-2xl font-bold text-slate-900 mb-2">Verify your email</h2>
                        <p className="text-slate-600 mb-6">
                            Please verify your email address to access all features.
                            Check your inbox for a verification link.
                        </p>

                        <div className="space-y-3">
                            <button
                                onClick={handleResendVerification}
                                disabled={resendLoading || resendSuccess}
                                className="w-full py-3 bg-primary-600 hover:bg-primary-700 text-white rounded-lg font-medium transition-colors disabled:opacity-50 flex items-center justify-center gap-2"
                            >
                                {resendLoading ? (
                                    <Loader2 className="w-4 h-4 animate-spin" />
                                ) : resendSuccess ? (
                                    <>
                                        <CheckCircle className="w-4 h-4" />
                                        Email sent!
                                    </>
                                ) : (
                                    <>
                                        <Send className="w-4 h-4" />
                                        Resend verification email
                                    </>
                                )}
                            </button>

                            <button
                                onClick={handleContinueUnverified}
                                className="w-full py-3 bg-slate-100 hover:bg-slate-200 text-slate-700 rounded-lg font-medium transition-colors"
                            >
                                Continue to Dashboard
                            </button>
                        </div>

                        <p className="text-xs text-slate-500 mt-4">
                            Some features may be limited until you verify your email.
                        </p>
                    </div>
                </motion.div>
            </div>
        );
    }

    return (
        <div className="flex min-h-[80vh] items-center justify-center px-4">
            <motion.div 
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
                className="w-full max-w-md"
            >
                <div className="bg-white p-8 rounded-2xl shadow-lg border border-slate-100">
                    <div className="text-center mb-8">
                        <h2 className="text-3xl font-bold tracking-tight text-slate-900">Welcome back</h2>
                        <p className="mt-2 text-sm text-slate-600">
                            Don't have an account?{' '}
                            <Link to="/register" className="font-medium text-primary-600 hover:text-primary-500">
                                Sign up
                            </Link>
                        </p>
                    </div>

                    {/* Passkey Login */}
                    {isPasskeySupported && showPasskeyOption && (
                        <div className="mb-6">
                            <button
                                type="button"
                                onClick={handlePasskeyLogin}
                                disabled={passkeyLoading || !email}
                                aria-label="Sign in with passkey"
                                className="w-full flex items-center justify-center gap-3 px-4 py-3 border-2 border-slate-200 rounded-xl font-medium text-slate-700 hover:bg-slate-50 hover:border-slate-300 transition-all disabled:opacity-50 disabled:cursor-not-allowed"
                            >
                                {passkeyLoading ? (
                                    <Loader2 className="h-5 w-5 animate-spin" aria-hidden="true" />
                                ) : (
                                    <Fingerprint className="h-5 w-5 text-primary-600" aria-hidden="true" />
                                )}
                                Sign in with Passkey
                            </button>
                            <p className="text-xs text-slate-500 text-center mt-2">
                                Enter your email first, then click to use your passkey
                            </p>
                        </div>
                    )}

                    {isPasskeySupported && showPasskeyOption && (
                        <div className="relative mb-6">
                            <div className="absolute inset-0 flex items-center">
                                <div className="w-full border-t border-slate-200" />
                            </div>
                            <div className="relative flex justify-center text-sm">
                                <span className="px-2 bg-white text-slate-500">or continue with email</span>
                            </div>
                        </div>
                    )}

                    <form onSubmit={handleSubmit} className="space-y-5">
                        <div>
                            <label htmlFor="email" className="block text-sm font-medium text-slate-700 mb-1">
                                Email address
                            </label>
                            <div className="relative">
                                <Mail className="absolute left-3 top-1/2 -translate-y-1/2 h-5 w-5 text-slate-400" aria-hidden="true" />
                                <input
                                    id="email"
                                    name="email"
                                    type="email"
                                    required
                                    aria-required="true"
                                    autoComplete="email"
                                    className="block w-full pl-10 pr-3 py-2.5 rounded-lg border border-slate-300 bg-white text-slate-900 placeholder:text-slate-400 shadow-sm focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                                    placeholder="you@example.com"
                                    value={email}
                                    onChange={(e) => setEmail(e.target.value)}
                                />
                            </div>
                        </div>
                        <div>
                            <div className="flex items-center justify-between mb-1">
                                <label htmlFor="password" className="block text-sm font-medium text-slate-700">
                                    Password
                                </label>
                                <Link 
                                    to="/forgot-password" 
                                    className="text-sm font-medium text-primary-600 hover:text-primary-500"
                                >
                                    Forgot password?
                                </Link>
                            </div>
                            <input
                                id="password"
                                name="password"
                                type="password"
                                required
                                aria-required="true"
                                autoComplete="current-password"
                                className="block w-full px-3 py-2.5 rounded-lg border border-slate-300 bg-white text-slate-900 placeholder:text-slate-400 shadow-sm focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                                placeholder="••••••••"
                                value={password}
                                onChange={(e) => setPassword(e.target.value)}
                            />
                        </div>

                        {error && (
                            <motion.div 
                                initial={{ opacity: 0, y: -10 }}
                                animate={{ opacity: 1, y: 0 }}
                                className="text-sm text-red-600 bg-red-50 p-3 rounded-lg"
                                role="alert"
                                aria-live="polite"
                            >
                                {error}
                            </motion.div>
                        )}

                        <button
                            type="submit"
                            disabled={loading}
                            className="flex w-full justify-center rounded-lg bg-primary-600 px-4 py-2.5 text-sm font-semibold text-white shadow-sm hover:bg-primary-500 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary-600 disabled:opacity-70 transition-colors"
                        >
                            {loading ? <Loader2 className="h-5 w-5 animate-spin" aria-hidden="true" /> : 'Sign in'}
                        </button>
                    </form>
                </div>
            </motion.div>
        </div>
    );
}

// Helper functions for WebAuthn
function base64ToBuffer(base64: string): ArrayBuffer {
    const base64Standard = base64.replace(/-/g, '+').replace(/_/g, '/');
    const padding = '='.repeat((4 - base64Standard.length % 4) % 4);
    const binary = atob(base64Standard + padding);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) {
        bytes[i] = binary.charCodeAt(i);
    }
    return bytes.buffer;
}

function bufferToBase64(buffer: ArrayBuffer): string {
    const bytes = new Uint8Array(buffer);
    let binary = '';
    for (let i = 0; i < bytes.length; i++) {
        binary += String.fromCharCode(bytes[i]);
    }
    return btoa(binary).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}
