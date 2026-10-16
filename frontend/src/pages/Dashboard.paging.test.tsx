import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '../test/test-utils';
import Dashboard from './Dashboard';

type LinkTag = { id: number; name: string; color: string | null };

function makeLink(n: number, overrides: Record<string, unknown> = {}) {
    const label = String(n).padStart(4, '0');
    return {
        id: n,
        code: `code${label}`,
        original_url: `https://example.com/page-${label}`,
        short_url: `http://localhost:3000/code${label}`,
        title: `Link ${label}`,
        click_count: 3,
        created_at: '2026-01-01T00:00:00Z',
        expires_at: null,
        has_password: false,
        notes: null as string | null,
        is_active: true,
        is_pinned: false,
        tags: [] as LinkTag[],
        ...overrides,
    };
}

// Newest first, the way the server lists them.
function makeLinks(count: number, overrides: Record<string, unknown> = {}) {
    return Array.from({ length: count }, (_, i) => makeLink(count - i, overrides));
}

function json(body: unknown, headers: Record<string, string> = {}) {
    return new Response(JSON.stringify(body), {
        status: 200,
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

/**
 * A server holding `all`: GET /links honours limit, offset and search,
 * lists pinned links first (`all` is otherwise newest first), and reports the
 * number of matches in X-Total-Count, like the real one. POST /links/:id/pin
 * flips the pin.
 */
function serve(all: ReturnType<typeof makeLinks>, account = { total_clicks: 0 }) {
    vi.mocked(fetch).mockImplementation(async (input, init) => {
        const url = new URL(String(input));
        const method = init?.method ?? 'GET';
        if (url.pathname === '/auth/settings') return json(SETTINGS);
        if (url.pathname === '/auth/me') return json(account);
        if (url.pathname === '/links/sparklines') return json({ sparklines: [] });
        const pin = url.pathname.match(/^\/links\/(\d+)\/pin$/);
        if (pin && method === 'POST') {
            const link = all.find(l => l.id === Number(pin[1]))!;
            link.is_pinned = !link.is_pinned;
            return json({ is_pinned: link.is_pinned, message: link.is_pinned ? 'Link pinned' : 'Link unpinned' });
        }
        if (url.pathname === '/links/bulk' && method === 'POST') {
            all.unshift(makeLink(all.length + 1, { title: 'Imported' }));
            return json({ links: [{}], errors: [] });
        }
        if (url.pathname === '/links' && method === 'GET') {
            const search = (url.searchParams.get('search') ?? '').toLowerCase();
            // Same fields as GET /links: URL, code, title, notes, and tag names.
            let matching = search
                ? all.filter(l => {
                    const tagNames = Array.isArray(l.tags) ? l.tags.map(tag => tag.name).join(' ') : '';
                    return [l.original_url, l.code, l.title ?? '', l.notes ?? '', tagNames]
                        .join(' ')
                        .toLowerCase()
                        .includes(search);
                })
                : all;
            // Stable, so the rest keep the order of `all`.
            matching = [...matching].sort((a, b) => Number(b.is_pinned) - Number(a.is_pinned));
            const limit = Number(url.searchParams.get('limit') ?? 1000);
            const offset = Number(url.searchParams.get('offset') ?? 0);
            return json(matching.slice(offset, offset + limit), {
                'X-Total-Count': String(matching.length),
            });
        }
        return json({});
    });
}

/** The query strings of every GET /links so far. */
function listRequests() {
    return vi
        .mocked(fetch)
        .mock.calls.map(([input, init]) => ({ url: new URL(String(input)), method: init?.method ?? 'GET' }))
        .filter(call => call.url.pathname === '/links' && call.method === 'GET')
        .map(call => call.url.searchParams);
}

function requested(path: string) {
    return vi.mocked(fetch).mock.calls.some(([input]) => new URL(String(input)).pathname === path);
}

describe('Dashboard paging', () => {
    beforeEach(() => {
        localStorage.setItem('token', 'test-token');
    });

    it('loads a page at a time and appends the next one on Load more', async () => {
        serve(makeLinks(250));
        const { user } = render(<Dashboard />);

        expect(await screen.findByText('Showing 100 of 250 links')).toBeInTheDocument();
        expect(listRequests()).toHaveLength(1);
        expect(listRequests()[0].get('limit')).toBe('100');
        expect(listRequests()[0].has('offset')).toBe(false);
        // The first page is what is browsable: 100 links in pages of twenty.
        expect(screen.getByText('1 / 5')).toBeInTheDocument();

        await user.click(screen.getByRole('button', { name: 'Load more' }));

        expect(await screen.findByText('Showing 200 of 250 links')).toBeInTheDocument();
        expect(listRequests()).toHaveLength(2);
        expect(listRequests()[1].get('offset')).toBe('100');
        expect(listRequests()[1].get('limit')).toBe('100');
        expect(screen.getByText('1 / 10')).toBeInTheDocument();

        await user.click(screen.getByRole('button', { name: 'Load more' }));

        // Everything is loaded: no more button, and the pager counts all 250.
        expect(await screen.findByText('1 / 13')).toBeInTheDocument();
        expect(screen.queryByRole('button', { name: 'Load more' })).not.toBeInTheDocument();
        expect(screen.queryByText(/of 250 links/)).not.toBeInTheDocument();
        expect(listRequests()[2].get('offset')).toBe('200');
        // Three pages of animated rows. Slow when the suite shares the CPU.
    }, 15_000);

    it('moves to the first of the new links when Load more is used on the last page', async () => {
        serve(makeLinks(250));
        const { user } = render(<Dashboard />);
        await screen.findByText('Showing 100 of 250 links');

        for (let i = 0; i < 4; i++) fireEvent.click(screen.getByRole('button', { name: 'Next page' }));
        expect(screen.getByText('5 / 5')).toBeInTheDocument();
        await user.click(screen.getByRole('button', { name: 'Load more' }));

        expect(await screen.findByText('6 / 10')).toBeInTheDocument();
        // Link 0150 is the 101st newest of 250, the first row of page 6.
        expect(screen.getByText('Link 0150')).toBeInTheDocument();
        // Five pages of animated rows; slow when the whole suite shares the CPU.
    }, 20_000);

    it('does not repeat a link that moved down while the next page was requested', async () => {
        const all = makeLinks(250);
        serve(all);
        const { user } = render(<Dashboard />);
        await screen.findByText('Showing 100 of 250 links');

        // A link created elsewhere shifts every row down by one, so the second
        // page starts with the last row of the first.
        all.unshift(makeLink(251));
        await user.click(screen.getByRole('button', { name: 'Load more' }));

        expect(await screen.findByText('Showing 199 of 251 links')).toBeInTheDocument();
    });

    it('shows the totals of the account, not of the loaded page', async () => {
        serve(makeLinks(250), { total_clicks: 4321 });
        render(<Dashboard />);

        expect(await screen.findByText('250 links', { exact: true })).toBeInTheDocument();
        // Settings reads this number from /auth/me; the hundred loaded rows
        // would add up to 300.
        expect(await screen.findByText('4,321 clicks', { exact: true })).toBeInTheDocument();
        expect(screen.queryByText('300 clicks')).not.toBeInTheDocument();
        // How many of all 250 are active is not known, so it is not guessed.
        expect(screen.queryByText(/active/)).not.toBeInTheDocument();
    });

    it('counts its own rows when the server has nothing more to give', async () => {
        const all = makeLinks(3);
        all[2] = makeLink(1, { is_active: false });
        serve(all, { total_clicks: 999 });
        render(<Dashboard />);

        expect(await screen.findByText('3 links', { exact: true })).toBeInTheDocument();
        expect(screen.getByText('2 active', { exact: true })).toBeInTheDocument();
        expect(screen.getByText('9 clicks', { exact: true })).toBeInTheDocument();
        expect(requested('/auth/me')).toBe(false);
        expect(screen.queryByRole('button', { name: 'Load more' })).not.toBeInTheDocument();
    });

    it('searches on the server when links are left unloaded', async () => {
        const all = makeLinks(250);
        // Far beyond the first hundred.
        all[200] = { ...all[200], title: 'The needle', original_url: 'https://example.com/needle' };
        serve(all, { total_clicks: 4321 });
        const { user } = render(<Dashboard />);
        await screen.findByText('Showing 100 of 250 links');
        expect(screen.queryByText('The needle')).not.toBeInTheDocument();

        await user.type(screen.getByRole('searchbox', { name: 'Search links' }), 'needle');

        expect(await screen.findByText('The needle', undefined, { timeout: 3000 })).toBeInTheDocument();
        const search = listRequests().filter(p => p.has('search'));
        expect(search).toHaveLength(1);
        expect(search[0].get('search')).toBe('needle');
        expect(search[0].get('limit')).toBe('100');
        // One match, all of it loaded; the account's counters do not shrink to it.
        expect(screen.queryByRole('button', { name: 'Load more' })).not.toBeInTheDocument();
        expect(screen.getByText('250 links', { exact: true })).toBeInTheDocument();
        expect(screen.getByText('4,321 clicks', { exact: true })).toBeInTheDocument();

        await user.clear(screen.getByRole('searchbox', { name: 'Search links' }));

        expect(await screen.findByText('Showing 100 of 250 links', undefined, { timeout: 3000 })).toBeInTheDocument();
        expect(listRequests().at(-1)?.has('search')).toBe(false);
        // A server search across animated rows. Slow when the suite shares the CPU.
    }, 15_000);

    it('finds a link by its tag when the list is paged', async () => {
        const all = makeLinks(250);
        // Past the first page, and "spring" is nowhere but the tag name.
        all[200] = {
            ...all[200],
            title: 'Link 0050',
            original_url: 'https://example.com/page-0050',
            notes: null,
            tags: [{ id: 1, name: 'spring', color: null }],
        };
        serve(all);
        const { user } = render(<Dashboard />);
        await screen.findByText('Showing 100 of 250 links');
        expect(screen.queryByText('Link 0050')).not.toBeInTheDocument();

        await user.type(screen.getByRole('searchbox', { name: 'Search links' }), 'spring');

        expect(await screen.findByText('Link 0050', undefined, { timeout: 3000 })).toBeInTheDocument();
        const search = listRequests().filter(p => p.has('search'));
        expect(search).toHaveLength(1);
        expect(search[0].get('search')).toBe('spring');
    });

    it('keeps the search box when the server finds nothing', async () => {
        serve(makeLinks(250));
        const { user } = render(<Dashboard />);
        await screen.findByText('Showing 100 of 250 links');

        await user.type(screen.getByRole('searchbox', { name: 'Search links' }), 'nothing-like-this');

        expect(await screen.findByText(/no links found matching/i, undefined, { timeout: 3000 })).toBeInTheDocument();
        expect(screen.getByRole('searchbox', { name: 'Search links' })).toBeInTheDocument();
        expect(screen.queryByText('No links yet')).not.toBeInTheDocument();
    });

    it('filters in the browser, with no request, when every link is loaded', async () => {
        serve(makeLinks(5));
        const { user } = render(<Dashboard />);
        await screen.findByText('Link 0003');

        await user.type(screen.getByRole('searchbox', { name: 'Search links' }), 'Link 0003');

        // Rows that do not match leave with an exit animation.
        await waitFor(() => expect(screen.queryByText('Link 0004')).not.toBeInTheDocument());
        expect(screen.getByText('Link 0003')).toBeInTheDocument();
        // Past the pause the server would have been asked.
        await new Promise(resolve => setTimeout(resolve, 600));
        expect(listRequests()).toHaveLength(1);
    });

    it('asks again for as many links as are showing after a change', async () => {
        const all = makeLinks(250);
        serve(all);
        const { user } = render(<Dashboard />);
        await screen.findByText('Showing 100 of 250 links');
        await user.click(screen.getByRole('button', { name: 'Load more' }));
        await screen.findByText('Showing 200 of 250 links');

        // A bulk import ends in a reload of the list.
        await user.click(screen.getByRole('button', { name: /bulk import/i }));
        await user.type(screen.getByPlaceholderText(/page1/i), 'https://example.com/new');
        await user.click(screen.getByRole('button', { name: /import all/i }));

        await waitFor(() => expect(listRequests()).toHaveLength(3));
        expect(listRequests()[2].get('limit')).toBe('200');
        expect(await screen.findByText('Showing 200 of 251 links')).toBeInTheDocument();
        // Two reloads of a couple of hundred rows; slow when the suite shares the CPU.
    }, 20_000);

    it('reads the list again after an unpin, so Load more neither skips a link nor runs forever', async () => {
        // An old pin keeps link 1 on the first page. Unpinning it hands its
        // place to link 51, which the server then has at index 99.
        const all = makeLinks(150);
        all[149].is_pinned = true;
        serve(all);
        const { user } = render(<Dashboard />);
        await screen.findByText('Showing 100 of 150 links');
        expect(listRequests()).toHaveLength(1);

        await user.click(screen.getByRole('button', { name: 'Unpin link code0001' }));

        await waitFor(() => expect(listRequests()).toHaveLength(2));
        expect(listRequests()[1].has('offset')).toBe(false);
        expect(listRequests()[1].get('limit')).toBe('100');
        await waitFor(() => expect(screen.queryByRole('button', { name: 'Unpin link code0001' })).not.toBeInTheDocument());

        await user.click(screen.getByRole('button', { name: 'Load more' }));

        expect(await screen.findByText('1 / 8')).toBeInTheDocument();
        expect(screen.queryByRole('button', { name: 'Load more' })).not.toBeInTheDocument();
        expect(screen.queryByText(/of 150 links/)).not.toBeInTheDocument();
        await user.type(screen.getByRole('searchbox', { name: 'Search links' }), 'Link 0051');
        expect(await screen.findByText('Link 0051')).toBeInTheDocument();
        // A hundred and fifty animated rows. Slow when the suite shares the CPU.
    }, 20_000);

    it('keeps a pin local when every link is already loaded', async () => {
        const all = makeLinks(5);
        serve(all);
        const { user } = render(<Dashboard />);
        await screen.findByText('Link 0001');

        await user.click(screen.getByRole('button', { name: 'Pin link code0001' }));

        expect(await screen.findByRole('button', { name: 'Unpin link code0001' })).toBeInTheDocument();
        expect(listRequests()).toHaveLength(1);
    });

    it('says that sorting covers only the loaded links', async () => {
        serve(makeLinks(250));
        const { user } = render(<Dashboard />);
        await screen.findByText('Showing 100 of 250 links');
        expect(screen.queryByText(/Sorting covers/)).not.toBeInTheDocument();

        await user.selectOptions(screen.getByLabelText('Sort links by'), 'clicks');

        expect(screen.getByText(/Sorting covers the links loaded so far/)).toBeInTheDocument();
    });
});
