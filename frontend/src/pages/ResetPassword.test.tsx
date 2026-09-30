import { StrictMode } from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '../test/test-utils';
import ResetPassword from './ResetPassword';

describe('ResetPassword', () => {
    beforeEach(() => {
        vi.mocked(global.fetch).mockReset();
    });

    it('clears the token from the address bar and still resets the password with it', async () => {
        vi.mocked(global.fetch).mockResolvedValue({ ok: true, json: async () => ({}) } as Response);

        // StrictMode runs the effects twice, as the dev build does.
        const { user } = render(
            <StrictMode>
                <ResetPassword />
            </StrictMode>,
            { route: '/reset-password?token=SECRET-reset-456' },
        );

        expect(await screen.findByRole('heading', { name: /reset your password/i })).toBeInTheDocument();
        expect(window.location.pathname).toBe('/reset-password');
        expect(window.location.search).toBe('');
        expect(window.location.href).not.toContain('SECRET-reset-456');

        await user.type(screen.getByLabelText('New Password'), 'correct horse battery');
        await user.type(screen.getByLabelText('Confirm Password'), 'correct horse battery');
        await user.click(screen.getByRole('button', { name: 'Reset Password' }));

        await waitFor(() => expect(global.fetch).toHaveBeenCalledTimes(1));
        const [url, init] = vi.mocked(global.fetch).mock.calls[0];
        expect(String(url)).toMatch(/\/auth\/reset-password$/);
        expect(JSON.parse(String(init?.body))).toEqual({
            token: 'SECRET-reset-456',
            password: 'correct horse battery',
        });
        expect(await screen.findByRole('heading', { name: /password reset!/i })).toBeInTheDocument();
    });

    it('shows the invalid-link card when there is no token', () => {
        render(<ResetPassword />, { route: '/reset-password' });

        expect(screen.getByRole('heading', { name: /invalid link/i })).toBeInTheDocument();
        expect(global.fetch).not.toHaveBeenCalled();
    });
});
