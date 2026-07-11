import { useEffect, useState } from 'react';

// GA_ID is injected into index.html at runtime; loadAnalytics is defined there and
// actually loads gtag. This component only decides *whether* to call it.
declare global {
    interface Window {
        GA_ID?: string;
        loadAnalytics?: () => void;
    }
}

const CONSENT_KEY = 'analytics_consent';

/** Analytics is only relevant when an operator has configured a GA measurement id. */
function analyticsConfigured(): boolean {
    return (window.GA_ID || '').startsWith('G-');
}

/** Honor an explicit browser opt-out (Do-Not-Track / Global Privacy Control). */
function optedOutByBrowser(): boolean {
    const nav = navigator as unknown as { doNotTrack?: string; msDoNotTrack?: string; globalPrivacyControl?: boolean };
    const win = window as unknown as { doNotTrack?: string };
    return (
        nav.doNotTrack === '1' ||
        win.doNotTrack === '1' ||
        nav.msDoNotTrack === '1' ||
        nav.globalPrivacyControl === true
    );
}

/**
 * GDPR-style consent gate for Google Analytics. GA does not load until the
 * visitor accepts; the choice is remembered. The banner never appears when GA is
 * not configured or the browser already signals a privacy opt-out. Backing the
 * DNT/GPC default already shipped, this closes the "GA fires with no consent"
 * gap for EU-facing deployments.
 */
export default function ConsentBanner() {
    // Decide visibility once, from client-only signals (GA config, DNT/GPC, stored
    // choice). Computed in the initializer rather than an effect so there is no
    // setState-in-effect.
    const [visible, setVisible] = useState(
        () => analyticsConfigured() && !optedOutByBrowser() && !localStorage.getItem(CONSENT_KEY),
    );

    useEffect(() => {
        // Side effect only: if the visitor already consented, load analytics.
        if (analyticsConfigured() && !optedOutByBrowser() && localStorage.getItem(CONSENT_KEY) === 'granted') {
            window.loadAnalytics?.();
        }
    }, []);

    if (!visible) return null;

    const accept = () => {
        localStorage.setItem(CONSENT_KEY, 'granted');
        window.loadAnalytics?.();
        setVisible(false);
    };
    const decline = () => {
        localStorage.setItem(CONSENT_KEY, 'denied');
        setVisible(false);
    };

    return (
        <div
            role="dialog"
            aria-label="Analytics consent"
            className="fixed bottom-0 inset-x-0 z-50 border-t border-slate-200 bg-white/95 backdrop-blur"
        >
            <div className="mx-auto max-w-3xl px-4 py-3 flex flex-col sm:flex-row sm:items-center gap-3">
                <p className="text-sm text-slate-600 flex-1">
                    We use privacy-friendly analytics to understand how the site is used.
                    Nothing is collected until you agree. See our{' '}
                    <a href="/privacy" className="text-primary-600 underline">privacy policy</a>.
                </p>
                <div className="flex gap-2 shrink-0">
                    <button
                        type="button"
                        onClick={decline}
                        className="px-4 py-2 text-sm font-medium rounded-lg border border-slate-300 text-slate-700 hover:bg-slate-50"
                    >
                        Decline
                    </button>
                    <button
                        type="button"
                        onClick={accept}
                        className="px-4 py-2 text-sm font-medium rounded-lg bg-primary-600 text-white hover:bg-primary-700"
                    >
                        Accept
                    </button>
                </div>
            </div>
        </div>
    );
}
