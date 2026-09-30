import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import App from './App';

const LAZY = { timeout: 5000 };

function json(body: unknown, status = 200) {
    return new Response(JSON.stringify(body), {
        status,
        headers: { 'Content-Type': 'application/json' },
    });
}

describe('short-link and bio routes', () => {
    it('loads /@name as a bio profile and does not preview it as a short link', async () => {
        const fetchMock = vi.mocked(fetch);
        fetchMock.mockImplementation(async (input) => {
            const url = String(input);
            if (url.includes('/api/bio/opnqav1a')) {
                return json({
                    username: 'opnqav1a',
                    display_name: 'QA Bio',
                    links: [],
                });
            }
            return json({ error: 'not found' }, 404);
        });

        render(
            <MemoryRouter initialEntries={['/@opnqav1a']}>
                <App />
            </MemoryRouter>,
        );

        expect(await screen.findByRole('heading', { name: 'QA Bio' }, LAZY)).toBeInTheDocument();
        const urls = fetchMock.mock.calls.map((call) => String(call[0]));
        expect(urls.some((url) => url.includes('/api/bio/opnqav1a'))).toBe(true);
        expect(urls.some((url) => url.includes('preview'))).toBe(false);
    });

    it('still treats a normal code as a short link', async () => {
        const fetchMock = vi.mocked(fetch);
        fetchMock.mockImplementation(async (input) => {
            const url = String(input);
            if (url.includes('/abcd12/preview')) {
                return json({
                    original_url: 'https://iana.org/x',
                    has_password: false,
                    is_expired: true,
                });
            }
            return json({ error: 'not found' }, 404);
        });

        render(
            <MemoryRouter initialEntries={['/abcd12']}>
                <App />
            </MemoryRouter>,
        );

        expect(await screen.findByText('This link has expired', {}, LAZY)).toBeInTheDocument();
        const urls = fetchMock.mock.calls.map((call) => String(call[0]));
        expect(urls.some((url) => url.includes('/abcd12/preview'))).toBe(true);
        expect(urls.some((url) => url.includes('/api/bio/'))).toBe(false);
    });
});
