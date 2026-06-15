import { useState, useEffect } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { Loader2, Mail, Lock, Check, CheckCircle, Send } from 'lucide-react';
import { motion } from 'framer-motion';
import { API_ENDPOINTS } from '../config/api';

const passwordRequirements = [
    { label: 'At least 8 characters', test: (p: string) => p.length >= 8 },
];

export default function Register() {
    const [email, setEmail] = useState('');
    const [password, setPassword] = useState('');
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState('');
    const [registrationComplete, setRegistrationComplete] = useState(false);
    const [resendLoading, setResendLoading] = useState(false);
    const [resendSuccess, setResendSuccess] = useState(false);
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

        try {
            const res = await fetch(API_ENDPOINTS.register, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ email, password }),
            });

            const data = await res.json();

            if (!res.ok) throw new Error(data.error || 'Registration failed');

            // Store token but show verification message
            localStorage.setItem('token', data.token);
            localStorage.setItem('is_admin', data.is_admin ? 'true' : 'false');
            
            // Check if email verification is required
            if (data.email_verified === false) {
                setRegistrationComplete(true);
            } else {
                // If email is already verified (e.g., SMTP not configured), go to dashboard
                navigate('/dashboard');
            }
        } catch (err: unknown) {
            setError(err instanceof Error ? err.message : 'Registration failed');
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
            // Silent fail - user can try again
        } finally {
            setResendLoading(false);
        }
    };

    const handleContinueToDashboard = () => {
        navigate('/dashboard');
    };

    const allRequirementsMet = passwordRequirements.every(req => req.test(password));

    // Show verification email sent screen
    if (registrationComplete) {
        return (
            <div className="flex min-h-[80vh] items-center justify-center px-4 py-16">
                <motion.div
                    initial={{ opacity: 0, y: 16 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ duration: 0.5, ease: [0.16, 1, 0.3, 1] }}
                    className="w-full max-w-md"
                >
                    <div className="rounded-2xl border border-line bg-surface p-8 shadow-card text-center">
                        <div className="mx-auto mb-5 flex h-14 w-14 items-center justify-center rounded-2xl bg-success/10">
                            <Mail className="h-7 w-7 text-success" aria-hidden="true" />
                        </div>
                        <h1 className="font-display text-2xl font-bold text-ink">Check your email</h1>
                        <p className="mt-2 text-muted">
                            We sent a verification link to<br />
                            <span className="font-mono text-sm text-ink">{email}</span>
                        </p>

                        <ol className="mt-6 space-y-3 rounded-xl border border-line bg-paper p-5 text-left">
                            {['Open your inbox', 'Click the verification link', 'Start shortening links'].map((step, i) => (
                                <li key={i} className="flex items-center gap-3 text-sm text-muted">
                                    <span className="font-mono text-xs font-semibold text-primary-600">0{i + 1}</span>
                                    {step}
                                </li>
                            ))}
                        </ol>

                        <div className="mt-6 space-y-3">
                            <button onClick={handleContinueToDashboard} className="w-full rounded-xl bg-primary-600 py-3 font-semibold text-white transition-colors hover:bg-primary-700">
                                Continue to dashboard
                            </button>
                            <button
                                onClick={handleResendVerification}
                                disabled={resendLoading || resendSuccess}
                                className="flex w-full items-center justify-center gap-2 rounded-xl border border-line2 bg-surface py-3 font-medium text-ink transition-colors hover:border-ink/30 disabled:opacity-50"
                            >
                                {resendLoading ? <Loader2 className="h-4 w-4 animate-spin" />
                                    : resendSuccess ? <><CheckCircle className="h-4 w-4 text-success" /> Email sent</>
                                    : <><Send className="h-4 w-4" /> Resend email</>}
                            </button>
                        </div>
                        <p className="mt-4 text-xs text-faint">No email? Check spam, or resend above.</p>
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
                    <p className="font-mono text-xs uppercase tracking-[0.2em] text-primary-600">Free forever</p>
                    <h1 className="mt-3 font-display text-3xl font-extrabold tracking-tight text-ink">Create your account</h1>
                    <p className="mt-2 text-sm text-muted">
                        Already have one?{' '}
                        <Link to="/login" className="font-medium text-primary-600 hover:text-primary-700">Log in</Link>
                    </p>
                </div>

                <div className="rounded-2xl border border-line bg-surface p-8 shadow-card">
                    <form onSubmit={handleSubmit} className="space-y-5">
                        <div>
                            <label htmlFor="email" className="mb-1.5 block text-sm font-medium text-ink">Email address</label>
                            <div className="relative">
                                <Mail className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-faint" aria-hidden="true" />
                                <input
                                    id="email" name="email" type="email" required autoComplete="email"
                                    className="block w-full rounded-xl border border-line2 bg-white py-2.5 pl-10 pr-3 text-ink shadow-subtle outline-none transition-colors focus:border-primary-500"
                                    placeholder="you@example.com"
                                    value={email} onChange={(e) => setEmail(e.target.value)}
                                />
                            </div>
                        </div>
                        <div>
                            <label htmlFor="password" className="mb-1.5 block text-sm font-medium text-ink">Password</label>
                            <div className="relative">
                                <Lock className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-faint" aria-hidden="true" />
                                <input
                                    id="password" name="password" type="password" required autoComplete="new-password" minLength={8}
                                    className="block w-full rounded-xl border border-line2 bg-white py-2.5 pl-10 pr-3 text-ink shadow-subtle outline-none transition-colors focus:border-primary-500"
                                    placeholder="••••••••"
                                    value={password} onChange={(e) => setPassword(e.target.value)}
                                />
                            </div>
                            {password && (
                                <div className="mt-3 space-y-2">
                                    {passwordRequirements.map((req, i) => (
                                        <div key={i} className="flex items-center gap-2 text-sm">
                                            <span className={`flex h-4 w-4 items-center justify-center rounded-full ${req.test(password) ? 'bg-success/15 text-success' : 'bg-line text-faint'}`}>
                                                <Check className="h-3 w-3" aria-hidden="true" />
                                            </span>
                                            <span className={req.test(password) ? 'text-success' : 'text-faint'}>{req.label}</span>
                                        </div>
                                    ))}
                                </div>
                            )}
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
                            disabled={loading || !allRequirementsMet}
                            className="flex w-full justify-center rounded-xl bg-primary-600 px-4 py-3 font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-60"
                        >
                            {loading ? <Loader2 className="h-5 w-5 animate-spin" aria-hidden="true" /> : 'Create account'}
                        </button>

                        <p className="text-center text-xs text-faint">
                            By creating an account you agree to our{' '}
                            <Link to="/terms" className="text-muted underline decoration-line2 underline-offset-2 hover:text-ink">Terms</Link>{' '}and{' '}
                            <Link to="/privacy" className="text-muted underline decoration-line2 underline-offset-2 hover:text-ink">Privacy Policy</Link>.
                        </p>
                    </form>
                </div>

                <p className="mt-6 text-center text-sm text-muted">
                    Tip: add a <span className="font-medium text-ink">passkey</span> in Settings for passwordless sign-in.
                </p>
            </motion.div>
        </div>
    );
}
