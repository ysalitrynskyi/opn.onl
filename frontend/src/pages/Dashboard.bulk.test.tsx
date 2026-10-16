import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '../test/test-utils';
import { ToastContainer } from '../components/Toast';
import Dashboard from './Dashboard';

// The real cap is 500. Two makes a five-row selection prove the split without
// building five hundred rows.
vi.mock('../components/dashboard/bulkActions', async () => {
    const actual = await vi.importActual<typeof import('../components/dashboard/bulkActions')>(
        '../components/dashboard/bulkActions',
    );
    return { ...actual, BULK_IDS_PER_REQUEST: 2 };
});

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
        expires_at: null as string | null,
        has_password: false,
        notes: null,
        is_active: true,
        is_pinned: false,
        tags: [] as { id: number; name: string; color: string }[],
        ...overrides,
    };
}

function makeLinks(count: number) {
    return Array.from({ length: count }, (_, i) => makeLink(count - i));
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

type BulkAnswer = { status: number; body: unknown };

/**
 * GET /links pages the way the server does. Bulk routes are answered by
 * `bulk`, which may also drop deleted rows so the next list matches.
 */
function serve(
    all: ReturnType<typeof makeLinks>,
    bulk: (path: string, ids: number[], body: Record<string, unknown>) => BulkAnswer = (_path, ids) => ({
        status: 200,
        body: { deleted: ids.length, updated: ids.length },
    }),
) {
    const links = [...all];
    vi.mocked(fetch).mockImplementation(async (input, init) => {
        const url = new URL(String(input));
        const method = init?.method ?? 'GET';
        if (url.pathname === '/auth/settings') return json(SETTINGS);
        if (url.pathname === '/auth/me') return json({ total_clicks: 0 });
        if (url.pathname === '/links/sparklines') {
            const ids = (url.searchParams.get('ids') ?? '').split(',').filter(Boolean).map(Number);
            return json({
                sparklines: ids.map((id) => ({ link_id: id, data: [1, 2], labels: ['a', 'b'] })),
            });
        }
        if ((url.pathname === '/links/bulk/delete' || url.pathname === '/links/bulk/update') && method === 'POST') {
            const body = JSON.parse(String(init?.body)) as Record<string, unknown>;
            const ids = body.ids as number[];
            const answer = bulk(url.pathname, ids, body);
            if (answer.status === 200 && url.pathname === '/links/bulk/delete') {
                const deleted = typeof (answer.body as { deleted?: unknown }).deleted === 'number'
                    ? (answer.body as { deleted: number }).deleted
                    : 0;
                const gone = new Set(ids.slice(0, deleted));
                for (let i = links.length - 1; i >= 0; i -= 1) {
                    if (gone.has(links[i].id)) links.splice(i, 1);
                }
            }
            return json(answer.body, answer.status);
        }
        if (url.pathname === '/links' && method === 'GET') {
            const search = url.searchParams.get('search') ?? '';
            const matching = search
                ? links.filter((link) => `${link.original_url} ${link.code} ${link.title}`.includes(search))
                : links;
            const limit = Number(url.searchParams.get('limit') ?? 1000);
            const offset = Number(url.searchParams.get('offset') ?? 0);
            return json(matching.slice(offset, offset + limit), 200, {
                'X-Total-Count': String(matching.length),
            });
        }
        return json({});
    });
}

function bulkPosts(path: string): Record<string, unknown>[] {
    return vi.mocked(fetch).mock.calls
        .filter(([input, init]) => new URL(String(input)).pathname === path && (init?.method ?? 'GET') === 'POST')
        .map(([, init]) => JSON.parse(String(init?.body)) as Record<string, unknown>);
}

function sparklineRequests(): string[] {
    return vi.mocked(fetch).mock.calls
        .filter(([input]) => new URL(String(input)).pathname === '/links/sparklines')
        .map(([input]) => new URL(String(input)).searchParams.get('ids') ?? '');
}

function renderDashboard() {
    return render(
        <>
            <Dashboard />
            <ToastContainer />
        </>,
    );
}

async function selectCodes(user: ReturnType<typeof renderDashboard>['user'], codes: string[]) {
    for (const code of codes) {
        await user.click(screen.getByRole('checkbox', { name: `Select link ${code}` }));
    }
}

describe('Dashboard bulk selection', () => {
    beforeEach(() => {
        localStorage.setItem('token', 'test-token');
    });

    it('selects rows, selects the whole page, and clears that on the next click', async () => {
        serve(makeLinks(3));
        const { user } = renderDashboard();

        const selectAll = await screen.findByRole('checkbox', { name: 'Select all on this page' });
        expect(screen.queryByRole('toolbar', { name: 'Bulk actions' })).not.toBeInTheDocument();

        await user.click(screen.getByRole('checkbox', { name: 'Select link code0001' }));
        expect(screen.getByRole('toolbar', { name: 'Bulk actions' })).toHaveTextContent('1 link selected');
        await waitFor(() => expect(selectAll).toBePartiallyChecked());

        await user.click(selectAll);
        expect(screen.getByRole('checkbox', { name: 'Select link code0001' })).toBeChecked();
        expect(screen.getByRole('checkbox', { name: 'Select link code0002' })).toBeChecked();
        expect(screen.getByRole('checkbox', { name: 'Select link code0003' })).toBeChecked();
        expect(screen.getByRole('toolbar', { name: 'Bulk actions' })).toHaveTextContent('3 links selected');

        await user.click(selectAll);
        expect(screen.getByRole('checkbox', { name: 'Select link code0001' })).not.toBeChecked();
        expect(screen.queryByRole('toolbar', { name: 'Bulk actions' })).not.toBeInTheDocument();
    });

    it('clears the selection when the page changes', async () => {
        serve(makeLinks(25));
        const { user } = renderDashboard();

        await screen.findByRole('checkbox', { name: 'Select all on this page' });
        await user.click(screen.getAllByRole('checkbox')[1]);
        expect(screen.getByRole('toolbar', { name: 'Bulk actions' })).toBeInTheDocument();

        await user.click(screen.getByRole('button', { name: 'Next page' }));
        await waitFor(() => {
            expect(screen.queryByRole('toolbar', { name: 'Bulk actions' })).not.toBeInTheDocument();
        });

        await user.click(screen.getByRole('button', { name: 'Previous page' }));
        expect(screen.getAllByRole('checkbox')[1]).not.toBeChecked();
    }, 15_000);

    it('clears the selection when the search changes', async () => {
        serve(makeLinks(2));
        const { user } = renderDashboard();

        await screen.findByRole('checkbox', { name: 'Select link code0001' });
        await user.click(screen.getByRole('checkbox', { name: 'Select link code0001' }));
        expect(screen.getByRole('toolbar', { name: 'Bulk actions' })).toBeInTheDocument();

        await user.type(screen.getByRole('searchbox', { name: 'Search links' }), 'Link');
        expect(screen.queryByRole('toolbar', { name: 'Bulk actions' })).not.toBeInTheDocument();
        expect(screen.getByRole('checkbox', { name: 'Select link code0001' })).not.toBeChecked();
    });

    it('deletes the selected rows, refreshes, and does not redraw the remaining charts', async () => {
        serve(makeLinks(3));
        const { user } = renderDashboard();

        await screen.findByRole('checkbox', { name: 'Select link code0001' });
        await waitFor(() => expect(sparklineRequests()).toHaveLength(1));
        const chartsBefore = document.querySelectorAll('svg').length;
        expect(chartsBefore).toBeGreaterThan(0);

        await selectCodes(user, ['code0001', 'code0002']);
        await user.click(screen.getByRole('button', { name: 'Delete' }));
        const dialog = screen.getByRole('dialog', { name: 'Delete 2 links?' });
        await user.click(within(dialog).getByRole('button', { name: 'Delete' }));

        await waitFor(() => expect(bulkPosts('/links/bulk/delete')).toEqual([{ ids: [1, 2] }]));
        await waitFor(() => {
            expect(screen.queryByRole('checkbox', { name: 'Select link code0001' })).not.toBeInTheDocument();
        });
        expect(screen.getByRole('checkbox', { name: 'Select link code0003' })).toBeInTheDocument();
        expect(screen.queryByRole('toolbar', { name: 'Bulk actions' })).not.toBeInTheDocument();
        expect(screen.getByText('Deleted 2 links.')).toBeInTheDocument();
        expect(sparklineRequests()).toHaveLength(1);
        expect(document.querySelectorAll('svg').length).toBeLessThan(chartsBefore);
    });

    it('reports a shortfall instead of claiming every link was deleted', async () => {
        serve(makeLinks(2), () => ({ status: 200, body: { deleted: 1 } }));
        const { user } = renderDashboard();

        await screen.findByRole('checkbox', { name: 'Select all on this page' });
        await user.click(screen.getByRole('checkbox', { name: 'Select all on this page' }));
        await user.click(screen.getByRole('button', { name: 'Delete' }));
        await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Delete' }));

        expect(await screen.findByRole('alert')).toHaveTextContent(
            'Deleted 1 of 2 links. 1 could not be deleted.',
        );
        expect(screen.queryByText('Deleted 2 links.')).not.toBeInTheDocument();
        await waitFor(() => {
            expect(screen.queryByRole('toolbar', { name: 'Bulk actions' })).not.toBeInTheDocument();
        });
    });

    it('shows a refused request and keeps the selection', async () => {
        serve(makeLinks(2), () => ({
            status: 400,
            body: { error: 'Too many items in one request (max 500)' },
        }));
        const { user } = renderDashboard();

        await screen.findByRole('checkbox', { name: 'Select link code0001' });
        await user.click(screen.getByRole('checkbox', { name: 'Select link code0001' }));
        await user.click(screen.getByRole('button', { name: 'Delete' }));
        await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Delete' }));

        expect(await screen.findByRole('alert')).toHaveTextContent(
            'Too many items in one request (max 500)',
        );
        expect(screen.getByRole('checkbox', { name: 'Select link code0001' })).toBeChecked();
        expect(screen.getByRole('toolbar', { name: 'Bulk actions' })).toBeInTheDocument();
        expect(screen.queryByText(/Deleted/)).not.toBeInTheDocument();
    });

    it('says how long to wait when a bulk action is rate limited', async () => {
        serve(makeLinks(2), () => ({
            status: 429,
            body: {
                error: 'Too many requests',
                retry_after: 9,
                message: 'Rate limit exceeded. Please try again in 9 seconds.',
            },
        }));
        const { user } = renderDashboard();

        await screen.findByRole('checkbox', { name: 'Select link code0001' });
        await user.click(screen.getByRole('checkbox', { name: 'Select link code0001' }));
        await user.click(screen.getByRole('button', { name: 'Remove expiry' }));

        expect(await screen.findByRole('alert')).toHaveTextContent('Rate limit exceeded. Please try again in 9 seconds.');
        expect(screen.getByRole('alert')).not.toHaveTextContent('Too many requests');
        expect(screen.getByRole('checkbox', { name: 'Select link code0001' })).toBeChecked();
    });

    it('says when the server accepted the call but sent no count', async () => {
        serve(makeLinks(1), () => ({ status: 200, body: {} }));
        const { user } = renderDashboard();

        await screen.findByRole('checkbox', { name: 'Select link code0001' });
        await user.click(screen.getByRole('checkbox', { name: 'Select link code0001' }));
        await user.click(screen.getByRole('button', { name: 'Delete' }));
        await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Delete' }));

        expect(await screen.findByRole('alert')).toHaveTextContent('The server response could not be read.');
        expect(screen.getByRole('checkbox', { name: 'Select link code0001' })).toBeChecked();
    });

    it('sends a selection larger than the chunk size as several requests', async () => {
        serve(makeLinks(5));
        const { user } = renderDashboard();

        await screen.findByRole('checkbox', { name: 'Select link code0001' });
        await selectCodes(user, ['code0001', 'code0002', 'code0003', 'code0004', 'code0005']);
        await user.click(screen.getByRole('button', { name: 'Delete' }));
        await user.click(within(screen.getByRole('dialog', { name: 'Delete 5 links?' })).getByRole('button', { name: 'Delete' }));

        await waitFor(() => expect(bulkPosts('/links/bulk/delete')).toHaveLength(3));
        expect(bulkPosts('/links/bulk/delete')).toEqual([
            { ids: [1, 2] },
            { ids: [3, 4] },
            { ids: [5] },
        ]);
        await waitFor(() => {
            expect(screen.queryByRole('checkbox', { name: 'Select link code0001' })).not.toBeInTheDocument();
        });
    }, 15_000);

    it('stops after a chunk is refused and reports how far it got', async () => {
        let calls = 0;
        serve(makeLinks(5), (_path, ids) => {
            calls += 1;
            if (calls === 1) return { status: 200, body: { deleted: ids.length } };
            return { status: 500, body: { error: 'Database error' } };
        });
        const { user } = renderDashboard();

        await screen.findByRole('checkbox', { name: 'Select link code0001' });
        await selectCodes(user, ['code0001', 'code0002', 'code0003', 'code0004', 'code0005']);
        await user.click(screen.getByRole('button', { name: 'Delete' }));
        await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Delete' }));

        expect(await screen.findByRole('alert')).toHaveTextContent(
            'Deleted 2 links, then the rest failed: Database error',
        );
        await waitFor(() => expect(bulkPosts('/links/bulk/delete')).toHaveLength(2));
        expect(screen.queryByText('Deleted 5 links.')).not.toBeInTheDocument();
    }, 15_000);

    it('sets an expiry at the end of the picked local day and can clear it', async () => {
        serve(makeLinks(2));
        const { user } = renderDashboard();

        await screen.findByRole('checkbox', { name: 'Select link code0001' });
        await selectCodes(user, ['code0002', 'code0001']);
        await user.click(screen.getByRole('button', { name: 'Set expiry' }));
        const dialog = screen.getByRole('dialog', { name: 'Set expiry' });
        fireEvent.change(within(dialog).getByLabelText('Expiration'), { target: { value: '2030-01-15' } });
        await user.click(within(dialog).getByRole('button', { name: 'Set expiry' }));

        await waitFor(() => expect(bulkPosts('/links/bulk/update')).toEqual([
            { ids: [2, 1], expires_at: new Date('2030-01-15T23:59:00').toISOString() },
        ]));
        await waitFor(() => {
            expect(screen.queryByRole('toolbar', { name: 'Bulk actions' })).not.toBeInTheDocument();
        });

        await selectCodes(user, ['code0001']);
        await user.click(screen.getByRole('button', { name: 'Remove expiry' }));
        await waitFor(() => expect(bulkPosts('/links/bulk/update')).toEqual([
            { ids: [2, 1], expires_at: new Date('2030-01-15T23:59:00').toISOString() },
            { ids: [1], remove_expiration: true },
        ]));
    });

    it('returns focus to Delete when the confirm dialog is cancelled', async () => {
        serve(makeLinks(1));
        const { user } = renderDashboard();

        await screen.findByRole('checkbox', { name: 'Select link code0001' });
        await user.click(screen.getByRole('checkbox', { name: 'Select link code0001' }));
        await user.click(screen.getByRole('button', { name: 'Delete' }));
        await user.click(screen.getByRole('button', { name: 'Cancel' }));

        expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Delete' })).toHaveFocus();
        expect(screen.getByRole('checkbox', { name: 'Select link code0001' })).toBeChecked();
        expect(bulkPosts('/links/bulk/delete')).toEqual([]);
    });
});
