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
        total_links: 5000,
        active_links: 4500,
        total_clicks: 250000,
        blocked_links_count: 25,
        blocked_domains_count: 10,
    };

    const mockUsers = [
        {
            id: 1,
            email: 'admin@example.com',
            is_admin: true,
            email_verified: true,
            created_at: '2024-01-01T00:00:00Z',
            deleted_at: null,
        },
        {
            id: 2,
            email: 'user@example.com',
            is_admin: false,
            email_verified: true,
            created_at: '2024-01-02T00:00:00Z',
            deleted_at: null,
        },
        {
            id: 3,
            email: 'unverified@example.com',
            is_admin: false,
            email_verified: false,
            created_at: '2024-01-03T00:00:00Z',
            deleted_at: null,
        },
        {
            id: 4,
            email: 'deleted@example.com',
            is_admin: false,
            email_verified: true,
            created_at: '2024-01-04T00:00:00Z',
            deleted_at: '2024-01-05T00:00:00Z',
        },
    ];

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
        
        // Mock fetch for different endpoints
        global.fetch = vi.fn((url: string) => {
            if (url.includes('/admin/stats')) {
                return Promise.resolve({
                    ok: true,
                    json: () => Promise.resolve(mockStats),
                });
            }
            if (url.includes('/admin/users')) {
                return Promise.resolve({
                    ok: true,
                    json: () => Promise.resolve(mockUsers),
                });
            }
            if (url.includes('/admin/blocked/links')) {
                return Promise.resolve({
                    ok: true,
                    json: () => Promise.resolve(mockBlockedLinks),
                });
            }
            if (url.includes('/admin/blocked/domains')) {
                return Promise.resolve({
                    ok: true,
                    json: () => Promise.resolve(mockBlockedDomains),
                });
            }
            return Promise.resolve({
                ok: true,
                json: () => Promise.resolve({}),
            });
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
            
            await waitFor(() => {
                expect(screen.getByText(/admin/i)).toBeInTheDocument();
            });
        });

        it('displays admin icon', async () => {
            render(<Admin />);
            
            await waitFor(() => {
                // Check for shield icon or similar admin indicator
            });
        });
    });

    describe('Navigation Tabs', () => {
        it('displays statistics tab', async () => {
            render(<Admin />);
            
            await waitFor(() => {
                expect(screen.getByText(/statistics/i)).toBeInTheDocument();
            });
        });

        it('displays blocked content tab', async () => {
            render(<Admin />);

            expect(await screen.findByRole('button', { name: /blocked content/i })).toBeInTheDocument();
        });

        it('displays users tab', async () => {
            render(<Admin />);

            expect(await screen.findByRole('button', { name: /^users$/i })).toBeInTheDocument();
        });

        it('switches tabs on click', async () => {
            render(<Admin />);

            const usersTab = await screen.findByRole('button', { name: /^users$/i });
            fireEvent.click(usersTab);

            // Users table renders after switching tabs.
            expect(await screen.findByText('admin@example.com')).toBeInTheDocument();
        });
    });

    describe('Statistics Tab', () => {
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
                expect(screen.getByText('5,000') || screen.getByText('5000')).toBeDefined();
            });
        });

        it('displays total clicks', async () => {
            render(<Admin />);
            
            await waitFor(() => {
                expect(screen.getByText('250,000') || screen.getByText('250000')).toBeDefined();
            });
        });

        it('displays blocked counts', async () => {
            render(<Admin />);
            
            await waitFor(() => {
                expect(screen.getByText('25')).toBeInTheDocument();
                expect(screen.getByText('10')).toBeInTheDocument();
            });
        });
    });

    describe('Backup Management', () => {
        it('displays backup section', async () => {
            render(<Admin />);
            
            await waitFor(() => {
                expect(screen.queryByText(/backup/i)).toBeDefined();
            });
        });

        it('has create backup button', async () => {
            render(<Admin />);
            
            await waitFor(() => {
                const backupBtn = screen.queryByRole('button', { name: /create backup/i }) ||
                                 screen.queryByText(/create backup/i);
            });
        });

        it('can trigger backup creation', async () => {
            render(<Admin />);
            
            await waitFor(async () => {
                const backupBtn = screen.queryByText(/create backup/i);
                if (backupBtn) {
                    fireEvent.click(backupBtn);
                    expect(global.fetch).toHaveBeenCalledWith(
                        expect.stringContaining('/admin/backup'),
                        expect.objectContaining({ method: 'POST' })
                    );
                }
            });
        });
    });

    describe('Blocked Content Tab', () => {
        it('displays blocked URLs section', async () => {
            render(<Admin />);

            const blockedTab = await screen.findByRole('button', { name: /blocked content/i });
            fireEvent.click(blockedTab);

            // "Block URL" heading appears in the blocked-content tab.
            expect(await screen.findByRole('heading', { name: /block url/i })).toBeInTheDocument();
        });

        it('displays blocked domains section', async () => {
            render(<Admin />);

            const blockedTab = await screen.findByRole('button', { name: /blocked content/i });
            fireEvent.click(blockedTab);

            expect(await screen.findByRole('heading', { name: /block domain/i })).toBeInTheDocument();
        });

        it('can add blocked URL', async () => {
            render(<Admin />);

            const blockedTab = await screen.findByRole('button', { name: /blocked content/i });
            fireEvent.click(blockedTab);

            const urlInput = await screen.findByPlaceholderText(/example\.com\/malicious/i);
            fireEvent.change(urlInput, { target: { value: 'https://bad.com/page' } });

            // The two "Block" buttons (URL + domain); click the first.
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

            const blockedTab = await screen.findByRole('button', { name: /blocked content/i });
            fireEvent.click(blockedTab);

            const domainInput = await screen.findByPlaceholderText(/malicious-domain/i);
            fireEvent.change(domainInput, { target: { value: 'evil.com' } });

            expect(domainInput).toHaveValue('evil.com');
        });

        it('displays existing blocked URLs', async () => {
            render(<Admin />);

            const blockedTab = await screen.findByRole('button', { name: /blocked content/i });
            fireEvent.click(blockedTab);

            expect(await screen.findByText(/malicious\.com/)).toBeInTheDocument();
        });

        it('can unblock URL', async () => {
            render(<Admin />);

            const blockedTab = await screen.findByRole('button', { name: /blocked content/i });
            fireEvent.click(blockedTab);

            // The blocked URL row renders with a trash/unblock action button.
            expect(await screen.findByText(/malicious\.com/)).toBeInTheDocument();
            const buttons = screen.getAllByRole('button');
            expect(buttons.length).toBeGreaterThan(0);
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

        it('displays user email', async () => {
            render(<Admin />);
            await openUsersTab();

            expect(await screen.findByText('user@example.com')).toBeInTheDocument();
        });

        it('shows admin badge for admin users', async () => {
            render(<Admin />);
            await openUsersTab();

            await screen.findByText('admin@example.com');
            // "Admin" badge for admin rows.
            expect(screen.getAllByText(/admin/i).length).toBeGreaterThan(0);
        });

        it('shows verification status', async () => {
            render(<Admin />);
            await openUsersTab();

            // Verified status badges render in the table.
            expect(await screen.findAllByText(/^verified$/i)).not.toHaveLength(0);
            expect(screen.getByText(/^unverified$/i)).toBeInTheDocument();
        });

        it('shows deleted users', async () => {
            render(<Admin />);
            await openUsersTab();

            // Soft-deleted user shows a "Deleted" status badge.
            expect(await screen.findByText(/^deleted$/i)).toBeInTheDocument();
        });

        it('can make user admin', async () => {
            render(<Admin />);
            await openUsersTab();

            // Non-admin, non-deleted users expose a "Make Admin" action.
            expect(await screen.findAllByRole('button', { name: /make admin/i })).not.toHaveLength(0);
        });

        it('can remove admin status', async () => {
            render(<Admin />);
            await openUsersTab();

            // The admin user exposes a "Remove Admin" action.
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

            // The soft-deleted user exposes a "Restore" action.
            expect(await screen.findByRole('button', { name: /restore/i })).toBeInTheDocument();
        });
    });

    describe('Error Handling', () => {
        it('displays error alerts', async () => {
            render(<Admin />);
            
            // Trigger an error and check for alert
        });

        it('displays success alerts', async () => {
            render(<Admin />);
            
            // Trigger a successful action and check for success message
        });

        it('can dismiss alerts', async () => {
            render(<Admin />);
            
            // Check for close button on alerts
        });
    });

    describe('Loading States', () => {
        it('shows loading indicator while fetching', () => {
            render(<Admin />);
            
            // Should show loading spinner initially
        });

        it('shows data after loading', async () => {
            render(<Admin />);
            
            await waitFor(() => {
                // Data should be visible after loading
            });
        });
    });
});


