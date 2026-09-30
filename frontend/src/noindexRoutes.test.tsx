import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import App from './App';

// App and link pages must say noindex, and only that: no `index, follow` next
// to it, and no canonical pointing at another page (the home page, before).

const LAZY = { timeout: 5000 };

function json(body: unknown, status = 200) {
    return new Response(JSON.stringify(body), {
        status,
        headers: { 'Content-Type': 'application/json' },
    });
}

const pending = () => new Promise<Response>(() => {});

function renderAt(path: string) {
    render(
        <MemoryRouter initialEntries={[path]}>
            <App />
        </MemoryRouter>,
    );
}

async function expectNoindexOnly(title: string) {
    await waitFor(() => expect(document.title).toBe(title), LAZY);
    const robots = [...document.head.querySelectorAll('meta[name="robots"]')].map((m) => m.getAttribute('content'));
    expect(robots).toEqual(['noindex, nofollow']);
    expect(document.head.querySelectorAll('link[rel="canonical"]')).toHaveLength(0);
}

describe('noindex on app and link pages', () => {
    beforeEach(() => {
        vi.mocked(fetch).mockReset();
    });

    it('the password prompt of a protected link', async () => {
        renderAt('/password/abc123');
        await expectNoindexOnly('Password required | opn.onl');
    });

    it('the safe-link interstitial', async () => {
        vi.mocked(fetch).mockResolvedValue(
            json({
                code: 'abc123',
                domain: 'example.com',
                original_url: 'https://example.com/landing',
                has_password: false,
                is_expired: false,
                interstitial_enabled: true,
                safe_link_interstitial: true,
                reputation: { verdict: 'safe', source: 'internal_blocklist' },
            }),
        );
        renderAt('/r/abc123');
        await expectNoindexOnly('Before you continue | opn.onl');
    });

    it('a short link that cannot be loaded', async () => {
        vi.mocked(fetch).mockResolvedValue(json({ error: 'boom' }, 500));
        renderAt('/r/abc123');
        await expectNoindexOnly('Link unavailable | opn.onl');
    });

    it('a short link while it resolves', async () => {
        vi.mocked(fetch).mockImplementation(pending);
        renderAt('/abc123');
        await expectNoindexOnly('Redirecting | opn.onl');
    });

    it('the admin panel', async () => {
        localStorage.setItem('token', 'test-token');
        vi.mocked(fetch).mockImplementation(pending);
        renderAt('/admin');
        await expectNoindexOnly('Admin | opn.onl');
    });

    it('the forgot-password form', async () => {
        renderAt('/forgot-password');
        await expectNoindexOnly('Forgot Password - opn.onl');
    });

    it('a page that does not exist', async () => {
        renderAt('/no/such/page');
        await expectNoindexOnly('Page Not Found - opn.onl');
    });
});
