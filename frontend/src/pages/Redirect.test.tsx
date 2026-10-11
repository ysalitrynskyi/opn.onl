import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '../test/test-utils';
import Redirect from './Redirect';

vi.mock('react-router-dom', async () => {
    const actual = await vi.importActual('react-router-dom');
    return {
        ...actual,
        useParams: () => ({ code: 'abc123' }),
    };
});

const preview = {
    code: 'abc123',
    short_url: 'https://links.example.org/abc123',
    original_url: 'https://example.com/landing',
    domain: 'example.com',
    has_password: false,
    is_expired: false,
    is_active: true,
    burn_after_reading: false,
    created_at: '2024-01-01T00:00:00Z',
    click_count: 0,
    reputation: { verdict: 'safe', source: 'internal_blocklist' },
    interstitial_enabled: true,
    safe_link_interstitial: true,
};

function mockPreview(overrides: Record<string, unknown> = {}) {
    global.fetch = vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: () => Promise.resolve({ ...preview, ...overrides }),
    });
}

describe('Redirect interstitial', () => {
    const assign = vi.fn();
    const realLocation = window.location;

    beforeEach(() => {
        assign.mockReset();
        Object.defineProperty(window, 'location', {
            configurable: true,
            value: { ...realLocation, assign },
        });
    });

    afterEach(() => {
        Object.defineProperty(window, 'location', { configurable: true, value: realLocation });
    });

    it('shows the destination and its verdict', async () => {
        mockPreview();
        render(<Redirect />);

        expect(await screen.findByText('example.com')).toBeInTheDocument();
        expect(screen.getByText(/looks safe/i)).toBeInTheDocument();
        expect(screen.getByRole('link', { name: /continue/i })).toHaveAttribute(
            'href',
            expect.stringContaining('confirm=1'),
        );
    });

    it('never says "looks safe" about a one-time link whose destination is hidden', async () => {
        mockPreview({
            original_url: '',
            domain: '',
            burn_after_reading: true,
            reputation: { verdict: 'unknown', source: 'internal_blocklist' },
        });
        render(<Redirect />);

        expect(await screen.findByText(/this is a one-time link/i)).toBeInTheDocument();
        expect(screen.queryByText(/looks safe/i)).not.toBeInTheDocument();
        expect(screen.queryByText(/couldn't verify/i)).not.toBeInTheDocument();
        // Continuing is still possible; it is what uses the link up.
        expect(screen.getByRole('link', { name: /continue/i })).toBeInTheDocument();
    });

    it('does not claim a verdict when the API sent a safe one for an empty destination', async () => {
        // An older API answered "safe" for a destination it had blanked.
        mockPreview({ original_url: '', domain: '' });
        render(<Redirect />);

        expect(await screen.findByText(/can't be shown or checked in advance/i)).toBeInTheDocument();
        expect(screen.queryByText(/looks safe/i)).not.toBeInTheDocument();
    });

    it('skips the interstitial for a link that is no longer active', async () => {
        mockPreview({ is_active: false, original_url: '', domain: '' });
        render(<Redirect />);

        await waitFor(() => expect(assign).toHaveBeenCalledTimes(1));
        expect(String(assign.mock.calls[0][0])).toMatch(/\/abc123$/);
        expect(screen.queryByText(/you're leaving/i)).not.toBeInTheDocument();
    });
});
