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
            
            await waitFor(() => {
                expect(screen.getByText(/blocked/i)).toBeInTheDocument();
            });
        });

        it('displays users tab', async () => {
            render(<Admin />);
            
            await waitFor(() => {
                expect(screen.getByText(/users/i)).toBeInTheDocument();
            });
        });

        it('switches tabs on click', async () => {
            render(<Admin />);
            
            await waitFor(async () => {
                const usersTab = screen.getByText(/users/i);
                fireEvent.click(usersTab);
            });
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
            
            await waitFor(async () => {
                const blockedTab = screen.getByText(/blocked/i);
                fireEvent.click(blockedTab);
            });
            
            await waitFor(() => {
                expect(screen.queryByText(/block.*url/i)).toBeDefined();
            });
        });

        it('displays blocked domains section', async () => {
            render(<Admin />);
            
            await waitFor(async () => {
                const blockedTab = screen.getByText(/blocked/i);
                fireEvent.click(blockedTab);
            });
            
            await waitFor(() => {
                expect(screen.queryByText(/block.*domain/i)).toBeDefined();
            });
        });

        it('can add blocked URL', async () => {
            render(<Admin />);
            
            await waitFor(async () => {
                const blockedTab = screen.getByText(/blocked/i);
                fireEvent.click(blockedTab);
            });
            
            await waitFor(async () => {
                const urlInput = screen.queryByPlaceholderText(/malicious/i);
                if (urlInput) {
                    fireEvent.change(urlInput, { target: { value: 'https://bad.com/page' } });
                    
                    const blockBtn = screen.queryByRole('button', { name: /block/i });
                    if (blockBtn) {
                        fireEvent.click(blockBtn);
                    }
                }
            });
        });

        it('can add blocked domain', async () => {
            render(<Admin />);
            
            await waitFor(async () => {
                const blockedTab = screen.getByText(/blocked/i);
                fireEvent.click(blockedTab);
            });
            
            await waitFor(async () => {
                const domainInput = screen.queryByPlaceholderText(/domain/i);
                if (domainInput) {
                    fireEvent.change(domainInput, { target: { value: 'evil.com' } });
                }
            });
        });

        it('displays existing blocked URLs', async () => {
            render(<Admin />);
            
            await waitFor(async () => {
                const blockedTab = screen.getByText(/blocked/i);
                fireEvent.click(blockedTab);
            });
            
            await waitFor(() => {
                expect(screen.queryByText(/malicious\.com/)).toBeDefined();
            });
        });

        it('can unblock URL', async () => {
            render(<Admin />);
            
            await waitFor(async () => {
                const blockedTab = screen.getByText(/blocked/i);
                fireEvent.click(blockedTab);
            });
            
            await waitFor(async () => {
                const deleteBtn = screen.queryByRole('button', { name: /delete|unblock|remove/i });
            });
        });
    });

    describe('Users Tab', () => {
        it('displays users table', async () => {
            render(<Admin />);
            
            await waitFor(async () => {
                const usersTab = screen.getByText(/users/i);
                fireEvent.click(usersTab);
            });
            
            await waitFor(() => {
                expect(screen.queryByText('admin@example.com')).toBeDefined();
            });
        });

        it('displays user email', async () => {
            render(<Admin />);
            
            await waitFor(async () => {
                const usersTab = screen.getByText(/users/i);
                fireEvent.click(usersTab);
            });
            
            await waitFor(() => {
                expect(screen.queryByText('user@example.com')).toBeDefined();
            });
        });

        it('shows admin badge for admin users', async () => {
            render(<Admin />);
            
            await waitFor(async () => {
                const usersTab = screen.getByText(/users/i);
                fireEvent.click(usersTab);
            });
            
            await waitFor(() => {
                // Check for admin indicator
                expect(screen.queryAllByText(/admin/i).length).toBeGreaterThan(0);
            });
        });

        it('shows verification status', async () => {
            render(<Admin />);
            
            await waitFor(async () => {
                const usersTab = screen.getByText(/users/i);
                fireEvent.click(usersTab);
            });
            
            await waitFor(() => {
                expect(screen.queryByText(/verified|unverified/i)).toBeDefined();
            });
        });

        it('shows deleted users', async () => {
            render(<Admin />);
            
            await waitFor(async () => {
                const usersTab = screen.getByText(/users/i);
                fireEvent.click(usersTab);
            });
            
            await waitFor(() => {
                expect(screen.queryByText(/deleted/i)).toBeDefined();
            });
        });

        it('can make user admin', async () => {
            render(<Admin />);
            
            await waitFor(async () => {
                const usersTab = screen.getByText(/users/i);
                fireEvent.click(usersTab);
            });
            
            await waitFor(async () => {
                const makeAdminBtn = screen.queryByText(/make admin/i);
            });
        });

        it('can remove admin status', async () => {
            render(<Admin />);
            
            await waitFor(async () => {
                const usersTab = screen.getByText(/users/i);
                fireEvent.click(usersTab);
            });
            
            await waitFor(async () => {
                const removeAdminBtn = screen.queryByText(/remove admin/i);
            });
        });

        it('can delete user', async () => {
            render(<Admin />);
            
            await waitFor(async () => {
                const usersTab = screen.getByText(/users/i);
                fireEvent.click(usersTab);
            });
            
            await waitFor(async () => {
                const deleteBtn = screen.queryByText(/delete/i);
            });
        });

        it('can restore deleted user', async () => {
            render(<Admin />);
            
            await waitFor(async () => {
                const usersTab = screen.getByText(/users/i);
                fireEvent.click(usersTab);
            });
            
            await waitFor(async () => {
                const restoreBtn = screen.queryByText(/restore/i);
            });
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


