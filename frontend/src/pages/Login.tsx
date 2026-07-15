import { useState, useEffect } from 'react';
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

    // Redirect to dashboard if already logged in
    useEffect(() => {
        const token = localStorage.getItem('token');
        if (token) {
            navigate('/dashboard', { replace: true });
        }
    }, [navigate]);

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
            localStorage.setItem('is_admin', data.is_admin ? 'true' : 'false');

            if (data.email_verified === false) {
                setNeedsVerification(true);
                return;
            }

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
            <div className="flex min-h-[80vh] items-center justify-center px-4 py-16">
                <motion.div
                    initial={{ opacity: 0, y: 16 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ duration: 0.5, ease: [0.16, 1, 0.3, 1] }}
                    className="w-full max-w-md"
                >
                    <div className="rounded-2xl border border-line bg-surface p-8 shadow-card text-center">
                        <div className="mx-auto mb-5 flex h-14 w-14 items-center justify-center rounded-2xl bg-warning/10">
                            <Mail className="h-7 w-7 text-warning" aria-hidden="true" />
                        </div>
                        <h1 className="font-display text-2xl font-bold text-ink">Verify your email</h1>
                        <p className="mt-2 text-muted leading-relaxed">
                            Check your inbox for a verification link to unlock every feature.
                        </p>
                        <div className="mt-6 space-y-3">
                            <button
                                onClick={handleResendVerification}
                                disabled={resendLoading || resendSuccess}
                                className="flex w-full items-center justify-center gap-2 rounded-xl bg-primary-600 py-3 font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-50"
                            >
                                {resendLoading ? <Loader2 className="h-4 w-4 animate-spin" />
                                    : resendSuccess ? <><CheckCircle className="h-4 w-4" /> Email sent</>
                                    : <><Send className="h-4 w-4" /> Resend verification email</>}
                            </button>
                            <button
                                onClick={handleContinueUnverified}
                                className="w-full rounded-xl border border-line2 bg-surface py-3 font-medium text-ink transition-colors hover:border-ink/30"
                            >
                                Continue to dashboard
                            </button>
                        </div>
                        <p className="mt-4 text-xs text-faint">Some features stay limited until you verify.</p>
                    </div>
                </motion.div>
            </div>
        );
    }

    return (
        <div className="flex min-h-[80vh] items-center justify-center px-4 py-16">
            <motion.div
                initial={{ opacity: 0, y: 16 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.5, ease: [0.16, 1, 0.3, 1] }}
                className="w-full max-w-md"
            >
                <div className="mb-8 text-center">
                    <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary-600">Welcome back</p>
                    <h1 className="mt-3 font-display text-3xl font-extrabold tracking-tight text-ink">Sign in to opn.onl</h1>
                    <p className="mt-2 text-sm text-muted">
                        New here?{' '}
                        <Link to="/register" className="font-medium text-primary-600 hover:text-primary-700">Create an account</Link>
                    </p>
                </div>

                <div className="rounded-2xl border border-line bg-surface p-8 shadow-card">
                    {isPasskeySupported && showPasskeyOption && (
                        <>
                            <button
                                type="button"
                                onClick={handlePasskeyLogin}
                                disabled={passkeyLoading || !email}
                                aria-label="Sign in with passkey"
                                className="flex w-full items-center justify-center gap-3 rounded-xl border border-line2 bg-surface px-4 py-3 font-medium text-ink transition-colors hover:border-primary-300 hover:bg-primary-50/50 disabled:opacity-50 disabled:cursor-not-allowed"
                            >
                                {passkeyLoading
                                    ? <Loader2 className="h-5 w-5 animate-spin" aria-hidden="true" />
                                    : <Fingerprint className="h-5 w-5 text-primary-600" aria-hidden="true" />}
                                Continue with passkey
                            </button>
                            <p className="mt-2 text-center text-xs text-faint">Enter your email first, then use your passkey.</p>

                            <div className="relative my-6">
                                <div className="absolute inset-0 flex items-center"><div className="w-full border-t border-line" /></div>
                                <div className="relative flex justify-center"><span className="bg-surface px-3 font-mono text-xs uppercase tracking-wider text-faint">or email</span></div>
                            </div>
                        </>
                    )}

                    <form onSubmit={handleSubmit} className="space-y-5">
                        <div>
                            <label htmlFor="email" className="mb-1.5 block text-sm font-medium text-ink">Email address</label>
                            <div className="relative">
                                <Mail className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-faint" aria-hidden="true" />
                                <input
                                    id="email" name="email" type="email" required autoComplete="email"
                                    className="block w-full rounded-xl border border-line2 bg-white py-2.5 pl-10 pr-3 text-ink shadow-subtle outline-none transition-colors focus:border-primary-500"
                                    placeholder="you@example.com"
                                    value={email}
                                    onChange={(e) => setEmail(e.target.value)}
                                />
                            </div>
                        </div>
                        <div>
                            <div className="mb-1.5 flex items-center justify-between">
                                <label htmlFor="password" className="block text-sm font-medium text-ink">Password</label>
                                <Link to="/forgot-password" className="text-sm font-medium text-primary-600 hover:text-primary-700">Forgot?</Link>
                            </div>
                            <input
                                id="password" name="password" type="password" required autoComplete="current-password"
                                className="block w-full rounded-xl border border-line2 bg-white px-3 py-2.5 text-ink shadow-subtle outline-none transition-colors focus:border-primary-500"
                                placeholder="••••••••"
                                value={password}
                                onChange={(e) => setPassword(e.target.value)}
                            />
                        </div>

                        {error && (
                            <motion.div
                                initial={{ opacity: 0, y: -8 }} animate={{ opacity: 1, y: 0 }}
                                role="alert" aria-live="polite"
                                className="rounded-xl border border-danger/30 bg-danger/5 px-3 py-2.5 text-sm text-danger"
                            >
                                {error}
                            </motion.div>
                        )}

                        <button
                            type="submit"
                            disabled={loading}
                            className="flex w-full justify-center rounded-xl bg-primary-600 px-4 py-3 font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-70"
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
