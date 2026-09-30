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
});
