import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '../test/test-utils';
import Dashboard from './Dashboard';

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

const ORGS = [
    { id: 2, name: 'Acme', slug: 'acme' },
    { id: 9, name: 'Beta', slug: 'beta' },
];

const FOLDERS = [{ id: 3, name: 'Campaigns', color: '#2563eb', link_count: 1 }];
const TAGS = [{ id: 4, name: 'launch', color: '#e11d48', link_count: 1 }];

function makeLink(n: number) {
    return {
        id: n,
        code: `code${n}`,
        original_url: `https://example.com/${n}`,
        short_url: `http://localhost:3000/code${n}`,
        title: `Link ${n}`,
        click_count: 1,
        created_at: '2026-01-01T00:00:00Z',
        expires_at: null,
        has_password: false,
        notes: null,
        is_active: true,
        is_pinned: false,
        tags: [] as { id: number; name: string; color?: string }[],
        folder_id: null as number | null,
    };
}

function json(body: unknown, status = 200, headers: Record<string, string> = {}) {
    return new Response(JSON.stringify(body), {
        status,
        headers: { 'Content-Type': 'application/json', ...headers },
    });
}

function serve(
    links: ReturnType<typeof makeLink>[],
    options: { total?: number; orgs?: typeof ORGS; folders?: typeof FOLDERS; tags?: typeof TAGS } = {},
) {
    const orgs = options.orgs ?? [];
    const folders = options.folders ?? [];
    const tags = options.tags ?? [];
    const total = options.total ?? links.length;
    vi.mocked(fetch).mockImplementation(async (input, init) => {
        const url = new URL(String(input));
        const method = init?.method ?? 'GET';
        if (url.pathname === '/auth/settings') return json(SETTINGS);
        if (url.pathname === '/auth/me') return json({ total_clicks: 0 });
        if (url.pathname === '/links/sparklines') return json({ sparklines: [] });
        if (url.pathname === '/orgs') return json(orgs);
        if (url.pathname === '/folders') return json(folders);
        if (url.pathname === '/tags') return json(tags);
        if (url.pathname === '/links/bulk' && method === 'POST') return json({ links: [{}], errors: [] });
        if (url.pathname === '/links/bulk/update' && method === 'POST') {
            const ids = (JSON.parse(String(init?.body)) as { ids: number[] }).ids;
            return json({ updated: ids.length });
        }
        if (url.pathname === '/links' && method === 'GET') {
            const limit = Number(url.searchParams.get('limit') ?? 1000);
            const offset = Number(url.searchParams.get('offset') ?? 0);
            return json(links.slice(offset, offset + limit), 200, { 'X-Total-Count': String(total) });
        }
        if (url.pathname === '/links' && method === 'POST') return json(makeLink(99), 201);
        return json({});
    });
}

function listRequests() {
    return vi.mocked(fetch).mock.calls
        .map(([input, init]) => ({ url: new URL(String(input)), method: init?.method ?? 'GET' }))
        .filter(call => call.url.pathname === '/links' && call.method === 'GET')
        .map(call => call.url.searchParams);
}

function callsTo(path: string) {
    return vi.mocked(fetch).mock.calls
        .map(([input]) => new URL(String(input)))
        .filter(url => url.pathname === path);
}

function bulkUpdates() {
    return vi.mocked(fetch).mock.calls
        .filter(([input, init]) => new URL(String(input)).pathname === '/links/bulk/update' && (init?.method ?? 'GET') === 'POST')
        .map(([, init]) => JSON.parse(String(init?.body)) as Record<string, unknown>);
}

describe('Dashboard folder, tag and team filters', () => {
    beforeEach(() => {
        localStorage.setItem('token', 'test-token');
    });

    it('asks for the personal list and hides the team switcher when there are no teams', async () => {
        serve([makeLink(1)]);
        render(<Dashboard />, { route: '/dashboard' });
        await screen.findByRole('checkbox', { name: 'Select link code1' });
        expect(screen.queryByRole('combobox', { name: 'Link context' })).not.toBeInTheDocument();
        expect(listRequests().some(params => params.has('org_id') || params.has('folder_id') || params.has('tag_id'))).toBe(false);
        expect(callsTo('/folders').some(url => url.searchParams.has('org_id'))).toBe(false);
        expect(callsTo('/tags').some(url => url.searchParams.has('org_id'))).toBe(false);
    });

    it('turns org, folder and tag in the URL into the link query, scoped to that team', async () => {
        serve([makeLink(1)], { orgs: ORGS, folders: FOLDERS, tags: TAGS });
        render(<Dashboard />, { route: '/dashboard?org=2&folder=3&tag=4' });
        await screen.findByRole('combobox', { name: 'Link context' });
        await waitFor(() => {
            const params = listRequests().at(-1);
            expect(params?.get('org_id')).toBe('2');
            expect(params?.get('folder_id')).toBe('3');
            expect(params?.get('tag_id')).toBe('4');
            expect(params?.get('limit')).toBe('100');
        });
        expect(callsTo('/folders').some(url => url.searchParams.get('org_id') === '2')).toBe(true);
        expect(callsTo('/tags').some(url => url.searchParams.get('org_id') === '2')).toBe(true);
        expect(callsTo('/orgs').some(url => url.searchParams.has('org_id'))).toBe(false);
    });

    it('drops a non-id in the URL instead of sending it', async () => {
        serve([makeLink(1)]);
        render(<Dashboard />, { route: '/dashboard?org=0&folder=no&tag=-2' });
        await screen.findByRole('searchbox', { name: 'Search links' });
        await waitFor(() => expect(listRequests().length).toBeGreaterThan(0));
        const params = listRequests().at(-1)!;
        expect(params.has('org_id')).toBe(false);
        expect(params.has('folder_id')).toBe(false);
        expect(params.has('tag_id')).toBe(false);
    });

    it('sends search and folder together, and resets to page 1 when the folder changes', async () => {
        const links = Array.from({ length: 25 }, (_, i) => makeLink(i + 1));
        serve(links, { folders: FOLDERS, total: 40 });
        render(<Dashboard />, { route: '/dashboard?folder=3' });
        await screen.findByRole('button', { name: 'Next page' });
        await waitFor(() => expect(listRequests().at(-1)?.get('folder_id')).toBe('3'));
        // The pointer path through animated rows is slow; the control is a button.
        fireEvent.click(screen.getByRole('button', { name: 'Next page' }));
        expect(screen.getByText('2 / 2')).toBeInTheDocument();

        const folderFilter = screen.getByRole('combobox', { name: 'Filter by folder' });
        fireEvent.change(folderFilter, { target: { value: '' } });
        await waitFor(() => {
            expect(screen.getByText('1 / 2')).toBeInTheDocument();
            expect(listRequests().at(-1)?.has('folder_id')).toBe(false);
        });

        fireEvent.change(folderFilter, { target: { value: '3' } });
        await waitFor(() => expect(listRequests().at(-1)?.get('folder_id')).toBe('3'));
        fireEvent.change(screen.getByRole('searchbox', { name: 'Search links' }), { target: { value: 'needle' } });
        await waitFor(() => {
            const params = listRequests().at(-1);
            expect(params?.get('search')).toBe('needle');
            expect(params?.get('folder_id')).toBe('3');
        });
        // Two folder reloads of animated rows, then a debounced search.
    }, 15_000);

    it('clears folder and tag when the team changes', async () => {
        serve([makeLink(1)], { orgs: ORGS, folders: FOLDERS, tags: TAGS });
        const { user } = render(<Dashboard />, { route: '/dashboard?org=2&folder=3&tag=4' });
        await screen.findByRole('combobox', { name: 'Link context' });
        await waitFor(() => expect(listRequests().at(-1)?.get('folder_id')).toBe('3'));
        await user.selectOptions(screen.getByRole('combobox', { name: 'Link context' }), '9');
        await waitFor(() => {
            const params = listRequests().at(-1);
            expect(params?.get('org_id')).toBe('9');
            expect(params?.has('folder_id')).toBe(false);
            expect(params?.has('tag_id')).toBe(false);
        });
        expect(window.location.search).toContain('org=9');
        expect(window.location.search).not.toContain('folder');
        expect(window.location.search).not.toContain('tag');
    });

    it('moves the selection into a folder and can take it back out', async () => {
        serve([makeLink(1)], { folders: FOLDERS });
        const { user } = render(<Dashboard />);
        await user.click(await screen.findByRole('checkbox', { name: 'Select link code1' }));
        await user.click(screen.getByRole('button', { name: 'Move to folder' }));
        const dialog = screen.getByRole('dialog', { name: 'Move to folder' });
        await user.selectOptions(within(dialog).getByRole('combobox', { name: 'Folder' }), '3');
        await user.click(within(dialog).getByRole('button', { name: /^Move$/ }));
        await waitFor(() => expect(bulkUpdates()).toContainEqual({ ids: [1], folder_id: 3 }));

        await user.click(await screen.findByRole('checkbox', { name: 'Select link code1' }));
        await user.click(screen.getByRole('button', { name: 'Move to folder' }));
        const again = screen.getByRole('dialog', { name: 'Move to folder' });
        await user.click(within(again).getByRole('button', { name: /^Move$/ }));
        await waitFor(() => expect(bulkUpdates()).toContainEqual({ ids: [1], remove_folder: true }));
    });

    it('creates imported links in the open team', async () => {
        serve([makeLink(1)], { orgs: ORGS });
        const { user } = render(<Dashboard />, { route: '/dashboard?org=2' });
        await user.click(await screen.findByRole('button', { name: 'Bulk Import' }));
        const dialog = screen.getByRole('dialog');
        fireEvent.change(within(dialog).getByRole('textbox'), { target: { value: 'https://example.com/imported' } });
        await user.click(within(dialog).getByRole('button', { name: 'Import All' }));
        await waitFor(() => {
            const post = vi.mocked(fetch).mock.calls.find(([input, init]) => (
                new URL(String(input)).pathname === '/links/bulk' && (init?.method ?? 'GET') === 'POST'
            ));
            expect(JSON.parse(String(post?.[1]?.body))).toEqual({
                urls: ['https://example.com/imported'],
                org_id: 2,
            });
        });
    });
});
