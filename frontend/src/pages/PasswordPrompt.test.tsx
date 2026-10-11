import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '../test/test-utils';
import PasswordPrompt from './PasswordPrompt';

vi.mock('react-router-dom', async () => {
    const actual = await vi.importActual('react-router-dom');
    return {
        ...actual,
        useParams: () => ({ code: 'abc123' }),
    };
});

function mockVerify(status: number, body: Record<string, unknown>) {
    global.fetch = vi.fn().mockResolvedValue({
        ok: status < 400,
        status,
        headers: new Headers(),
        json: () => Promise.resolve(body),
    });
}

async function submit(user: ReturnType<typeof render>['user']) {
    await user.type(screen.getByLabelText('Password'), 'hunter2!');
    await user.click(screen.getByRole('button', { name: /continue/i }));
}

describe('PasswordPrompt 410 reasons', () => {
    beforeEach(() => {
        vi.restoreAllMocks();
    });

    it('says an expired link has expired', async () => {
        mockVerify(410, { error: 'Link has expired' });
        const { user } = render(<PasswordPrompt />);
        await submit(user);

        expect(await screen.findByText('This link has expired.')).toBeInTheDocument();
    });

    it.each([
        ['This one-time link has already been opened', 'This one-time link has already been opened.'],
        ['Link has reached maximum clicks', 'Link has reached maximum clicks.'],
        ['Link is scheduled to activate later', 'Link is scheduled to activate later.'],
    ])('shows the real reason: %s', async (reason, shown) => {
        mockVerify(410, { error: reason });
        const { user } = render(<PasswordPrompt />);
        await submit(user);

        expect(await screen.findByText(shown)).toBeInTheDocument();
        expect(screen.queryByText('This link has expired.')).not.toBeInTheDocument();
    });
});
