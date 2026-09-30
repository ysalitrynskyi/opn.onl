import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import App from './App';
import { blockSiteStorage } from './test/helpers';

// Lazy routes load their module on first visit; give that more than the default
// one second on a cold transform cache.
const LAZY_ROUTE_TIMEOUT = { timeout: 5000 };

const renderAt = (path: string) =>
    render(
        <MemoryRouter initialEntries={[path]}>
            <App />
        </MemoryRouter>,
    );

// A browser that blocks site data throws on any touch of localStorage or
// sessionStorage. The app must still serve every page, treating the visitor as
// signed out, instead of replacing the site with the error screen.
describe('App with site storage blocked by the browser', () => {
    let unblock: () => void;
    beforeEach(() => {
        unblock = blockSiteStorage();
    });
    afterEach(() => unblock());

    it.each([
        ['/', /short links that\s+answer to you/i],
        ['/login', /sign in to opn\.onl/i],
        ['/register', /create your account/i],
    ])('renders the public page %s', async (path, heading) => {
        renderAt(path);

        expect(await screen.findByRole('heading', { level: 1, name: heading })).toBeInTheDocument();
        expect(screen.getAllByRole('link', { name: /log in/i }).length).toBeGreaterThan(0);
        expect(screen.queryByText(/something went wrong/i)).not.toBeInTheDocument();
    });

    it.each(['/dashboard', '/settings', '/admin', '/analytics/1'])(
        'sends %s to the login page as a signed-out visitor',
        async (path) => {
            renderAt(path);

            expect(
                await screen.findByRole('heading', { level: 1, name: /sign in to opn\.onl/i }, LAZY_ROUTE_TIMEOUT),
            ).toBeInTheDocument();
            expect(screen.queryByText(/something went wrong/i)).not.toBeInTheDocument();
        },
    );

    it('sends a visitor who shortens on the home page on to sign up', async () => {
        const user = userEvent.setup();
        renderAt('/');

        await user.type(await screen.findByPlaceholderText(/your-very-long-link/i), 'https://example.com/long');
        await user.click(screen.getByRole('button', { name: /shorten/i }));

        expect(await screen.findByRole('heading', { level: 1, name: /create your account/i })).toBeInTheDocument();
        expect(screen.queryByText(/something went wrong/i)).not.toBeInTheDocument();
    });
});
