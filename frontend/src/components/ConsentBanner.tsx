import { useEffect, useState } from 'react';

// GA_ID and GA_CONSENT_MODE are injected into index.html at runtime; loadAnalytics
// and disableAnalytics are defined there and do the actual gtag work. This
// component only decides *whether* to call them.
declare global {
    interface Window {
        GA_ID?: string;
        GA_CONSENT_MODE?: string;
        loadAnalytics?: () => void;
        disableAnalytics?: () => void;
    }
}

const CONSENT_KEY = 'analytics_consent';

/** Analytics is only relevant when an operator has configured a GA measurement id. */
function analyticsConfigured(): boolean {
    return (window.GA_ID || '').startsWith('G-');
}

/**
 * Opt-out deployments (GA_CONSENT_MODE=opt-out) collect from the first second
 * and stop only on an explicit Decline. Anything else is the opt-in default,
 * where nothing loads before an explicit Accept.
 */
function optOutMode(): boolean {
    return window.GA_CONSENT_MODE === 'opt-out';
}

/** Global Privacy Control — a legally recognized opt-out; honored in both modes. */
function globalPrivacyControl(): boolean {
    const nav = navigator as unknown as { globalPrivacyControl?: boolean };
    return nav.globalPrivacyControl === true;
}

/** Do-Not-Track — honored in opt-in mode only (deprecated, and not an explicit choice). */
function doNotTrack(): boolean {
    const nav = navigator as unknown as { doNotTrack?: string; msDoNotTrack?: string };
    const win = window as unknown as { doNotTrack?: string };
    return nav.doNotTrack === '1' || win.doNotTrack === '1' || nav.msDoNotTrack === '1';
}

function storedChoice(): string | null {
    try {
        return localStorage.getItem(CONSENT_KEY);
    } catch {
        return null;
    }
}

function storeChoice(choice: 'granted' | 'denied'): void {
    try {
        localStorage.setItem(CONSENT_KEY, choice);
    } catch {
        /* storage blocked: the in-page decision below still applies to this visit */
    }
}

/**
 * Consent banner for Google Analytics, in whichever direction the deployment is
 * configured for. In the default opt-in mode GA does not load until the visitor
 * accepts (GDPR-style). In opt-out mode GA runs from page load and an explicit
 * Decline shuts it down. The banner never appears when GA is not configured, when
 * the visitor already chose, or when the browser signals Global Privacy Control
 * (plus Do-Not-Track in opt-in mode).
 */
export default function ConsentBanner() {
    const optOut = optOutMode();

    // Decide visibility once, from client-only signals (GA config, privacy
    // signals, stored choice). Computed in the initializer rather than an effect
    // so there is no setState-in-effect.
    const [visible, setVisible] = useState(
        () =>
            analyticsConfigured() &&
            !globalPrivacyControl() &&
            (optOut || !doNotTrack()) &&
            !storedChoice(),
    );

    useEffect(() => {
        // Side effect only, and only in opt-in mode: if the visitor already
        // consented, load analytics. Opt-out mode has already loaded it from
        // index.html before React mounted — that is the whole point of the mode.
        if (
            !optOut &&
            analyticsConfigured() &&
            !globalPrivacyControl() &&
            !doNotTrack() &&
            storedChoice() === 'granted'
        ) {
            window.loadAnalytics?.();
        }
    }, [optOut]);

    if (!visible) return null;

    const accept = () => {
        storeChoice('granted');
        window.loadAnalytics?.();
        setVisible(false);
    };
    const decline = () => {
        storeChoice('denied');
        // Only opt-out mode can have a tag running at this point; in opt-in mode
        // this is a harmless no-op rather than a second code path.
        window.disableAnalytics?.();
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
                    We use privacy-friendly analytics to understand how the site is used.{' '}
                    {optOut
                        ? 'It is on by default — decline to turn it off.'
                        : 'Nothing is collected until you agree.'}{' '}
                    See our <a href="/privacy" className="text-primary-600 underline">privacy policy</a>.
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
