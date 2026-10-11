import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '../test/test-utils';
import Preview from './Preview';

vi.mock('react-router-dom', async () => {
    const actual = await vi.importActual('react-router-dom');
    return {
        ...actual,
        useParams: () => ({ code: 'abc123+' }),
    };
});

const livePreview = {
    code: 'abc123',
    short_url: 'https://links.example.org/abc123',
    original_url: 'https://example.com/landing',
    domain: 'example.com',
    has_password: false,
    is_expired: false,
    is_active: true,
    burn_after_reading: false,
    created_at: '2024-01-01T00:00:00Z',
    click_count: 7,
    reputation: { verdict: 'safe', source: 'internal_blocklist' },
};

function mockPreview(overrides: Record<string, unknown> = {}) {
    global.fetch = vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: () => Promise.resolve({ ...livePreview, ...overrides }),
    });
}

describe('Preview page', () => {
    beforeEach(() => {
        mockPreview();
    });

    afterEach(() => {
        vi.unstubAllEnvs();
    });

    it('gives the icon-only destination link an accessible name', async () => {
        render(<Preview />);

        const link = await screen.findByRole('link', { name: 'Open example.com in a new tab' });
        expect(link).toHaveAttribute('href', 'https://example.com/landing');
    });

    it("names the short link with the instance's own host, not a hard-coded opn.onl", async () => {
        vi.stubEnv('VITE_FRONTEND_URL', 'https://links.example.org');
        render(<Preview />);

        expect(await screen.findByText('links.example.org/abc123')).toBeInTheDocument();
        expect(screen.queryByText(/opn\.onl\//)).not.toBeInTheDocument();
    });
    it('offers a live link normally', async () => {
        render(<Preview />);

        expect(await screen.findByRole('link', { name: /visit link/i })).toBeInTheDocument();
        expect(screen.getByText(/looks safe/i)).toBeInTheDocument();
        expect(screen.queryByText(/no longer active/i)).not.toBeInTheDocument();
    });

    it('says a link that is no longer active is, and disables Visit Link', async () => {
        // The API withholds the destination of a used-up, scheduled or blocked link.
        mockPreview({ is_active: false, original_url: '', domain: '' });
        render(<Preview />);

        expect(await screen.findByText('This link is no longer active')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Link Unavailable' })).toBeDisabled();
        expect(screen.queryByRole('link', { name: /visit link/i })).not.toBeInTheDocument();
        expect(screen.queryByText(/looks safe/i)).not.toBeInTheDocument();
        expect(screen.queryByText('Destination')).not.toBeInTheDocument();
        expect(screen.queryByRole('link', { name: /in a new tab/i })).not.toBeInTheDocument();
    });

    it('keeps the expired wording for an expired link', async () => {
        mockPreview({ is_active: false, is_expired: true, original_url: '', domain: '' });
        render(<Preview />);

        expect(await screen.findByText('This link has expired')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Link Expired' })).toBeDisabled();
        expect(screen.queryByRole('link', { name: /visit link/i })).not.toBeInTheDocument();
    });

    it('explains a one-time link instead of showing an empty destination', async () => {
        mockPreview({
            original_url: '',
            domain: '',
            burn_after_reading: true,
            reputation: { verdict: 'unknown', source: 'internal_blocklist' },
        });
        render(<Preview />);

        expect(await screen.findByText('Destination hidden')).toBeInTheDocument();
        expect(screen.getByText(/can be opened only once/i)).toBeInTheDocument();
        expect(screen.queryByText(/looks safe/i)).not.toBeInTheDocument();
        expect(screen.queryByText(/heading to/i)).not.toBeInTheDocument();
        expect(screen.queryByRole('link', { name: /in a new tab/i })).not.toBeInTheDocument();
        // The link is still live, so it can be visited.
        expect(screen.getByRole('link', { name: /visit link/i })).toBeInTheDocument();
    });
});
