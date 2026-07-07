import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '../test/test-utils';
import Admin from './Admin';
import { mockToken } from '../test/test-utils';

// Mock navigate
const mockNavigate = vi.fn();
vi.mock('react-router-dom', async () => {
    const actual = await vi.importActual('react-router-dom');
    return {
        ...actual,
        useNavigate: () => mockNavigate,
    };
});

describe('Admin Page', () => {
    const mockStats = {
        total_users: 150,
        active_users: 120,
        verified_users: 100,
        admin_users: 2,
        total_links: 5000,
        active_links: 4500,
        total_clicks: 250000,
        total_orgs: 12,
        users_today: 3,
        links_today: 40,
        clicks_today: 900,
        blocked_links_count: 25,
        blocked_domains_count: 10,
        suspicious_links_count: 2,
    };

    const mockActivity = {
        days: [
            { date: '2026-07-05', new_users: 1, new_links: 12, clicks: 300 },
            { date: '2026-07-06', new_users: 2, new_links: 28, clicks: 600 },
        ],
    };

    const userDefaults = {
        display_name: null,
        bio_username: null,
        bio_enabled: false,
        links_count: 5,
        total_clicks: 42,
        api_keys_count: 1,
        passkeys_count: 0,
        orgs_owned: 0,
    };

    const mockUsers = {
        users: [
            {
                ...userDefaults,
                id: 1,
                email: 'admin@example.com',
                is_admin: true,
                email_verified: true,
                created_at: '2024-01-01T00:00:00Z',
                deleted_at: null,
            },
            {
                ...userDefaults,
                id: 2,
                email: 'user@example.com',
                is_admin: false,
                email_verified: true,
                created_at: '2024-01-02T00:00:00Z',
                deleted_at: null,
            },
            {
                ...userDefaults,
                id: 3,
                email: 'unverified@example.com',
                is_admin: false,
                email_verified: false,
                created_at: '2024-01-03T00:00:00Z',
                deleted_at: null,
            },
            {
                ...userDefaults,
                id: 4,
                email: 'deleted@example.com',
                is_admin: false,
                email_verified: true,
                created_at: '2024-01-04T00:00:00Z',
                deleted_at: '2024-01-05T00:00:00Z',
            },
        ],
        total: 4,
        page: 1,
        per_page: 25,
    };

    const mockLinks = {
        links: [
            {
                id: 10,
                code: 'abc123',
                original_url: 'https://example.com/some/long/path',
                title: 'Example page',
                user_id: 2,
                user_email: 'user@example.com',
                org_id: null,
                folder_id: null,
                click_count: 77,
                max_clicks: null,
                created_at: '2024-02-01T00:00:00Z',
                starts_at: null,
                expires_at: null,
                deleted_at: null,
                burned_at: null,
                is_pinned: false,
                burn_after_reading: false,
                safe_link_interstitial: false,
                bio_visible: false,
                has_password: true,
                is_active: true,
                inactive_reason: null,
                suspicious: false,
                suspicion_reason: null,
            },
            {
                id: 12,
                code: 'muzzlava',
                original_url: 'http://69.12.83.125/30/puregolds.hta',
                title: null,
                user_id: 5,
                user_email: 'ardhra56070@gmail.com',
                org_id: null,
                folder_id: null,
                click_count: 784,
                max_clicks: null,
                created_at: '2026-07-07T00:00:00Z',
                starts_at: null,
                expires_at: null,
                deleted_at: null,
                burned_at: null,
                is_pinned: false,
                burn_after_reading: false,
                safe_link_interstitial: false,
                bio_visible: false,
                has_password: false,
                is_active: true,
                inactive_reason: null,
                suspicious: true,
                suspicion_reason: 'dangerous file type (.hta), raw IP host',
            },
            {
                id: 11,
                code: 'gone99',
                original_url: 'https://old.example.com',
                title: null,
                user_id: 3,
                user_email: 'unverified@example.com',
                org_id: null,
                folder_id: null,
                click_count: 3,
                max_clicks: null,
                created_at: '2024-02-02T00:00:00Z',
                starts_at: null,
                expires_at: null,
                deleted_at: '2024-03-01T00:00:00Z',
                burned_at: null,
                is_pinned: false,
                burn_after_reading: false,
                safe_link_interstitial: false,
                bio_visible: false,
                has_password: false,
                is_active: false,
                inactive_reason: null,
                suspicious: false,
                suspicion_reason: null,
            },
        ],
        total: 3,
        page: 1,
        per_page: 25,
    };

    const mockOrgs = {
        orgs: [
            {
                id: 1,
                name: 'Acme Team',
                slug: 'acme-team',
                owner_id: 2,
                owner_email: 'user@example.com',
                member_count: 3,
                links_count: 15,
                created_at: '2024-01-10T00:00:00Z',
            },
        ],
        total: 1,
        page: 1,
        per_page: 25,
    };

    const mockBlockedLinks = [
        {
            id: 1,
            url: 'https://malicious.com/bad',
            reason: 'Malware',
            blocked_by: 1,
            created_at: '2024-01-01T00:00:00Z',
        },
    ];

    const mockBlockedDomains = [
        {
            id: 1,
            domain: 'spam.com',
            reason: 'Spam',
            blocked_by: 1,
            created_at: '2024-01-01T00:00:00Z',
        },
    ];

    beforeEach(() => {
        vi.clearAllMocks();
        localStorage.setItem('token', mockToken);

        // Mock fetch per endpoint. Blocked-content URLs must be matched before
        // the broader /admin/links and /admin/users prefixes.
        global.fetch = vi.fn((url: string) => {
            const respond = (payload: unknown) => Promise.resolve({
                ok: true,
                json: () => Promise.resolve(payload),
            });
            if (url.includes('/admin/stats')) return respond(mockStats);
            if (url.includes('/admin/activity')) return respond(mockActivity);
            if (url.includes('/admin/blocked/links')) return respond(mockBlockedLinks);
            if (url.includes('/admin/blocked/domains')) return respond(mockBlockedDomains);
            if (url.includes('/admin/users')) return respond(mockUsers);
            if (url.includes('/admin/links')) return respond(mockLinks);
            if (url.includes('/admin/orgs')) return respond(mockOrgs);
            return respond({});
        }) as any;
    });

    describe('Authentication & Authorization', () => {
        it('redirects to login if not authenticated', async () => {
            localStorage.removeItem('token');
            render(<Admin />);

            await waitFor(() => {
                expect(mockNavigate).toHaveBeenCalledWith('/login');
            });
        });

        it('redirects to dashboard if not admin', async () => {
            global.fetch = vi.fn().mockResolvedValue({
                ok: false,
                status: 403,
                json: () => Promise.resolve({}),
            });

            render(<Admin />);

            await waitFor(() => {
                expect(mockNavigate).toHaveBeenCalledWith('/dashboard');
            });
        });
    });

    describe('Page Header', () => {
        it('displays admin dashboard title', async () => {
            render(<Admin />);

            expect(await screen.findByRole('heading', { name: /admin dashboard/i })).toBeInTheDocument();
        });
    });

    describe('Navigation Tabs', () => {
        it('displays overview tab', async () => {
            render(<Admin />);

            expect(await screen.findByRole('button', { name: /overview/i })).toBeInTheDocument();
        });

        it('displays blocked content tab', async () => {
            render(<Admin />);

            expect(await screen.findByRole('button', { name: /blocked content/i })).toBeInTheDocument();
        });

        it('displays users, links, and organizations tabs', async () => {
            render(<Admin />);

            expect(await screen.findByRole('button', { name: /^users$/i })).toBeInTheDocument();
            expect(screen.getByRole('button', { name: /^links$/i })).toBeInTheDocument();
            expect(screen.getByRole('button', { name: /organizations/i })).toBeInTheDocument();
        });

        it('switches tabs on click', async () => {
            render(<Admin />);

            const usersTab = await screen.findByRole('button', { name: /^users$/i });
            fireEvent.click(usersTab);

            // Users table renders after switching tabs.
            expect(await screen.findByText('admin@example.com')).toBeInTheDocument();
        });
    });

    describe('Overview Tab', () => {
        it('displays total users', async () => {
            render(<Admin />);

            await waitFor(() => {
                expect(screen.getByText('150')).toBeInTheDocument();
            });
        });

        it('displays active users', async () => {
            render(<Admin />);

            await waitFor(() => {
                expect(screen.getByText('120')).toBeInTheDocument();
            });
        });

        it('displays total links', async () => {
            render(<Admin />);

            await waitFor(() => {
                expect(screen.getByText('5,000')).toBeInTheDocument();
            });
        });

        it('displays total clicks', async () => {
            render(<Admin />);

            await waitFor(() => {
                expect(screen.getByText('250,000')).toBeInTheDocument();
            });
        });

        it('displays organizations count and blocked counts', async () => {
            render(<Admin />);

            await waitFor(() => {
                expect(screen.getByText('12')).toBeInTheDocument();
                expect(screen.getByText('25')).toBeInTheDocument();
                expect(screen.getByText('10')).toBeInTheDocument();
            });
        });

        it('fetches the activity timeseries', async () => {
            render(<Admin />);

            await waitFor(() => {
                expect(global.fetch).toHaveBeenCalledWith(
                    expect.stringContaining('/admin/activity'),
                    expect.anything(),
                );
            });
        });

        it('shows a suspicious-links banner and jumps to the filtered Links tab', async () => {
            render(<Admin />);

            const banner = await screen.findByRole('button', { name: /suspicious link/i });
            fireEvent.click(banner);

            await waitFor(() => {
                expect(global.fetch).toHaveBeenCalledWith(
                    expect.stringMatching(/\/admin\/links\?.*suspicious=true/),
                    expect.anything(),
                );
            });
        });
    });

    describe('Backup Management', () => {
        it('displays backup section', async () => {
            render(<Admin />);

            expect(await screen.findByText(/backup management/i)).toBeInTheDocument();
        });

        it('can trigger backup creation', async () => {
            render(<Admin />);

            const backupBtn = await screen.findByRole('button', { name: /create backup/i });
            fireEvent.click(backupBtn);

            await waitFor(() => {
                expect(global.fetch).toHaveBeenCalledWith(
                    expect.stringContaining('/admin/backup'),
                    expect.objectContaining({ method: 'POST' })
                );
            });
        });
    });

    describe('Blocked Content Tab', () => {
        const openBlockedTab = async () => {
            const blockedTab = await screen.findByRole('button', { name: /blocked content/i });
            fireEvent.click(blockedTab);
        };

        it('displays blocked URLs section', async () => {
            render(<Admin />);
            await openBlockedTab();

            expect(await screen.findByRole('heading', { name: /block url/i })).toBeInTheDocument();
        });

        it('displays blocked domains section', async () => {
            render(<Admin />);
            await openBlockedTab();

            expect(await screen.findByRole('heading', { name: /block domain/i })).toBeInTheDocument();
        });

        it('can add blocked URL', async () => {
            render(<Admin />);
            await openBlockedTab();

            const urlInput = await screen.findByPlaceholderText(/example\.com\/malicious/i);
            fireEvent.change(urlInput, { target: { value: 'https://bad.com/page' } });

            const [blockBtn] = screen.getAllByRole('button', { name: /^block$/i });
            fireEvent.click(blockBtn);

            await waitFor(() => {
                expect(global.fetch).toHaveBeenCalledWith(
                    expect.stringContaining('/admin/blocked/links'),
                    expect.objectContaining({ method: 'POST' })
                );
            });
        });

        it('can add blocked domain', async () => {
            render(<Admin />);
            await openBlockedTab();

            const domainInput = await screen.findByPlaceholderText(/malicious-domain/i);
            fireEvent.change(domainInput, { target: { value: 'evil.com' } });

            expect(domainInput).toHaveValue('evil.com');
        });

        it('displays existing blocked URLs', async () => {
            render(<Admin />);
            await openBlockedTab();

            expect(await screen.findByText(/malicious\.com/)).toBeInTheDocument();
        });

        it('can unblock URL', async () => {
            render(<Admin />);
            await openBlockedTab();

            const unblockBtn = await screen.findByRole('button', { name: /unblock https:\/\/malicious\.com\/bad/i });
            fireEvent.click(unblockBtn);

            await waitFor(() => {
                expect(global.fetch).toHaveBeenCalledWith(
                    expect.stringContaining('/admin/blocked/links/1'),
                    expect.objectContaining({ method: 'DELETE' })
                );
            });
        });
    });

    describe('Users Tab', () => {
        const openUsersTab = async () => {
            const usersTab = await screen.findByRole('button', { name: /^users$/i });
            fireEvent.click(usersTab);
        };

        it('displays users table', async () => {
            render(<Admin />);
            await openUsersTab();

            expect(await screen.findByText('admin@example.com')).toBeInTheDocument();
        });

        it('requests paginated users', async () => {
            render(<Admin />);
            await openUsersTab();

            await waitFor(() => {
                expect(global.fetch).toHaveBeenCalledWith(
                    expect.stringMatching(/\/admin\/users\?.*page=1/),
                    expect.anything(),
                );
            });
        });

        it('displays per-user link and click counts', async () => {
            render(<Admin />);
            await openUsersTab();

            await screen.findByText('admin@example.com');
            expect(screen.getAllByText('42').length).toBeGreaterThan(0);
        });

        it('shows verification status', async () => {
            render(<Admin />);
            await openUsersTab();

            expect(await screen.findAllByText(/^verified$/i)).not.toHaveLength(0);
            // "Unverified" also appears as a filter option, so expect >= 2.
            expect(screen.getAllByText(/^unverified$/i).length).toBeGreaterThan(1);
        });

        it('shows deleted users', async () => {
            render(<Admin />);
            await openUsersTab();

            expect(await screen.findByText(/^deleted$/i)).toBeInTheDocument();
        });

        it('offers verify action for unverified users', async () => {
            render(<Admin />);
            await openUsersTab();

            expect(await screen.findByRole('button', { name: /verify/i })).toBeInTheDocument();
        });

        it('can make user admin', async () => {
            render(<Admin />);
            await openUsersTab();

            expect(await screen.findAllByRole('button', { name: /make admin/i })).not.toHaveLength(0);
        });

        it('can remove admin status', async () => {
            render(<Admin />);
            await openUsersTab();

            expect(await screen.findByRole('button', { name: /remove admin/i })).toBeInTheDocument();
        });

        it('can delete user', async () => {
            render(<Admin />);
            await openUsersTab();

            expect(await screen.findAllByRole('button', { name: /^delete$/i })).not.toHaveLength(0);
        });

        it('can restore deleted user', async () => {
            render(<Admin />);
            await openUsersTab();

            expect(await screen.findByRole('button', { name: /restore/i })).toBeInTheDocument();
        });

        it('can search users', async () => {
            render(<Admin />);
            await openUsersTab();

            const search = await screen.findByPlaceholderText(/search email/i);
            fireEvent.change(search, { target: { value: 'user@' } });

            await waitFor(() => {
                expect(global.fetch).toHaveBeenCalledWith(
                    expect.stringMatching(/\/admin\/users\?.*search=user%40/),
                    expect.anything(),
                );
            });
        });
    });

    describe('Links Tab', () => {
        const openLinksTab = async () => {
            const linksTab = await screen.findByRole('button', { name: /^links$/i });
            fireEvent.click(linksTab);
        };

        it('lists links from all users with owner emails', async () => {
            render(<Admin />);
            await openLinksTab();

            expect(await screen.findByText('/abc123')).toBeInTheDocument();
            expect(screen.getByText('user@example.com')).toBeInTheDocument();
        });

        it('shows destination URLs', async () => {
            render(<Admin />);
            await openLinksTab();

            expect(await screen.findByText(/example\.com\/some\/long\/path/)).toBeInTheDocument();
        });

        it('shows link status badges', async () => {
            render(<Admin />);
            await openLinksTab();

            // "Deleted" also appears as a filter option, so expect >= 2.
            expect((await screen.findAllByText(/^active$/i)).length).toBeGreaterThan(0);
            expect(screen.getAllByText(/^deleted$/i).length).toBeGreaterThan(1);
        });

        it('can delete a live link', async () => {
            vi.spyOn(window, 'confirm').mockReturnValue(true);
            render(<Admin />);
            await openLinksTab();

            const deleteBtns = await screen.findAllByRole('button', { name: /^delete$/i });
            fireEvent.click(deleteBtns[0]);

            await waitFor(() => {
                expect(global.fetch).toHaveBeenCalledWith(
                    expect.stringContaining('/admin/links/10'),
                    expect.objectContaining({ method: 'DELETE' })
                );
            });
        });

        it('can restore a deleted link', async () => {
            render(<Admin />);
            await openLinksTab();

            const restoreBtn = await screen.findByRole('button', { name: /restore/i });
            fireEvent.click(restoreBtn);

            await waitFor(() => {
                expect(global.fetch).toHaveBeenCalledWith(
                    expect.stringContaining('/admin/links/11/restore'),
                    expect.objectContaining({ method: 'POST' })
                );
            });
        });

        it('can search links', async () => {
            render(<Admin />);
            await openLinksTab();

            const search = await screen.findByPlaceholderText(/search code/i);
            fireEvent.change(search, { target: { value: 'abc' } });

            await waitFor(() => {
                expect(global.fetch).toHaveBeenCalledWith(
                    expect.stringMatching(/\/admin\/links\?.*search=abc/),
                    expect.anything(),
                );
            });
        });

        it('surfaces the suspicious flag and reason on malicious links', async () => {
            render(<Admin />);
            await openLinksTab();

            expect(await screen.findByText(/dangerous file type \(\.hta\), raw IP host/i)).toBeInTheDocument();
        });

        it('the suspicious-only toggle sets the query param', async () => {
            render(<Admin />);
            await openLinksTab();

            const toggle = await screen.findByRole('button', { name: /suspicious only/i });
            fireEvent.click(toggle);

            await waitFor(() => {
                expect(global.fetch).toHaveBeenCalledWith(
                    expect.stringMatching(/\/admin\/links\?.*suspicious=true/),
                    expect.anything(),
                );
            });
        });

        it('block domain action calls the block-domain endpoint', async () => {
            vi.spyOn(window, 'confirm').mockReturnValue(true);
            render(<Admin />);
            await openLinksTab();

            const blockBtns = await screen.findAllByRole('button', { name: /block domain/i });
            fireEvent.click(blockBtns[0]);

            await waitFor(() => {
                expect(global.fetch).toHaveBeenCalledWith(
                    expect.stringMatching(/\/admin\/links\/\d+\/block-domain/),
                    expect.objectContaining({ method: 'POST' }),
                );
            });
        });

        it('select-all then bulk delete calls the bulk endpoint', async () => {
            vi.spyOn(window, 'confirm').mockReturnValue(true);
            render(<Admin />);
            await openLinksTab();

            const selectAll = await screen.findByRole('checkbox', { name: /select all links/i });
            fireEvent.click(selectAll);

            const bulkDelete = await screen.findByRole('button', { name: /delete selected/i });
            fireEvent.click(bulkDelete);

            await waitFor(() => {
                expect(global.fetch).toHaveBeenCalledWith(
                    expect.stringContaining('/admin/links/bulk/delete'),
                    expect.objectContaining({ method: 'POST' }),
                );
            });
        });
    });

    describe('Organizations Tab', () => {
        it('lists organizations with owner and counts', async () => {
            render(<Admin />);

            const orgsTab = await screen.findByRole('button', { name: /organizations/i });
            fireEvent.click(orgsTab);

            expect(await screen.findByText('Acme Team')).toBeInTheDocument();
            expect(screen.getByText('acme-team')).toBeInTheDocument();
            expect(screen.getByText('user@example.com')).toBeInTheDocument();
        });
    });
});
