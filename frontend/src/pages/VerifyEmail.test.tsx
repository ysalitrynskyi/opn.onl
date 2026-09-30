import { StrictMode } from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '../test/test-utils';
import VerifyEmail from './VerifyEmail';

describe('VerifyEmail', () => {
    beforeEach(() => {
        vi.mocked(global.fetch).mockReset();
    });

    it('shows Invalid verification link when the token query parameter is missing', () => {
        render(<VerifyEmail />, { route: '/verify-email' });

        expect(screen.getByText('Invalid verification link. No token provided.')).toBeInTheDocument();
        expect(screen.getByRole('heading', { name: /verification failed/i })).toBeInTheDocument();
        expect(global.fetch).not.toHaveBeenCalled();
    });

    it('clears the token from the address bar and still verifies with it', async () => {
        vi.mocked(global.fetch).mockResolvedValue({ ok: true, json: async () => ({}) } as Response);

        // StrictMode runs the effects twice, as the dev build does.
        render(
            <StrictMode>
                <VerifyEmail />
            </StrictMode>,
            { route: '/verify-email?token=SECRET-verify-123' },
        );

        expect(await screen.findByRole('heading', { name: /email verified/i })).toBeInTheDocument();
        expect(window.location.pathname).toBe('/verify-email');
        expect(window.location.search).toBe('');
        expect(window.location.href).not.toContain('SECRET-verify-123');

        const calls = vi.mocked(global.fetch).mock.calls;
        expect(calls.length).toBeGreaterThan(0);
        for (const [url, init] of calls) {
            expect(String(url)).toMatch(/\/auth\/verify-email$/);
            expect(JSON.parse(String(init?.body))).toEqual({ token: 'SECRET-verify-123' });
        }
    });
});
