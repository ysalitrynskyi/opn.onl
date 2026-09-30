import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import App from './App';

// A link inside a sentence has to be told apart from the text around it by
// more than colour (WCAG 1.4.1), so it is underlined. Navigation, buttons and
// links that make up their whole line are not "in running text" and are left
// alone.

const LAZY = { timeout: 5000 };

function json(body: unknown, status = 200) {
    return new Response(JSON.stringify(body), {
        status,
        headers: { 'Content-Type': 'application/json' },
    });
}

function inlineLinksWithoutUnderline(): string[] {
    return [...document.querySelectorAll('a[href]')]
        .filter((a) => {
            if (a.closest('nav, header, footer')) return false;
            const block = a.closest('p, li');
            if (!block) return false;
            const rest = (block.textContent ?? '').replace(a.textContent ?? '', '').trim();
            if (rest.length < 3) return false;
            // index.css underlines every .prose a.
            if (a.closest('.prose')) return false;
            return !/(^|\s)underline(\s|$)/.test(a.getAttribute('class') ?? '');
        })
        .map((a) => (a.textContent ?? '').trim());
}

function renderAt(path: string) {
    render(
        <MemoryRouter initialEntries={[path]}>
            <App />
        </MemoryRouter>,
    );
}

describe('links in running text are underlined', () => {
    beforeEach(() => {
        vi.mocked(fetch).mockReset();
    });

    it.each(['/', '/features', '/pricing', '/about', '/privacy', '/terms', '/contact', '/faq', '/docs', '/developers', '/login', '/register'])(
        'on %s',
        async (path) => {
            renderAt(path);
            await screen.findAllByRole('heading', { level: 1 });
            expect(inlineLinksWithoutUnderline()).toEqual([]);
        },
    );

    it('on the settings page', async () => {
        localStorage.setItem('token', 'test-token');
        vi.mocked(fetch).mockImplementation(async (input) => {
            const url = String(input);
            if (url.endsWith('/auth/me')) {
                return json({
                    id: 1, email: 'qa@example.com', email_verified: true, is_admin: false,
                    created_at: '2026-01-01T00:00:00Z', link_count: 0, total_clicks: 0,
                    display_name: null, bio: null, website: null, avatar_url: null, location: null,
                    bio_username: null, bio_enabled: false, bio_theme: null,
                });
            }
            if (url.endsWith('/auth/settings')) {
                return json({
                    account_deletion_enabled: false, custom_aliases_enabled: true, max_links_per_user: null,
                    passkeys_enabled: true, link_in_bio_enabled: false, api_keys_enabled: true,
                });
            }
            if (url.endsWith('/auth/passkeys')) return json({ passkeys: [] });
            return json([]);
        });

        renderAt('/settings');
        await screen.findByRole('link', { name: 'opn.onl MCP server' }, LAZY);
        expect(inlineLinksWithoutUnderline()).toEqual([]);
    });

    it('on the dashboard', async () => {
        localStorage.setItem('token', 'test-token');
        vi.mocked(fetch).mockImplementation(async (input) => {
            const url = String(input);
            if (url.endsWith('/links')) {
                return json([{
                    id: 1, code: 'abc123', original_url: 'https://example.com/long', short_url: 'http://localhost:3000/abc123',
                    title: null, click_count: 3, created_at: '2026-01-01T00:00:00Z', expires_at: null,
                    has_password: false, notes: null, is_active: true, is_pinned: false, tags: [],
                }]);
            }
            if (url.includes('/links/sparklines')) return json({ sparklines: [] });
            return json({});
        });

        renderAt('/dashboard');
        await screen.findByRole('button', { name: 'Delete link abc123' }, LAZY);
        expect(inlineLinksWithoutUnderline()).toEqual([]);
    });
});
