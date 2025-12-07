import { useState } from 'react';
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
            <div className="flex min-h-[80vh] items-center justify-center px-4">
                <motion.div 
                    initial={{ opacity: 0, y: 20 }}
                    animate={{ opacity: 1, y: 0 }}
                    className="w-full max-w-md"
                >
                    <div className="bg-white p-8 rounded-2xl shadow-lg border border-slate-100 text-center">
                        <motion.div
                            initial={{ scale: 0 }}
                            animate={{ scale: 1 }}
                            transition={{ type: 'spring', delay: 0.1 }}
                            className="w-20 h-20 bg-emerald-100 rounded-full flex items-center justify-center mx-auto mb-6"
                        >
                            <Mail className="w-10 h-10 text-emerald-600" aria-hidden="true" />
                        </motion.div>

                        <h2 className="text-2xl font-bold text-slate-900 mb-2">Check your email</h2>
                        <p className="text-slate-600 mb-6">
                            We've sent a verification link to<br />
                            <strong className="text-slate-900">{email}</strong>
                        </p>

                        <div className="bg-slate-50 rounded-xl p-4 mb-6 text-left">
                            <h3 className="font-medium text-slate-900 mb-2">Next steps:</h3>
                            <ol className="text-sm text-slate-600 space-y-2">
                                <li className="flex gap-2">
                                    <span className="flex-shrink-0 w-5 h-5 bg-primary-100 text-primary-600 rounded-full flex items-center justify-center text-xs font-bold">1</span>
                                    Open your email inbox
                                </li>
                                <li className="flex gap-2">
                                    <span className="flex-shrink-0 w-5 h-5 bg-primary-100 text-primary-600 rounded-full flex items-center justify-center text-xs font-bold">2</span>
                                    Click the verification link
                                </li>
                                <li className="flex gap-2">
                                    <span className="flex-shrink-0 w-5 h-5 bg-primary-100 text-primary-600 rounded-full flex items-center justify-center text-xs font-bold">3</span>
                                    Start shortening links!
                                </li>
                            </ol>
                        </div>

                        <div className="space-y-3">
                            <button
                                onClick={handleContinueToDashboard}
                                className="w-full py-3 bg-primary-600 hover:bg-primary-700 text-white rounded-lg font-medium transition-colors"
                            >
                                Continue to Dashboard
                            </button>

                            <button
                                onClick={handleResendVerification}
                                disabled={resendLoading || resendSuccess}
                                className="w-full py-3 bg-slate-100 hover:bg-slate-200 text-slate-700 rounded-lg font-medium transition-colors disabled:opacity-50 flex items-center justify-center gap-2"
                            >
                                {resendLoading ? (
                                    <Loader2 className="w-4 h-4 animate-spin" />
                                ) : resendSuccess ? (
                                    <>
                                        <CheckCircle className="w-4 h-4 text-emerald-600" />
                                        Email sent!
                                    </>
                                ) : (
                                    <>
                                        <Send className="w-4 h-4" />
                                        Resend verification email
                                    </>
                                )}
                            </button>
                        </div>

                        <p className="text-xs text-slate-500 mt-4">
                            Didn't receive the email? Check your spam folder or try resending.
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
                        <h2 className="text-3xl font-bold tracking-tight text-slate-900">Create an account</h2>
                        <p className="mt-2 text-sm text-slate-600">
                            Already have an account?{' '}
                            <Link to="/login" className="font-medium text-primary-600 hover:text-primary-500">
                                Log in
                            </Link>
                        </p>
                    </div>

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
                            <label htmlFor="password" className="block text-sm font-medium text-slate-700 mb-1">
                                Password
                            </label>
                            <div className="relative">
                                <Lock className="absolute left-3 top-1/2 -translate-y-1/2 h-5 w-5 text-slate-400" aria-hidden="true" />
                                <input
                                    id="password"
                                    name="password"
                                    type="password"
                                    required
                                    aria-required="true"
                                    autoComplete="new-password"
                                    minLength={8}
                                    className="block w-full pl-10 pr-3 py-2.5 rounded-lg border border-slate-300 bg-white text-slate-900 placeholder:text-slate-400 shadow-sm focus:border-primary-500 focus:ring-1 focus:ring-primary-500"
                                    placeholder="••••••••"
                                    value={password}
                                    onChange={(e) => setPassword(e.target.value)}
                                />
                            </div>
                            
                            {/* Password requirements */}
                            {password && (
                                <motion.div 
                                    initial={{ opacity: 0, height: 0 }}
                                    animate={{ opacity: 1, height: 'auto' }}
                                    className="mt-3 space-y-2"
                                >
                                    {passwordRequirements.map((req, i) => (
                                        <div key={i} className="flex items-center gap-2 text-sm">
                                            <div className={`h-4 w-4 rounded-full flex items-center justify-center ${
                                                req.test(password) ? 'bg-emerald-100 text-emerald-600' : 'bg-slate-100 text-slate-400'
                                            }`}>
                                                <Check className="h-3 w-3" aria-hidden="true" />
                                            </div>
                                            <span className={req.test(password) ? 'text-emerald-600' : 'text-slate-500'}>
                                                {req.label}
                                            </span>
                                        </div>
                                    ))}
                                </motion.div>
                            )}
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
                            disabled={loading || !allRequirementsMet}
                            className="flex w-full justify-center rounded-lg bg-primary-600 px-4 py-2.5 text-sm font-semibold text-white shadow-sm hover:bg-primary-500 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary-600 disabled:opacity-70 transition-colors"
                        >
                            {loading ? <Loader2 className="h-5 w-5 animate-spin" aria-hidden="true" /> : 'Create account'}
                        </button>

                        <p className="text-xs text-center text-slate-500">
                            By creating an account, you agree to our{' '}
                            <Link to="/terms" className="text-primary-600 hover:underline">Terms of Service</Link>
                            {' '}and{' '}
                            <Link to="/privacy" className="text-primary-600 hover:underline">Privacy Policy</Link>.
                        </p>
                    </form>
                </div>

                <motion.div
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    transition={{ delay: 0.3 }}
                    className="mt-6 bg-slate-50 rounded-xl p-4 text-center"
                >
                    <p className="text-sm text-slate-600">
                        🔐 After registering, you can add a <strong>Passkey</strong> in Settings for passwordless login!
                    </p>
                </motion.div>
            </motion.div>
        </div>
    );
}
