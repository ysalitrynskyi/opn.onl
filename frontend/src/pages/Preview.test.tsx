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

describe('Preview page', () => {
    beforeEach(() => {
        global.fetch = vi.fn().mockResolvedValue({
            ok: true,
            status: 200,
            json: () => Promise.resolve({
                code: 'abc123',
                short_url: 'https://links.example.org/abc123',
                original_url: 'https://example.com/landing',
                domain: 'example.com',
                has_password: false,
                is_expired: false,
                created_at: '2024-01-01T00:00:00Z',
                click_count: 7,
            }),
        });
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
});
