import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '../test/test-utils';
import Faq from './Faq';

function mockSettings(settings: Record<string, unknown>) {
    global.fetch = vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: () => Promise.resolve(settings),
    });
}

function deleteAnswerSchema(): string {
    return [...document.querySelectorAll('script[type="application/ld+json"]')]
        .map((node) => node.textContent ?? '')
        .find((text) => text.includes('Can I delete my data?')) ?? '';
}

describe('FAQ', () => {
    afterEach(() => {
        global.fetch = vi.fn();
    });

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

    // A deleted link stops redirecting immediately, but the row stays until the
    // purge window from /auth/settings (30 days when the server is quiet).
    it('says a deleted link stops redirecting now and is removed after the default window', async () => {
        global.fetch = vi.fn().mockReturnValue(new Promise(() => {}));
        const { user } = render(<Faq />);

        await user.click(screen.getByRole('button', { name: /can i delete my data/i }));

        const answer = await screen.findByText(/stops redirecting at once/i);
        expect(answer).toHaveTextContent(/permanently removed 30 days later/i);
        expect(answer).not.toHaveTextContent(/processed immediately/i);
        expect(answer).toHaveTextContent(/account deletion is currently disabled on the hosted version/i);
        expect(deleteAnswerSchema()).toContain('permanently removed 30 days later');
        expect(deleteAnswerSchema()).not.toContain('processed immediately');
    });

    it("quotes the operator's purge window for a deleted link", async () => {
        mockSettings({ account_purge_days: 14 });
        const { user } = render(<Faq />);

        await user.click(screen.getByRole('button', { name: /can i delete my data/i }));

        const answer = await screen.findByText(/stops redirecting at once/i);
        await waitFor(() => expect(answer).toHaveTextContent(/permanently removed 14 days later/i));
        expect(answer).not.toHaveTextContent(/30 days/);
        await waitFor(() => expect(deleteAnswerSchema()).toContain('permanently removed 14 days later'));
    });

    it('says one day when the window is a single day', async () => {
        mockSettings({ account_purge_days: 1 });
        const { user } = render(<Faq />);

        await user.click(screen.getByRole('button', { name: /can i delete my data/i }));

        const answer = await screen.findByText(/stops redirecting at once/i);
        await waitFor(() => expect(answer).toHaveTextContent('permanently removed 1 day later'));
        expect(answer).not.toHaveTextContent('1 days');
    });

    it('does not promise removal when the operator switched the sweep off', async () => {
        mockSettings({ account_purge_days: null });
        const { user } = render(<Faq />);

        await user.click(screen.getByRole('button', { name: /can i delete my data/i }));

        const answer = await screen.findByText(/stops redirecting at once/i);
        await waitFor(() =>
            expect(answer).toHaveTextContent(/kept until an administrator permanently removes it/i),
        );
        expect(answer).not.toHaveTextContent(/30 days/);
        expect(answer).toHaveTextContent(/account deletion is currently disabled on the hosted version/i);
    });

    it('keeps the default window when an older server omits the field', async () => {
        mockSettings({});
        const { user } = render(<Faq />);

        await user.click(screen.getByRole('button', { name: /can i delete my data/i }));

        const answer = await screen.findByText(/stops redirecting at once/i);
        await waitFor(() => expect(global.fetch).toHaveBeenCalled());
        expect(answer).toHaveTextContent(/permanently removed 30 days later/i);
    });

    it('tells assistive tech whether an answer is open', async () => {
        const { user } = render(<Faq />);
        const question = screen.getByRole('button', { name: /do you track users who click my links/i });

        expect(question).toHaveAttribute('aria-expanded', 'false');
        await user.click(question);
        expect(question).toHaveAttribute('aria-expanded', 'true');
    });
});
