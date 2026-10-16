import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';
import Bio from './Bio';

function json(body: unknown, status = 200) {
    return new Response(JSON.stringify(body), {
        status,
        headers: { 'Content-Type': 'application/json' },
    });
}

function renderBio(username: string) {
    render(
        <MemoryRouter initialEntries={[`/@${username}`]}>
            <Routes>
                <Route path=":code" element={<Bio />} />
            </Routes>
        </MemoryRouter>,
    );
}

describe('Bio page theme', () => {
    it('renders a known theme and the link label', async () => {
        vi.mocked(fetch).mockImplementation(async (input) => {
            const url = String(input);
            if (url.includes('/api/bio/ada')) {
                return json({
                    username: 'ada',
                    display_name: 'Ada',
                    theme: 'ink',
                    links: [{ code: 'shop', short_url: 'https://links.example.org/shop', label: 'Shop window' }],
                });
            }
            return json({ error: 'not found' }, 404);
        });

        renderBio('ada');

        expect(await screen.findByRole('heading', { name: 'Ada' })).toBeInTheDocument();
        expect(document.querySelector('[data-bio-theme="ink"]')).toBeInTheDocument();
        expect(screen.getByRole('link', { name: 'Shop window' })).toBeInTheDocument();
    });

    it('falls back to the light page when the stored theme is missing or unknown', async () => {
        vi.mocked(fetch).mockImplementation(async (input) => {
            const url = String(input);
            if (url.includes('/api/bio/nope')) {
                return json({ username: 'nope', display_name: 'Nope', theme: 'neon', links: [] });
            }
            if (url.includes('/api/bio/plain')) {
                return json({ username: 'plain', display_name: 'Plain', links: [] });
            }
            return json({ error: 'not found' }, 404);
        });

        const { unmount } = render(
            <MemoryRouter initialEntries={['/@nope']}>
                <Routes>
                    <Route path=":code" element={<Bio />} />
                </Routes>
            </MemoryRouter>,
        );
        expect(await screen.findByRole('heading', { name: 'Nope' })).toBeInTheDocument();
        expect(document.querySelector('[data-bio-theme="light"]')).toBeInTheDocument();
        unmount();

        renderBio('plain');
        expect(await screen.findByRole('heading', { name: 'Plain' })).toBeInTheDocument();
        expect(document.querySelector('[data-bio-theme="light"]')).toBeInTheDocument();
    });
});
