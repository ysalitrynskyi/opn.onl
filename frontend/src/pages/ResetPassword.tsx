import { useState } from 'react';
import { useSearchParams, Link, useNavigate } from 'react-router-dom';
import { motion } from 'framer-motion';
import { Lock, CheckCircle, XCircle, Eye, EyeOff } from 'lucide-react';
import { Helmet } from 'react-helmet-async';
import { API_ENDPOINTS } from '../config/api';

export default function ResetPassword() {
  const [searchParams] = useSearchParams();
  const navigate = useNavigate();
  const token = searchParams.get('token');

  const [password, setPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [showPassword, setShowPassword] = useState(false);
  const [status, setStatus] = useState<'idle' | 'loading' | 'success' | 'error'>('idle');
  const [message, setMessage] = useState('');

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (password !== confirmPassword) {
      setStatus('error');
      setMessage('Passwords do not match');
      return;
    }

    if (password.length < 8) {
      setStatus('error');
      setMessage('Password must be at least 8 characters');
      return;
    }

    setStatus('loading');

    try {
      const response = await fetch(API_ENDPOINTS.resetPassword, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ token, password }),
      });

      if (response.ok) {
        setStatus('success');
        setMessage('Your password has been reset successfully!');
        setTimeout(() => navigate('/login'), 3000);
      } else {
        const data = await response.json();
        setStatus('error');
        setMessage(data.error || 'Failed to reset password. The link may have expired.');
      }
    } catch {
      setStatus('error');
      setMessage('An error occurred. Please try again later.');
    }
  };

  if (!token) {
    return (
      <>
        <Helmet>
          <title>Reset Password - opn.onl</title>
          <meta name="robots" content="noindex, nofollow" />
        </Helmet>

        <div className="min-h-[80vh] flex items-center justify-center py-12 px-4">
          <div className="max-w-md w-full rounded-2xl border border-line bg-surface p-8 text-center shadow-card">
            <XCircle className="w-16 h-16 text-danger mx-auto mb-4" aria-hidden="true" />
            <h1 className="font-display text-2xl font-bold text-ink mb-2">Invalid Link</h1>
            <p className="text-muted mb-6">This password reset link is invalid or has expired.</p>
            <Link
              to="/forgot-password"
              className="inline-flex items-center justify-center px-6 py-3 bg-primary-600 hover:bg-primary-700 text-white rounded-xl font-semibold transition-colors"
            >
              Request New Link
            </Link>
          </div>
        </div>
      </>
    );
  }

  return (
    <>
      <Helmet>
        <title>Reset Password - opn.onl</title>
        <meta name="robots" content="noindex, nofollow" />
      </Helmet>

      <div className="min-h-[80vh] flex items-center justify-center py-12 px-4">
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          className="max-w-md w-full"
        >
          <div className="rounded-2xl border border-line bg-surface p-8 shadow-card">
            {status === 'success' ? (
              <div className="text-center">
                <CheckCircle className="w-16 h-16 text-success mx-auto mb-4" aria-hidden="true" />
                <h1 className="font-display text-2xl font-bold text-ink mb-2">Password Reset!</h1>
                <p className="text-muted mb-2">{message}</p>
                <p className="text-sm text-muted">Redirecting to login...</p>
              </div>
            ) : (
              <>
                <div className="text-center mb-8">
                  <div className="w-16 h-16 bg-primary-50 rounded-full flex items-center justify-center mx-auto mb-4">
                    <Lock className="w-8 h-8 text-primary-600" aria-hidden="true" />
                  </div>
                  <h1 className="font-display text-2xl font-bold text-ink mb-2">Reset your password</h1>
                  <p className="text-muted">Enter your new password below.</p>
                </div>

                <form onSubmit={handleSubmit} className="space-y-4">
                  <div>
                    <label htmlFor="password" className="block text-sm font-medium text-ink mb-2">
                      New Password
                    </label>
                    <div className="relative">
                      <input
                        id="password"
                        type={showPassword ? 'text' : 'password'}
                        value={password}
                        onChange={(e) => setPassword(e.target.value)}
                        required
                        minLength={8}
                        className="block w-full rounded-xl border border-line2 bg-white px-4 py-3 text-ink shadow-subtle outline-none transition-colors placeholder:text-muted/70 focus:border-primary-500 pr-12"
                        placeholder="••••••••"
                        aria-required="true"
                        aria-describedby="password-requirements"
                      />
                      <button
                        type="button"
                        onClick={() => setShowPassword(!showPassword)}
                        className="absolute right-3 top-1/2 -translate-y-1/2 text-muted hover:text-ink"
                        aria-label={showPassword ? 'Hide password' : 'Show password'}
                      >
                        {showPassword ? <EyeOff className="w-5 h-5" /> : <Eye className="w-5 h-5" />}
                      </button>
                    </div>
                    <p id="password-requirements" className="text-xs text-muted mt-1">
                      Must be at least 8 characters
                    </p>
                  </div>

                  <div>
                    <label htmlFor="confirmPassword" className="block text-sm font-medium text-ink mb-2">
                      Confirm Password
                    </label>
                    <input
                      id="confirmPassword"
                      type={showPassword ? 'text' : 'password'}
                      value={confirmPassword}
                      onChange={(e) => setConfirmPassword(e.target.value)}
                      required
                      className="block w-full rounded-xl border border-line2 bg-white px-4 py-3 text-ink shadow-subtle outline-none transition-colors placeholder:text-muted/70 focus:border-primary-500"
                      placeholder="••••••••"
                      aria-required="true"
                    />
                  </div>

                  {status === 'error' && (
                    <p className="text-danger text-sm" role="alert">{message}</p>
                  )}

                  <button
                    type="submit"
                    disabled={status === 'loading'}
                    className="w-full py-3 bg-primary-600 hover:bg-primary-700 disabled:opacity-50 disabled:cursor-not-allowed text-white rounded-xl font-semibold transition-colors"
                  >
                    {status === 'loading' ? 'Resetting...' : 'Reset Password'}
                  </button>
                </form>
              </>
            )}
          </div>
        </motion.div>
      </div>
    </>
  );
}

