import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '../test/test-utils';
import Dashboard from './Dashboard';

function makeLink(n: number) {
    return {
        id: n,
        code: `code${n}`,
        original_url: `https://example.com/page-${n}`,
        short_url: `http://localhost:3000/code${n}`,
        title: `Link ${n}`,
        click_count: 3,
        created_at: '2026-01-01T00:00:00Z',
        expires_at: null,
        has_password: false,
        notes: null,
        is_active: true,
        is_pinned: false,
        tags: [],
    };
}

function json(body: unknown, status = 200, headers: Record<string, string> = {}) {
    return new Response(JSON.stringify(body), {
        status,
        headers: { 'Content-Type': 'application/json', ...headers },
    });
}

const SETTINGS = {
    custom_aliases_enabled: false,
    min_alias_length: 4,
    max_alias_length: 50,
    qr_branding_enabled: false,
    burn_after_reading_enabled: false,
    safe_link_interstitial_enabled: false,
    conditional_routing_enabled: false,
    link_in_bio_enabled: false,
};

type BulkAnswer = { links: unknown[]; errors: string[] } | { status: number; body: unknown };

/** A server with two links whose POST /links/bulk is answered by `bulk`. */
function serve(bulk: (urls: string[]) => BulkAnswer = () => ({ links: [], errors: [] })) {
    vi.mocked(fetch).mockImplementation(async (input, init) => {
        const url = new URL(String(input));
        const method = init?.method ?? 'GET';
        if (url.pathname === '/auth/settings') return json(SETTINGS);
        if (url.pathname === '/auth/me') return json({ total_clicks: 0 });
        if (url.pathname === '/links/sparklines') return json({ sparklines: [] });
        if (url.pathname === '/links/bulk' && method === 'POST') {
            const answer = bulk(JSON.parse(String(init?.body)).urls);
            return 'status' in answer ? json(answer.body, answer.status) : json(answer);
        }
        if (url.pathname === '/links' && method === 'GET') {
            return json([makeLink(2), makeLink(1)], 200, { 'X-Total-Count': '2' });
        }
        return json({});
    });
}

/** The URL lists sent to POST /links/bulk so far. */
function bulkRequests(): string[][] {
    return vi
        .mocked(fetch)
        .mock.calls.filter(([input, init]) => new URL(String(input)).pathname === '/links/bulk' && init?.method === 'POST')
        .map(([, init]) => JSON.parse(String(init?.body)).urls);
}

function listRequestCount() {
    return vi
        .mocked(fetch)
        .mock.calls.filter(([input, init]) => new URL(String(input)).pathname === '/links' && (init?.method ?? 'GET') === 'GET')
        .length;
}

const createInput = () => screen.getByPlaceholderText('https://example.com/long-url');
const searchInput = () => screen.getByPlaceholderText(/search links/i);

describe('Dashboard bulk import', () => {
    const pasted = [
        'https://a.example/1',
        'https://b.example/2',
        'https://c.example/3',
        'https://d.example/4',
        'https://e.example/5',
        'https://f.example/6',
    ];
    const failed = pasted.slice(1, 5);
    const partlyDone = () => ({
        links: [{ id: 91, code: 'new1' }, { id: 92, code: 'new2' }],
        errors: [
            'https://b.example/2: Invalid URL',
            'Failed to shorten https://c.example/3: connection reset',
            'https://d.example/4: rate limit reached, try again in 30s',
            'https://e.example/5: This URL or domain has been blocked',
        ],
    });

    beforeEach(() => {
        localStorage.setItem('token', 'test-token');
    });

    async function openAndPaste(urls: string[]) {
        const rendered = render(<Dashboard />);
        await rendered.user.click(await screen.findByRole('button', { name: /bulk import/i }));
        const dialog = await screen.findByRole('dialog', { name: 'Bulk import URLs' });
        fireEvent.change(within(dialog).getByRole('textbox'), { target: { value: urls.join('\n') } });
        return { ...rendered, dialog };
    }

    it('keeps the modal open with only the failed URLs, and lists every failure with its reason', async () => {
        serve(partlyDone);
        const { user, dialog } = await openAndPaste(pasted);

        await user.click(within(dialog).getByRole('button', { name: /import all/i }));

        const report = await within(dialog).findByRole('alert');
        expect(report).toHaveTextContent(/^Created 2 links\. 4 failed\./);
        const rows = within(within(dialog).getByRole('list', { name: 'Failed URLs' })).getAllByRole('listitem');
        expect(rows).toHaveLength(4);
        expect(rows[0]).toHaveTextContent('https://b.example/2 Invalid URL');
        expect(rows[1]).toHaveTextContent('https://c.example/3 connection reset');
        expect(rows[2]).toHaveTextContent('https://d.example/4 rate limit reached, try again in 30s');
        expect(rows[3]).toHaveTextContent('https://e.example/5 This URL or domain has been blocked');
        expect(within(dialog).getByRole('textbox')).toHaveValue(failed.join('\n'));
        expect(within(dialog).getByText('4 valid URLs')).toBeInTheDocument();
        // The two that worked are in the list behind the modal.
        await waitFor(() => expect(listRequestCount()).toBeGreaterThan(1));
    });

    it('sends only the failed URLs when the import is tried again, then closes', async () => {
        let round = 0;
        serve(() => (round++ === 0 ? partlyDone() : { links: [{ id: 93 }, { id: 94 }, { id: 95 }, { id: 96 }], errors: [] }));
        const { user, dialog } = await openAndPaste(pasted);

        await user.click(within(dialog).getByRole('button', { name: /import all/i }));
        await within(dialog).findByRole('alert');
        await user.click(within(dialog).getByRole('button', { name: /import all/i }));

        await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
        expect(bulkRequests()).toEqual([pasted, failed]);

        await user.click(screen.getByRole('button', { name: /bulk import/i }));
        expect(within(await screen.findByRole('dialog')).getByRole('textbox')).toHaveValue('');
    });

    it('closes and clears the box when every URL was imported', async () => {
        serve(urls => ({ links: urls.map((_, id) => ({ id })), errors: [] }));
        const { user, dialog } = await openAndPaste(pasted);

        await user.click(within(dialog).getByRole('button', { name: /import all/i }));

        await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
        await user.click(screen.getByRole('button', { name: /bulk import/i }));
        expect(within(await screen.findByRole('dialog')).getByRole('textbox')).toHaveValue('');
    });

    it('keeps every URL when none of them could be imported', async () => {
        const two = pasted.slice(0, 2);
        serve(urls => ({ links: [], errors: urls.map(url => `${url}: This URL or domain has been blocked`) }));
        const { user, dialog } = await openAndPaste(two);

        await user.click(within(dialog).getByRole('button', { name: /import all/i }));

        expect(await within(dialog).findByRole('alert')).toHaveTextContent(/^Created 0 links\. 2 failed\./);
        expect(within(dialog).getByRole('textbox')).toHaveValue(two.join('\n'));
        expect(within(dialog).getAllByRole('listitem')).toHaveLength(2);
    });

    it('keeps the failed URLs, but not the old report, when the modal is closed and opened again', async () => {
        serve(partlyDone);
        const { user, dialog } = await openAndPaste(pasted);
        await user.click(within(dialog).getByRole('button', { name: /import all/i }));
        await within(dialog).findByRole('alert');

        await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
        await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
        await user.click(screen.getByRole('button', { name: /bulk import/i }));

        const reopened = await screen.findByRole('dialog', { name: 'Bulk import URLs' });
        expect(within(reopened).getByRole('textbox')).toHaveValue(failed.join('\n'));
        expect(within(reopened).queryByRole('alert')).not.toBeInTheDocument();
    });

    it('shows a refusal of the whole request inside the modal', async () => {
        serve(() => ({ status: 403, body: { links: [], errors: ['Please verify your email address before creating links'] } }));
        const { user, dialog } = await openAndPaste(pasted.slice(0, 1));

        await user.click(within(dialog).getByRole('button', { name: /import all/i }));

        expect(await within(dialog).findByRole('alert')).toHaveTextContent(/please verify your email/i);
        expect(within(dialog).getByRole('textbox')).toHaveValue(pasted[0]);
        expect(within(dialog).queryByRole('list')).not.toBeInTheDocument();
    });
});
