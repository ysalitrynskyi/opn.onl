import { describe, it, expect } from 'vitest';
import { render, screen } from '../test/test-utils';
import Faq from './Faq';

describe('FAQ', () => {
    // GA_CONSENT_MODE decides when Google Analytics runs: opt-in waits for
    // Accept, opt-out runs until Decline (production is opt-out). The FAQ is
    // prerendered once for every deployment, so it has to be true for both.
    it('says when Google Analytics runs in either consent mode', async () => {
        const { user } = render(<Faq />);

        await user.click(screen.getByRole('button', { name: /do you track users who click my links/i }));

        const answer = await screen.findByText(/if the operator enables google analytics/i);
        expect(answer).toHaveTextContent(/only after you accept/i);
        expect(answer).toHaveTextContent(/until you decline/i);
        expect(answer).toHaveTextContent(/global privacy control signal keeps it off in both cases/i);
        expect(answer).not.toHaveTextContent(/does not load until you allow it/i);
    });

    it('tells assistive tech whether an answer is open', async () => {
        const { user } = render(<Faq />);
        const question = screen.getByRole('button', { name: /do you track users who click my links/i });

        expect(question).toHaveAttribute('aria-expanded', 'false');
        await user.click(question);
        expect(question).toHaveAttribute('aria-expanded', 'true');
    });
});
