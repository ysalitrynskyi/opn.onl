import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '../test/test-utils';
import Dashboard from './Dashboard';

const link = {
    id: 7,
    code: 'zzcode',
    original_url: 'https://iana.org/not-the-title',
    short_url: 'http://localhost:3000/zzcode',
    title: 'Lifecycle title',
    click_count: 0,
    created_at: '2026-01-01 00:00:00',
    expires_at: null,
    has_password: false,
    notes: null,
    is_active: true,
    is_pinned: false,
    tags: [],
};

function json(body: unknown, status = 200) {
    return new Response(JSON.stringify(body), {
        status,
        headers: { 'Content-Type': 'application/json' },
    });
}

describe('Dashboard clipboard and title search', () => {
    beforeEach(() => {
        localStorage.setItem('token', 'test-token');
        vi.mocked(fetch).mockImplementation(async (input) => {
            const url = String(input);
            if (url.includes('/auth/settings')) {
                return json({
                    custom_aliases_enabled: false,
                    min_alias_length: 4,
                    max_alias_length: 50,
                    qr_branding_enabled: false,
                    burn_after_reading_enabled: false,
                    safe_link_interstitial_enabled: false,
                    conditional_routing_enabled: false,
                    link_in_bio_enabled: false,
                });
            }
            if (url.includes('/sparklines')) {
                return json({ sparklines: [] });
            }
            if (url.includes('/links')) {
                return json([link]);
            }
            return json({});
        });
    });

    it('reads the clipboard only after Paste, and filters by title', async () => {
        const { user } = render(<Dashboard />);

        expect(await screen.findByText('Lifecycle title')).toBeInTheDocument();
        expect(screen.queryByText(/url detected in clipboard/i)).not.toBeInTheDocument();
        const urlInput = screen.getByPlaceholderText('https://example.com/long-url');
        expect(urlInput).toHaveValue('');

        // user-event replaces navigator.clipboard, so install the spy after render.
        const readText = vi.fn().mockResolvedValue('https://iana.org/from-clipboard');
        Object.assign(navigator.clipboard, { readText });

        await user.click(screen.getByRole('button', { name: 'Paste' }));
        expect(readText).toHaveBeenCalledTimes(1);
        expect(urlInput).toHaveValue('https://iana.org/from-clipboard');

        const search = screen.getByPlaceholderText(/search links/i);
        await user.type(search, 'Lifecycle');
        expect(screen.getByText('Lifecycle title')).toBeInTheDocument();

        fireEvent.change(search, { target: { value: 'missing-title' } });
        expect(await screen.findByText(/no links found matching/i)).toBeInTheDocument();
    });
});
