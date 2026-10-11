import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, mockFetchResponse, mockToken, mockLink } from '../test/test-utils';
import Dashboard from './Dashboard';

const linkA = {
    ...mockLink,
    id: 1,
    code: 'aaa111',
    is_active: true,
    is_pinned: false,
};

type Call = { url: string; method: string };

/** A dashboard whose clone requests are answered by `cloneReply`. */
function mockDashboard(cloneReply: () => Promise<unknown>) {
    const calls: Call[] = [];
    vi.mocked(global.fetch).mockImplementation((url, options) => {
        const requestUrl = String(url);
        const method = (options?.method ?? 'GET').toUpperCase();
        calls.push({ url: requestUrl, method });
        if (requestUrl.endsWith('/auth/settings')) {
            return mockFetchResponse({
                custom_aliases_enabled: true,
                min_alias_length: 5,
                max_alias_length: 50,
            }) as any;
        }
        if (requestUrl.includes('/links/sparklines')) {
            return mockFetchResponse({ sparklines: [] }) as any;
        }
        if (method === 'POST' && requestUrl.includes('/clone')) {
            return cloneReply() as any;
        }
        return mockFetchResponse([linkA]) as any;
    });
    return {
        clonePosts: () => calls.filter(c => c.method === 'POST' && c.url.includes('/links/1/clone')),
    };
}

/** A clone request that stays open until `finish` is called. */
function pendingClone() {
    let finish: (ok?: boolean) => void = () => {};
    const reply = () =>
        new Promise(resolve => {
            finish = (ok = true) => resolve({
                ok,
                status: ok ? 201 : 403,
                json: () => Promise.resolve(ok ? { code: 'cloned9' } : { error: 'You have reached the maximum' }),
            });
        });
    return { reply, finish: (ok?: boolean) => finish(ok) };
}

describe('Dashboard clone', () => {
    beforeEach(() => {
        vi.mocked(global.fetch).mockReset();
        vi.mocked(localStorage.getItem).mockReturnValue(mockToken);
    });

    it('sends one request for a double click', async () => {
        const clone = pendingClone();
        const { clonePosts } = mockDashboard(clone.reply);

        const { user } = render(<Dashboard />);
        const button = await screen.findByRole('button', { name: /clone link aaa111/i });
        await user.dblClick(button);

        expect(clonePosts()).toHaveLength(1);
        clone.finish();
    });

    it('sends one request for two clicks in the same tick', async () => {
        const clone = pendingClone();
        const { clonePosts } = mockDashboard(clone.reply);

        render(<Dashboard />);
        const button = await screen.findByRole('button', { name: /clone link aaa111/i });
        fireEvent.click(button);
        fireEvent.click(button);

        expect(clonePosts()).toHaveLength(1);
        clone.finish();
    });

    it('disables the button while the request is open and enables it again after', async () => {
        const first = pendingClone();
        let reply = first.reply;
        const { clonePosts } = mockDashboard(() => reply());

        const { user } = render(<Dashboard />);
        const button = await screen.findByRole('button', { name: /clone link aaa111/i });
        expect(button).toBeEnabled();
        await user.click(button);
        expect(button).toBeDisabled();

        first.finish();
        await waitFor(() => expect(button).toBeEnabled());

        // A deliberate second clone, after the first was answered, goes through.
        const second = pendingClone();
        reply = second.reply;
        await user.click(button);
        await waitFor(() => expect(button).toBeDisabled());
        second.finish();
        await waitFor(() => expect(button).toBeEnabled());
        expect(clonePosts()).toHaveLength(2);
    });

    it('lets the user try again after a refused clone', async () => {
        const refused = pendingClone();
        let reply = refused.reply;
        const { clonePosts } = mockDashboard(() => reply());

        const { user } = render(<Dashboard />);
        const button = await screen.findByRole('button', { name: /clone link aaa111/i });
        await user.click(button);
        refused.finish(false);
        await waitFor(() => expect(button).toBeEnabled());
        expect(clonePosts()).toHaveLength(1);

        const retry = pendingClone();
        reply = retry.reply;
        await user.click(button);
        await waitFor(() => expect(button).toBeDisabled());
        retry.finish();
        await waitFor(() => expect(button).toBeEnabled());
        expect(clonePosts()).toHaveLength(2);
    });
});
