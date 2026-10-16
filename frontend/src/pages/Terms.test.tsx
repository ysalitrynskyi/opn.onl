import { describe, expect, it } from 'vitest';
import { render, screen } from '../test/test-utils';
import Terms from './Terms';

describe('Terms of Service', () => {
    it('does not promise Settings deletion on an instance that has it switched off', () => {
        render(<Terms />);

        const termination = screen.getByText(/where the instance offers it/i);
        expect(termination).toHaveTextContent(/contacting the operator/i);
        expect(termination).not.toHaveTextContent(/in Settings or by contacting us/);
        expect(screen.getByText('Last updated: October 16, 2026')).toBeInTheDocument();
        // The effective date is unchanged: only the deletion sentence moved.
        expect(screen.getByText(/effective as of December 7, 2025/)).toBeInTheDocument();
    });
});
