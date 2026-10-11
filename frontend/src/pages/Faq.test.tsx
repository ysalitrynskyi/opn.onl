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

    // The home page sends a visitor without a session to /register, so "you can
    // use it without an account" was only true of the raw API.
    it('says the website needs an account', async () => {
        const { user } = render(<Faq />);

        await user.click(screen.getByRole('button', { name: /do i need an account to shorten links/i }));

        const answer = await screen.findByText(/asks you to sign up or log in before it creates a link/i);
        expect(answer).toHaveTextContent(/the api can create a basic link without an account/i);
        expect(answer).not.toHaveTextContent(/you can use opn\.onl without an account/i);
    });

    it('gives the alias rules the backend enforces by default', async () => {
        const { user } = render(<Faq />);

        await user.click(screen.getByRole('button', { name: /can i customize my short url/i }));

        const answer = await screen.findByText(/by default a custom alias is/i);
        expect(answer).toHaveTextContent(/5 to 25 characters/i);
        expect(answer).toHaveTextContent(/underscores/i);
        expect(answer).not.toHaveTextContent(/3-20/);
    });

    it('tells assistive tech whether an answer is open', async () => {
        const { user } = render(<Faq />);
        const question = screen.getByRole('button', { name: /do you track users who click my links/i });

        expect(question).toHaveAttribute('aria-expanded', 'false');
        await user.click(question);
        expect(question).toHaveAttribute('aria-expanded', 'true');
    });
});
