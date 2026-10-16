import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '../test/test-utils';
import { mockToken } from '../test/test-utils';
import Organizations from './Organizations';

const mockNavigate = vi.fn();
vi.mock('react-router-dom', async () => {
    const actual = await vi.importActual<typeof import('react-router-dom')>('react-router-dom');
    return { ...actual, useNavigate: () => mockNavigate };
});

const ME = { id: 10, email: 'me@example.com' };
const JOINED = '2026-01-02T03:04:05Z';

interface Org {
    id: number;
    name: string;
    slug: string;
    owner_id: number;
    created_at: string;
    member_count: number;
    link_count: number;
}

interface Member {
    id: number;
    user_id: number;
    email: string;
    role: string;
    joined_at: string;
}

function http(body: unknown, status = 200) {
    return {
        ok: status >= 200 && status < 300,
        status,
        json: async () => body,
    };
}

type Handle = (
    method: string,
    path: string,
    search: string,
    body: unknown,
) => { status?: number; body?: unknown } | void;

function install(opts: {
    orgs?: Org[];
    members?: Record<number, Member[]>;
    memberError?: { status: number; error: string };
    meStatus?: number;
    audit?: (offset: number) => unknown[];
    handle?: Handle;
} = {}) {
    const orgs = opts.orgs ?? [];
    const members = opts.members ?? {};
    global.fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const url = new URL(String(input));
        const method = (init?.method ?? 'GET').toUpperCase();
        const body = typeof init?.body === 'string' && init.body.length > 0 ? JSON.parse(init.body) : undefined;
        const custom = opts.handle?.(method, url.pathname, url.search, body);
        if (custom) return http(custom.body ?? null, custom.status ?? 200);
        if (url.pathname === '/auth/me') {
            if (opts.meStatus) return http({ error: 'Invalid or expired credentials' }, opts.meStatus);
            return http(ME);
        }
        if (url.pathname === '/orgs' && method === 'GET') return http(orgs);
        const memberList = url.pathname.match(/^\/orgs\/(\d+)\/members$/);
        if (memberList && method === 'GET') {
            if (opts.memberError) return http({ error: opts.memberError.error }, opts.memberError.status);
            return http(members[Number(memberList[1])] ?? []);
        }
        if (/^\/orgs\/\d+\/audit$/.test(url.pathname) && method === 'GET') {
            const offset = Number(url.searchParams.get('offset') ?? '0');
            return http(opts.audit ? opts.audit(offset) : []);
        }
        return http({ error: `unexpected ${method} ${url.pathname}` }, 500);
    }) as unknown as typeof fetch;
}

function calls(method: string, path: string) {
    return vi.mocked(fetch).mock.calls.filter((call) => {
        const url = String(call[0]);
        const init = call[1] as RequestInit | undefined;
        const used = (init?.method ?? 'GET').toUpperCase();
        return used === method && new URL(url).pathname === path;
    });
}

function team(role: 'owner' | 'admin' | 'editor' | 'viewer') {
    const meMember: Member = { id: 1, user_id: ME.id, email: ME.email, role, joined_at: JOINED };
    const owner: Member = { id: 2, user_id: 99, email: 'owner@example.com', role: 'owner', joined_at: '2026-01-01T00:00:00Z' };
    const ada: Member = { id: 3, user_id: 20, email: 'ada@example.com', role: 'editor', joined_at: '2026-01-03T00:00:00Z' };
    const members = role === 'owner' ? [meMember, ada] : [owner, meMember, ada];
    const org: Org = {
        id: 7,
        name: 'Acme',
        slug: 'acme',
        owner_id: role === 'owner' ? ME.id : 99,
        created_at: '2026-01-01T00:00:00Z',
        member_count: members.length,
        link_count: 2,
    };
    return { org, members, meMember, ada };
}

async function openAcme() {
    const { user } = render(<Organizations />);
    await user.click(await screen.findByRole('button', { name: /Acme/ }));
    expect(await screen.findByText('ada@example.com')).toBeInTheDocument();
    return user;
}

describe('Organizations page', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        localStorage.clear();
        localStorage.setItem('token', mockToken);
    });

    it('sends a signed-out visitor to log in without calling the API', async () => {
        localStorage.removeItem('token');
        install();
        render(<Organizations />);
        await waitFor(() => expect(mockNavigate).toHaveBeenCalledWith('/login'));
        expect(fetch).not.toHaveBeenCalled();
    });

    it('lists teams with the role from the members list', async () => {
        const owned = team('owner');
        const viewer: Member = { id: 4, user_id: ME.id, email: ME.email, role: 'viewer', joined_at: JOINED };
        const beta: Org = {
            id: 8,
            name: 'Beta',
            slug: 'beta',
            owner_id: 99,
            created_at: '2026-01-04T00:00:00Z',
            member_count: 1,
            link_count: 0,
        };
        install({
            orgs: [owned.org, beta],
            members: { 7: owned.members, 8: [viewer] },
        });
        render(<Organizations />);

        const acme = await screen.findByRole('button', { name: /Acme/ });
        expect(acme).toHaveTextContent('Owner');
        expect(acme).toHaveTextContent('2 members');
        expect(acme).toHaveTextContent('2 links');
        expect(screen.getByRole('button', { name: /Beta/ })).toHaveTextContent('Viewer');
        expect(screen.getByText('2 teams')).toBeInTheDocument();
        // Role comes from the members call, not from owner_id alone.
        expect(calls('GET', '/orgs/8/members').length).toBeGreaterThan(0);
    });

    it('shows an empty state when the account has no teams', async () => {
        install({ orgs: [] });
        render(<Organizations />);
        expect(await screen.findByText('You are not on a team yet.')).toBeInTheDocument();
        expect(screen.getByText('0 teams')).toBeInTheDocument();
    });

    it.each([
        ['viewer', false, false],
        ['editor', false, false],
        ['admin', true, false],
        ['owner', true, true],
    ] as const)('%s sees only the controls that role allows', async (role, manage, own) => {
        const fixture = team(role);
        install({ orgs: [fixture.org], members: { 7: fixture.members } });
        await openAcme();

        expect(screen.getByRole('link', { name: 'View links' })).toHaveAttribute('href', '/dashboard?org=7');
        if (manage) {
            expect(screen.getByRole('button', { name: 'Add member' })).toBeInTheDocument();
            expect(screen.getByRole('button', { name: 'Save' })).toBeInTheDocument();
            expect(screen.getByLabelText('Role for ada@example.com')).toBeInTheDocument();
            expect(screen.getByRole('button', { name: 'Remove ada@example.com' })).toBeInTheDocument();
            expect(screen.getByRole('heading', { name: 'Activity' })).toBeInTheDocument();
            await waitFor(() => {
                expect(calls('GET', '/orgs/7/audit').length).toBeGreaterThan(0);
            });
            const auditUrl = String(calls('GET', '/orgs/7/audit')[0][0]);
            expect(auditUrl).toContain('limit=50');
            expect(auditUrl).toContain('offset=0');
        } else {
            expect(screen.queryByRole('button', { name: 'Add member' })).not.toBeInTheDocument();
            expect(screen.queryByRole('button', { name: 'Save' })).not.toBeInTheDocument();
            expect(screen.queryByLabelText('Role for ada@example.com')).not.toBeInTheDocument();
            expect(screen.queryByRole('button', { name: 'Remove ada@example.com' })).not.toBeInTheDocument();
            expect(screen.queryByRole('heading', { name: 'Activity' })).not.toBeInTheDocument();
            expect(calls('GET', '/orgs/7/audit')).toHaveLength(0);
        }
        if (own) {
            expect(screen.getByRole('button', { name: 'Delete team' })).toBeInTheDocument();
            expect(screen.getByRole('button', { name: 'Transfer ownership' })).toBeInTheDocument();
            expect(screen.queryByRole('button', { name: 'Leave team' })).not.toBeInTheDocument();
        } else {
            expect(screen.queryByRole('button', { name: 'Delete team' })).not.toBeInTheDocument();
            expect(screen.queryByRole('button', { name: 'Transfer ownership' })).not.toBeInTheDocument();
            expect(screen.getByRole('button', { name: 'Leave team' })).toBeInTheDocument();
        }
    });

    it('creates a team with the name and slug it sends', async () => {
        const orgs: Org[] = [];
        const members: Record<number, Member[]> = {};
        install({
            orgs,
            members,
            handle(method, path, _search, body) {
                if (method === 'POST' && path === '/orgs') {
                    const payload = body as { name: string; slug: string };
                    const created: Org = {
                        id: 7,
                        name: payload.name,
                        slug: payload.slug,
                        owner_id: ME.id,
                        created_at: JOINED,
                        member_count: 1,
                        link_count: 0,
                    };
                    orgs.push(created);
                    members[7] = [{ id: 1, user_id: ME.id, email: ME.email, role: 'owner', joined_at: JOINED }];
                    return { status: 201, body: created };
                }
            },
        });
        const { user } = render(<Organizations />);
        await user.type(await screen.findByLabelText('Team name'), 'Acme');
        expect(screen.getByLabelText('Team slug')).toHaveValue('acme');
        await user.clear(screen.getByLabelText('Team slug'));
        await user.type(screen.getByLabelText('Team slug'), 'acme-hq');
        await user.click(screen.getByRole('button', { name: 'Create team' }));

        expect(await screen.findByRole('heading', { name: 'Acme' })).toBeInTheDocument();
        expect(screen.getByText('Team created')).toBeInTheDocument();
        expect(calls('POST', '/orgs')).toHaveLength(1);
        expect(calls('POST', '/orgs')[0][1]).toEqual(expect.objectContaining({
            method: 'POST',
            body: JSON.stringify({ name: 'Acme', slug: 'acme-hq' }),
            headers: expect.objectContaining({ Authorization: `Bearer ${mockToken}` }),
        }));
    });

    it('shows the server refusal when the slug is taken', async () => {
        install({
            orgs: [],
            handle(method, path) {
                if (method === 'POST' && path === '/orgs') {
                    return { status: 409, body: { error: 'Slug already exists' } };
                }
            },
        });
        const { user } = render(<Organizations />);
        await user.type(await screen.findByLabelText('Team name'), 'Acme');
        await user.click(screen.getByRole('button', { name: 'Create team' }));
        expect(await screen.findByRole('alert')).toHaveTextContent('Slug already exists');
        expect(screen.queryByText(/could not create/i)).not.toBeInTheDocument();
    });

    it('adds a member with the email and role, and shows a refusal verbatim', async () => {
        const fixture = team('owner');
        let refuse = true;
        install({
            orgs: [fixture.org],
            members: { 7: fixture.members },
            handle(method, path, _search, body) {
                if (method === 'POST' && path === '/orgs/7/members') {
                    if (refuse) return { status: 404, body: { error: 'User not found' } };
                    const payload = body as { email: string; role: string };
                    fixture.members.push({
                        id: 9,
                        user_id: 30,
                        email: payload.email,
                        role: payload.role,
                        joined_at: JOINED,
                    });
                    return {
                        status: 201,
                        body: { id: 9, user_id: 30, email: payload.email, role: payload.role, joined_at: JOINED },
                    };
                }
            },
        });
        const user = await openAcme();
        await user.type(screen.getByLabelText('Member email'), 'missing@example.com');
        await user.selectOptions(screen.getByLabelText('Member role'), 'editor');
        await user.click(screen.getByRole('button', { name: 'Add member' }));
        expect(await screen.findByRole('alert')).toHaveTextContent('User not found');
        expect(calls('POST', '/orgs/7/members')[0][1]).toEqual(expect.objectContaining({
            method: 'POST',
            body: JSON.stringify({ email: 'missing@example.com', role: 'editor' }),
        }));

        refuse = false;
        await user.clear(screen.getByLabelText('Member email'));
        await user.type(screen.getByLabelText('Member email'), 'new@example.com');
        await user.selectOptions(screen.getByLabelText('Member role'), 'admin');
        await user.click(screen.getByRole('button', { name: 'Add member' }));
        expect(await screen.findByText('new@example.com')).toBeInTheDocument();
        expect(calls('POST', '/orgs/7/members').at(-1)?.[1]).toEqual(expect.objectContaining({
            body: JSON.stringify({ email: 'new@example.com', role: 'admin' }),
        }));
    });

    it('changes a member role with a PUT of that role', async () => {
        const fixture = team('owner');
        install({
            orgs: [fixture.org],
            members: { 7: fixture.members },
            handle(method, path, _search, body) {
                if (method === 'PUT' && path === '/orgs/7/members/3') {
                    fixture.ada.role = (body as { role: string }).role;
                    return { status: 200, body: { ...fixture.ada } };
                }
            },
        });
        const user = await openAcme();
        await user.selectOptions(screen.getByLabelText('Role for ada@example.com'), 'viewer');
        await waitFor(() => expect(screen.getByLabelText('Role for ada@example.com')).toHaveValue('viewer'));
        expect(calls('PUT', '/orgs/7/members/3')[0][1]).toEqual(expect.objectContaining({
            method: 'PUT',
            body: JSON.stringify({ role: 'viewer' }),
        }));
    });

    it('shows the server message when a rename is refused', async () => {
        const fixture = team('admin');
        install({
            orgs: [fixture.org],
            members: { 7: fixture.members },
            handle(method, path) {
                if (method === 'PUT' && path === '/orgs/7') {
                    return { status: 403, body: { error: 'Insufficient permissions' } };
                }
            },
        });
        const user = await openAcme();
        await user.click(screen.getByRole('button', { name: 'Save' }));
        expect(await screen.findByRole('alert')).toHaveTextContent('Insufficient permissions');
        expect(calls('PUT', '/orgs/7')[0][1]).toEqual(expect.objectContaining({
            body: JSON.stringify({ name: 'Acme', slug: 'acme' }),
        }));
    });

    it('does not remove a member until the confirmation, then sends DELETE', async () => {
        const fixture = team('admin');
        install({
            orgs: [fixture.org],
            members: { 7: fixture.members },
            handle(method, path) {
                if (method === 'DELETE' && path === '/orgs/7/members/3') {
                    const index = fixture.members.findIndex((member) => member.id === 3);
                    if (index >= 0) fixture.members.splice(index, 1);
                    return { status: 204 };
                }
            },
        });
        const user = await openAcme();
        await user.click(screen.getByRole('button', { name: 'Remove ada@example.com' }));
        const dialog = screen.getByRole('dialog', { name: 'Remove member' });
        await user.click(within(dialog).getByRole('button', { name: 'Cancel' }));
        expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
        expect(calls('DELETE', '/orgs/7/members/3')).toHaveLength(0);

        await user.click(screen.getByRole('button', { name: 'Remove ada@example.com' }));
        await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Remove member' }));
        await waitFor(() => expect(screen.queryByText('ada@example.com')).not.toBeInTheDocument());
        expect(calls('DELETE', '/orgs/7/members/3')).toHaveLength(1);
    });

    it('leaves after confirmation and drops the team', async () => {
        const fixture = team('editor');
        const orgs = [fixture.org];
        install({
            orgs,
            members: { 7: fixture.members },
            handle(method, path) {
                if (method === 'POST' && path === '/orgs/7/leave') {
                    orgs.splice(0, orgs.length);
                    return { status: 204 };
                }
            },
        });
        const user = await openAcme();
        await user.click(screen.getByRole('button', { name: 'Leave team' }));
        await user.click(within(screen.getByRole('dialog', { name: 'Leave team' })).getByRole('button', { name: 'Leave team' }));
        expect(await screen.findByText('You left the team')).toBeInTheDocument();
        expect(screen.getByText('You are not on a team yet.')).toBeInTheDocument();
        expect(calls('POST', '/orgs/7/leave')).toHaveLength(1);
    });

    it('transfers ownership to the chosen member after confirmation', async () => {
        const fixture = team('owner');
        install({
            orgs: [fixture.org],
            members: { 7: fixture.members },
            handle(method, path, _search, body) {
                if (method === 'POST' && path === '/orgs/7/transfer-ownership') {
                    fixture.org.owner_id = (body as { new_owner_user_id: number }).new_owner_user_id;
                    fixture.meMember.role = 'admin';
                    fixture.ada.role = 'owner';
                    return { status: 200, body: fixture.org };
                }
            },
        });
        const user = await openAcme();
        await user.click(screen.getByRole('button', { name: 'Transfer ownership' }));
        const dialog = screen.getByRole('dialog', { name: 'Transfer ownership' });
        expect(within(dialog).getByLabelText('New owner')).toHaveValue('20');
        await user.click(within(dialog).getByRole('button', { name: 'Transfer ownership' }));
        expect(await screen.findByText('Ownership transferred')).toBeInTheDocument();
        expect(calls('POST', '/orgs/7/transfer-ownership')[0][1]).toEqual(expect.objectContaining({
            method: 'POST',
            body: JSON.stringify({ new_owner_user_id: 20 }),
        }));
        expect(screen.queryByRole('button', { name: 'Delete team' })).not.toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Leave team' })).toBeInTheDocument();
    });

    it('does not offer transfer when the owner is the only member', async () => {
        const only: Member = { id: 1, user_id: ME.id, email: ME.email, role: 'owner', joined_at: JOINED };
        const org: Org = {
            id: 7, name: 'Acme', slug: 'acme', owner_id: ME.id,
            created_at: JOINED, member_count: 1, link_count: 0,
        };
        install({ orgs: [org], members: { 7: [only] } });
        const { user } = render(<Organizations />);
        await user.click(await screen.findByRole('button', { name: /Acme/ }));
        await user.click(await screen.findByRole('button', { name: 'Transfer ownership' }));
        const dialog = screen.getByRole('dialog', { name: 'Transfer ownership' });
        expect(within(dialog).getByText('Add a member before transferring ownership.')).toBeInTheDocument();
        expect(within(dialog).queryByRole('button', { name: 'Transfer ownership' })).not.toBeInTheDocument();
    });

    it('deletes only after the team name is typed', async () => {
        const fixture = team('owner');
        const orgs = [fixture.org];
        install({
            orgs,
            members: { 7: fixture.members },
            handle(method, path) {
                if (method === 'DELETE' && path === '/orgs/7') {
                    orgs.splice(0, orgs.length);
                    return { status: 204 };
                }
            },
        });
        const user = await openAcme();
        await user.click(screen.getByRole('button', { name: 'Delete team' }));
        const dialog = () => screen.getByRole('dialog', { name: 'Delete team' });
        const confirm = () => within(dialog()).getByRole('button', { name: 'Delete team' });
        expect(confirm()).toBeDisabled();
        await user.type(within(dialog()).getByLabelText('Type the team name'), 'nope');
        expect(confirm()).toBeDisabled();
        fireEvent.submit(within(dialog()).getByLabelText('Type the team name').closest('form') as HTMLFormElement);
        expect(calls('DELETE', '/orgs/7')).toHaveLength(0);

        await user.clear(within(dialog()).getByLabelText('Type the team name'));
        await user.type(within(dialog()).getByLabelText('Type the team name'), 'Acme');
        expect(confirm()).toBeEnabled();
        await user.click(confirm());
        expect(await screen.findByText('Team deleted')).toBeInTheDocument();
        expect(screen.getByText('You are not on a team yet.')).toBeInTheDocument();
        expect(calls('DELETE', '/orgs/7')).toHaveLength(1);
    });

    it('says the session ended when the stored session is rejected', async () => {
        install({ meStatus: 401 });
        render(<Organizations />);
        expect(await screen.findByRole('alert')).toHaveTextContent('Session expired. Please log in again.');
        expect(screen.queryByText(/could not load teams/i)).not.toBeInTheDocument();
    });

    it('shows a members refusal and hides management when the role cannot be loaded', async () => {
        const fixture = team('owner');
        install({
            orgs: [fixture.org],
            memberError: { status: 403, error: 'Not a member of this organization' },
        });
        const { user } = render(<Organizations />);
        await user.click(await screen.findByRole('button', { name: /Acme/ }));
        expect(await screen.findByRole('alert')).toHaveTextContent('Not a member of this organization');
        expect(screen.queryByRole('button', { name: 'Add member' })).not.toBeInTheDocument();
        expect(screen.queryByRole('button', { name: 'Delete team' })).not.toBeInTheDocument();
    });

    it('pages the activity log', async () => {
        const fixture = team('owner');
        const entry = (id: number) => ({
            id,
            user_id: ME.id,
            user_email: ME.email,
            action: 'update',
            resource_type: 'organization',
            resource_id: 7,
            details: null,
            ip_address: '203.0.113.0',
            created_at: '2026-02-01T00:00:00Z',
        });
        install({
            orgs: [fixture.org],
            members: { 7: fixture.members },
            audit(offset) {
                if (offset === 0) return Array.from({ length: 50 }, (_, index) => entry(index + 1));
                return [entry(51)];
            },
        });
        const user = await openAcme();
        const activity = () => screen.getByRole('list', { name: 'Activity' });
        await waitFor(() => expect(within(activity()).getAllByRole('listitem')).toHaveLength(50));
        expect(within(activity()).queryByText(/203\.0\.113/)).toBeNull();
        await user.click(screen.getByRole('button', { name: 'Older' }));
        await waitFor(() => expect(within(activity()).getAllByRole('listitem')).toHaveLength(1));
        const urls = calls('GET', '/orgs/7/audit').map((call) => String(call[0]));
        expect(urls.some((url) => url.includes('offset=0'))).toBe(true);
        expect(urls.some((url) => url.includes('offset=50'))).toBe(true);
    });
});
