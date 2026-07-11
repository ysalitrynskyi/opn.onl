import { useEffect, useState } from 'react';
import { useParams, useNavigate, Link as RouterLink } from 'react-router-dom';
import { ExternalLink, ShieldCheck, ShieldAlert, ShieldQuestion, ArrowRight } from 'lucide-react';
import { API_ENDPOINTS } from '../config/api';

interface Interstitial {
    domain: string;
    original_url: string;
    verdict: string;
    source: string;
}

const API_URL = import.meta.env.VITE_API_URL || 'http://localhost:3000';

/**
 * Redirect component - handles short link redirects by checking if a code exists
 * and either redirecting to the target URL or showing 404. When a link opts into
 * the safe-link interstitial (and the instance has it enabled), a "you're leaving
 * to X" confirmation is shown before proceeding to the backend redirect.
 */
export default function Redirect() {
    const { code } = useParams<{ code: string }>();
    const navigate = useNavigate();
    const [checking, setChecking] = useState(true);
    const [error, setError] = useState<string | null>(null);
    const [interstitial, setInterstitial] = useState<Interstitial | null>(null);

    useEffect(() => {
        if (!code) {
            navigate('/404', { replace: true });
            return;
        }

        const checkAndRedirect = async () => {
            try {
                const previewRes = await fetch(`${API_ENDPOINTS.preview(code)}`);

                if (previewRes.ok) {
                    const data = await previewRes.json();

                    if (data.has_password) {
                        navigate(`/password/${code}`, { replace: true });
                        return;
                    }

                    if (data.is_expired) {
                        setError('This link has expired');
                        setChecking(false);
                        return;
                    }

                    // Opted-in + instance-enabled → show the safety interstitial
                    // instead of redirecting straight away.
                    if (data.interstitial_enabled && data.safe_link_interstitial) {
                        setInterstitial({
                            domain: data.domain,
                            original_url: data.original_url,
                            verdict: data.reputation?.verdict ?? 'unknown',
                            source: data.reputation?.source ?? 'internal_blocklist',
                        });
                        setChecking(false);
                        return;
                    }

                    // Redirect via the backend (records the click, then 302s).
                    window.location.href = `${API_URL}/${code}`;
                } else if (previewRes.status === 404) {
                    navigate('/404', { replace: true });
                } else {
                    setError('Failed to load link');
                    setChecking(false);
                }
            } catch (err) {
                console.error('Redirect error:', err);
                // On network error, still try to redirect via API (might work)
                window.location.href = `${API_URL}/${code}`;
            }
        };

        checkAndRedirect();
    }, [code, navigate]);

    if (error) {
        return (
            <div className="min-h-[60vh] flex items-center justify-center">
                <div className="text-center">
                    <h1 className="text-2xl font-bold text-slate-800 mb-2">Link Unavailable</h1>
                    <p className="text-slate-600">{error}</p>
                </div>
            </div>
        );
    }

    if (interstitial) {
        const verdict = interstitial.verdict;
        const safe = verdict === 'safe';
        const danger = verdict === 'malicious' || verdict === 'suspicious';
        const VerdictIcon = safe ? ShieldCheck : danger ? ShieldAlert : ShieldQuestion;
        const verdictTone = safe
            ? 'bg-emerald-50 border-emerald-200 text-emerald-800'
            : danger
                ? 'bg-red-50 border-red-200 text-red-800'
                : 'bg-slate-50 border-slate-200 text-slate-700';
        const verdictLabel = safe
            ? "Looks safe — we have nothing bad on record for this destination."
            : danger
                ? 'Caution: this destination is flagged on our blocklist.'
                : "We couldn't verify this destination. Proceed only if you trust it.";

        return (
            <div className="min-h-[60vh] flex items-center justify-center px-4 py-12">
                <div className="max-w-lg w-full">
                    <div className="bg-white rounded-2xl shadow-xl border border-slate-200 overflow-hidden">
                        <div className="bg-gradient-to-r from-primary-600 to-primary-700 px-6 py-5">
                            <h1 className="text-xl font-bold text-white">You're leaving opn.onl</h1>
                            <p className="text-primary-100 text-sm mt-1">Review the destination before continuing</p>
                        </div>
                        <div className="p-6 space-y-5">
                            <div className="flex items-center gap-3 p-4 bg-slate-50 rounded-xl">
                                {/* Domain initial instead of a third-party favicon service:
                                    fetching favicons from Google leaked the destination
                                    domain and the visitor's IP to a third party. */}
                                <div className="h-10 w-10 bg-white rounded-lg border border-slate-200 flex items-center justify-center flex-shrink-0">
                                    <span className="text-sm font-bold text-slate-500 uppercase">
                                        {interstitial.domain?.charAt(0) || '?'}
                                    </span>
                                </div>
                                <div className="min-w-0 flex-1">
                                    <p className="font-medium text-slate-900 truncate">{interstitial.domain}</p>
                                    <p className="text-sm text-slate-500 truncate">{interstitial.original_url}</p>
                                </div>
                                <ExternalLink className="h-4 w-4 text-slate-400 flex-shrink-0" />
                            </div>

                            <div className={`flex items-start gap-2.5 p-3 rounded-lg border text-sm ${verdictTone}`}>
                                <VerdictIcon className="h-4 w-4 flex-shrink-0 mt-0.5" />
                                <span>{verdictLabel}</span>
                            </div>

                            <div className="flex gap-3 pt-1">
                                <RouterLink
                                    to="/"
                                    className="flex-1 text-center rounded-xl border border-slate-200 px-6 py-3 font-medium text-slate-600 hover:bg-slate-50 transition-colors"
                                >
                                    Cancel
                                </RouterLink>
                                <a
                                    href={`${API_URL}/${code}?confirm=1`}
                                    className="flex-1 flex items-center justify-center gap-2 bg-primary-600 text-white px-6 py-3 rounded-xl font-medium hover:bg-primary-700 transition-colors"
                                >
                                    Continue
                                    <ArrowRight className="h-4 w-4" />
                                </a>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
        );
    }

    if (checking) {
        return (
            <div className="min-h-[60vh] flex items-center justify-center">
                <div className="flex flex-col items-center gap-4">
                    <div className="w-8 h-8 border-4 border-primary-200 border-t-primary-600 rounded-full animate-spin" />
                    <p className="text-slate-500 text-sm">Redirecting...</p>
                </div>
            </div>
        );
    }

    return null;
}
