import { useEffect, useState } from 'react';
import { useSearchParams, Link } from 'react-router-dom';
import { motion } from 'framer-motion';
import { CheckCircle, XCircle, Loader2 } from 'lucide-react';
import { Helmet } from 'react-helmet-async';
import { API_ENDPOINTS } from '../config/api';

export default function VerifyEmail() {
  const [searchParams] = useSearchParams();
  const [status, setStatus] = useState<'loading' | 'success' | 'error'>('loading');
  const [message, setMessage] = useState('');
  const token = searchParams.get('token');

  useEffect(() => {
    if (!token) {
      setStatus('error');
      setMessage('Invalid verification link. No token provided.');
      return;
    }

    const verifyEmail = async () => {
      try {
        const response = await fetch(API_ENDPOINTS.verifyEmail, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ token }),
        });

        if (response.ok) {
          setStatus('success');
          setMessage('Your email has been verified successfully!');
        } else {
          const data = await response.json();
          setStatus('error');
          setMessage(data.error || 'Failed to verify email. The link may have expired.');
        }
      } catch {
        setStatus('error');
        setMessage('An error occurred. Please try again later.');
      }
    };

    verifyEmail();
  }, [token]);

  return (
    <>
      <Helmet>
        <title>Verify Email - opn.onl</title>
        <meta name="robots" content="noindex, nofollow" />
      </Helmet>

      {/*
        * Design tokens only (surface / line / ink / muted / primary), matching the
        * login and password-reset cards. This page used to carry dark-theme classes
        * — a translucent slate-800 card with white text — which rendered as a grey
        * slab with near-illegible body copy on the app's light background.
        */}
      <div className="min-h-[80vh] flex items-center justify-center py-12 px-4">
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.5, ease: [0.16, 1, 0.3, 1] }}
          className="w-full max-w-md rounded-2xl border border-line bg-surface p-8 text-center shadow-card"
        >
          {status === 'loading' && (
            <>
              <div className="mx-auto mb-5 flex h-14 w-14 items-center justify-center rounded-2xl bg-primary-50">
                <Loader2 className="h-7 w-7 animate-spin text-primary-600" aria-hidden="true" />
              </div>
              <h1 className="font-display text-2xl font-bold text-ink">Verifying your email…</h1>
              <p className="mt-2 leading-relaxed text-muted">Please wait while we verify your email address.</p>
            </>
          )}

          {status === 'success' && (
            <>
              <motion.div
                initial={{ scale: 0 }}
                animate={{ scale: 1 }}
                transition={{ type: 'spring', delay: 0.1 }}
                className="mx-auto mb-5 flex h-14 w-14 items-center justify-center rounded-2xl bg-success/10"
              >
                <CheckCircle className="h-7 w-7 text-success" aria-hidden="true" />
              </motion.div>
              <h1 className="font-display text-2xl font-bold text-ink">Email verified</h1>
              <p className="mt-2 leading-relaxed text-muted">{message}</p>
              <Link
                to="/login"
                className="mt-6 flex w-full items-center justify-center rounded-xl bg-primary-600 py-3 font-semibold text-white transition-colors hover:bg-primary-700"
              >
                Continue to login
              </Link>
            </>
          )}

          {status === 'error' && (
            <>
              <motion.div
                initial={{ scale: 0 }}
                animate={{ scale: 1 }}
                transition={{ type: 'spring', delay: 0.1 }}
                className="mx-auto mb-5 flex h-14 w-14 items-center justify-center rounded-2xl bg-danger/10"
              >
                <XCircle className="h-7 w-7 text-danger" aria-hidden="true" />
              </motion.div>
              <h1 className="font-display text-2xl font-bold text-ink">Verification failed</h1>
              <p className="mt-2 leading-relaxed text-muted">{message}</p>
              <div className="mt-6 space-y-3">
                <Link
                  to="/login"
                  className="flex w-full items-center justify-center rounded-xl bg-primary-600 py-3 font-semibold text-white transition-colors hover:bg-primary-700"
                >
                  Go to login
                </Link>
                <Link
                  to="/"
                  className="flex w-full items-center justify-center rounded-xl border border-line2 bg-surface py-3 font-medium text-ink transition-colors hover:border-ink/30"
                >
                  Go to homepage
                </Link>
              </div>
            </>
          )}
        </motion.div>
      </div>
    </>
  );
}

