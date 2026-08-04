import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, screen, fireEvent } from '../test/test-utils';
import ConsentBanner from './ConsentBanner';

// The banner reads runtime globals that index.html normally injects, so each
// test sets them up explicitly. loadAnalytics/disableAnalytics are stubbed:
// this component's whole job is deciding whether to call them.
const loadAnalytics = vi.fn();
const disableAnalytics = vi.fn();

function setup(options: {
    gaId?: string;
    mode?: string;
    consent?: string | null;
    gpc?: boolean;
    dnt?: boolean;
} = {}) {
    const { gaId = 'G-TEST12345', mode, consent = null, gpc = false, dnt = false } = options;
    window.GA_ID = gaId;
    window.GA_CONSENT_MODE = mode;
    window.loadAnalytics = loadAnalytics;
    window.disableAnalytics = disableAnalytics;
    if (consent) localStorage.setItem('analytics_consent', consent);
    Object.defineProperty(navigator, 'globalPrivacyControl', { value: gpc, configurable: true });
    Object.defineProperty(navigator, 'doNotTrack', { value: dnt ? '1' : null, configurable: true });
}

const banner = () => screen.queryByRole('dialog', { name: 'Analytics consent' });

describe('ConsentBanner', () => {
    beforeEach(() => {
        localStorage.clear();
        loadAnalytics.mockClear();
        disableAnalytics.mockClear();
    });

    afterEach(() => {
        delete window.GA_ID;
        delete window.GA_CONSENT_MODE;
        delete window.loadAnalytics;
        delete window.disableAnalytics;
    });

    describe('opt-in mode (default)', () => {
        it('shows the banner and loads nothing until the visitor accepts', () => {
            setup();
            render(<ConsentBanner />);
            expect(banner()).toBeInTheDocument();
            expect(screen.getByText(/nothing is collected until you agree/i)).toBeInTheDocument();
            expect(loadAnalytics).not.toHaveBeenCalled();
        });

        it('loads analytics on accept and remembers the choice', () => {
            setup();
            render(<ConsentBanner />);
            fireEvent.click(screen.getByRole('button', { name: 'Accept' }));
            expect(loadAnalytics).toHaveBeenCalled();
            expect(localStorage.getItem('analytics_consent')).toBe('granted');
            expect(banner()).not.toBeInTheDocument();
        });

        it('loads analytics on mount for a visitor who already accepted', () => {
            setup({ consent: 'granted' });
            render(<ConsentBanner />);
            expect(loadAnalytics).toHaveBeenCalled();
            expect(banner()).not.toBeInTheDocument();
        });

        it('stays off for a visitor who declined', () => {
            setup({ consent: 'denied' });
            render(<ConsentBanner />);
            expect(loadAnalytics).not.toHaveBeenCalled();
            expect(banner()).not.toBeInTheDocument();
        });

        it('stays off and hides the banner under Do-Not-Track', () => {
            setup({ dnt: true });
            render(<ConsentBanner />);
            expect(banner()).not.toBeInTheDocument();
            expect(loadAnalytics).not.toHaveBeenCalled();
        });
    });

    describe('opt-out mode', () => {
        it('does not load analytics itself — index.html already did before mount', () => {
            setup({ mode: 'opt-out' });
            render(<ConsentBanner />);
            expect(loadAnalytics).not.toHaveBeenCalled();
        });

        it('shows a banner that says analytics is already on', () => {
            setup({ mode: 'opt-out' });
            render(<ConsentBanner />);
            expect(banner()).toBeInTheDocument();
            expect(screen.getByText(/on by default — decline to turn it off/i)).toBeInTheDocument();
        });

        it('shuts analytics down on decline and remembers the choice', () => {
            setup({ mode: 'opt-out' });
            render(<ConsentBanner />);
            fireEvent.click(screen.getByRole('button', { name: 'Decline' }));
            expect(disableAnalytics).toHaveBeenCalled();
            expect(localStorage.getItem('analytics_consent')).toBe('denied');
            expect(banner()).not.toBeInTheDocument();
        });

        it('keeps the banner up under Do-Not-Track, which it does not honor', () => {
            setup({ mode: 'opt-out', dnt: true });
            render(<ConsentBanner />);
            expect(banner()).toBeInTheDocument();
        });

        it('hides the banner under Global Privacy Control, which it does honor', () => {
            setup({ mode: 'opt-out', gpc: true });
            render(<ConsentBanner />);
            expect(banner()).not.toBeInTheDocument();
            expect(loadAnalytics).not.toHaveBeenCalled();
        });

        it('hides the banner once the visitor has chosen', () => {
            setup({ mode: 'opt-out', consent: 'denied' });
            render(<ConsentBanner />);
            expect(banner()).not.toBeInTheDocument();
        });
    });

    it('never appears when no GA measurement id is configured', () => {
        setup({ gaId: '%%GA_ID%%', mode: 'opt-out' });
        render(<ConsentBanner />);
        expect(banner()).not.toBeInTheDocument();
        expect(loadAnalytics).not.toHaveBeenCalled();
    });
});
