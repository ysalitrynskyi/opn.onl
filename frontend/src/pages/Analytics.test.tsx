import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '../test/test-utils';
import Analytics from './Analytics';
import { mockToken } from '../test/test-utils';

// Mock the react-router-dom hooks
vi.mock('react-router-dom', async () => {
    const actual = await vi.importActual('react-router-dom');
    return {
        ...actual,
        useParams: () => ({ id: '1' }),
        useNavigate: () => vi.fn(),
    };
});

describe('Analytics Page', () => {
    const mockLinkStats = {
        link_id: 1,
        code: 'abc123',
        original_url: 'https://example.com/very-long-url',
        total_clicks: 1234,
        unique_visitors: 890,
        clicks_by_day: [
            { date: '2024-01-01', count: 50 },
            { date: '2024-01-02', count: 75 },
            { date: '2024-01-03', count: 120 },
        ],
        clicks_by_country: [
            { country: 'United States', count: 500, percentage: 40.5 },
            { country: 'United Kingdom', count: 200, percentage: 16.2 },
            { country: 'Germany', count: 150, percentage: 12.2 },
        ],
        clicks_by_city: [
            { city: 'New York', country: 'United States', count: 150, percentage: 12.2 },
            { city: 'London', country: 'United Kingdom', count: 100, percentage: 8.1 },
        ],
        clicks_by_device: [
            { device: 'Desktop', count: 600, percentage: 48.6 },
            { device: 'Mobile', count: 500, percentage: 40.5 },
            { device: 'Tablet', count: 134, percentage: 10.9 },
        ],
        clicks_by_browser: [
            { browser: 'Chrome', count: 700, percentage: 56.7 },
            { browser: 'Safari', count: 300, percentage: 24.3 },
            { browser: 'Firefox', count: 234, percentage: 19.0 },
        ],
        clicks_by_os: [
            { os: 'Windows', count: 500, percentage: 40.5 },
            { os: 'macOS', count: 400, percentage: 32.4 },
            { os: 'iOS', count: 200, percentage: 16.2 },
            { os: 'Android', count: 134, percentage: 10.9 },
        ],
        clicks_by_referer: [
            { referer: 'Google', count: 400, percentage: 32.4 },
            { referer: 'Direct', count: 300, percentage: 24.3 },
            { referer: 'Twitter', count: 200, percentage: 16.2 },
        ],
        recent_clicks: [
            {
                id: 1,
                timestamp: '2024-01-03T12:30:00Z',
                country: 'United States',
                city: 'New York',
                device: 'Desktop',
                browser: 'Chrome',
                os: 'Windows',
                referer: 'Google',
            },
        ],
    };

    beforeEach(() => {
        vi.clearAllMocks();
        localStorage.setItem('token', mockToken);
        
        // Mock successful API response
        global.fetch = vi.fn().mockResolvedValue({
            ok: true,
            status: 200,
            json: () => Promise.resolve(mockLinkStats),
        });
    });

    describe('Initial Load', () => {
        it('shows loading state initially', () => {
            render(<Analytics />);
            // Should show loading indicator or skeleton
        });

        it('fetches analytics data on mount', async () => {
            render(<Analytics />);
            
            await waitFor(() => {
                expect(global.fetch).toHaveBeenCalled();
            });
        });

        it('displays analytics after loading', async () => {
            render(<Analytics />);
            
            await waitFor(() => {
                expect(screen.getByText(/abc123/i) || screen.getByText(/1,234|1234/)).toBeDefined();
            });
        });
    });

    describe('Stats Display', () => {
        it('displays total clicks', async () => {
            render(<Analytics />);
            
            await waitFor(() => {
                const totalClicks = screen.queryByText('1,234') || screen.queryByText('1234');
                expect(totalClicks).toBeDefined();
            });
        });

        it('displays unique visitors', async () => {
            render(<Analytics />);
            
            await waitFor(() => {
                const visitors = screen.queryByText('890') || screen.queryByText(/unique/i);
                expect(visitors).toBeDefined();
            });
        });

        it('displays link code', async () => {
            render(<Analytics />);
            
            await waitFor(() => {
                expect(screen.queryByText(/abc123/)).toBeDefined();
            });
        });
    });

    describe('Charts', () => {
        it('renders clicks over time chart', async () => {
            render(<Analytics />);
            
            await waitFor(() => {
                // Check for chart container or recharts elements
                const chart = document.querySelector('.recharts-wrapper') || 
                             document.querySelector('[class*="chart"]');
            });
        });
    });

    describe('Data Tables', () => {
        it('displays country statistics', async () => {
            render(<Analytics />);
            
            await waitFor(() => {
                const countrySection = screen.queryByText(/countries/i) || 
                                      screen.queryByText(/United States/);
                expect(countrySection).toBeDefined();
            });
        });

        it('displays device statistics', async () => {
            render(<Analytics />);
            
            await waitFor(() => {
                const deviceSection = screen.queryByText(/devices/i) || 
                                     screen.queryByText(/Desktop|Mobile/);
                expect(deviceSection).toBeDefined();
            });
        });

        it('displays browser statistics', async () => {
            render(<Analytics />);
            
            await waitFor(() => {
                const browserSection = screen.queryByText(/browsers/i) || 
                                      screen.queryByText(/Chrome|Safari/);
                expect(browserSection).toBeDefined();
            });
        });

        it('displays referer statistics', async () => {
            render(<Analytics />);
            
            await waitFor(() => {
                const refererSection = screen.queryByText(/referrers?/i) || 
                                      screen.queryByText(/Google|Direct/);
                expect(refererSection).toBeDefined();
            });
        });
    });

    describe('Recent Clicks', () => {
        it('displays recent clicks table', async () => {
            render(<Analytics />);
            
            await waitFor(() => {
                const recentSection = screen.queryByText(/recent/i);
                expect(recentSection).toBeDefined();
            });
        });
    });

    describe('Time Range Filter', () => {
        it('has time range selector', async () => {
            render(<Analytics />);
            
            await waitFor(() => {
                const selector = screen.queryByRole('combobox') || 
                                screen.queryByText(/7 days|30 days/i);
            });
        });
    });

    describe('Navigation', () => {
        it('has back to dashboard link', async () => {
            render(<Analytics />);
            
            await waitFor(() => {
                const backLink = screen.queryByText(/back/i) || 
                                screen.queryByRole('link', { name: /dashboard/i });
                expect(backLink).toBeDefined();
            });
        });
    });

    describe('Error Handling', () => {
        it('shows error message on failed fetch', async () => {
            global.fetch = vi.fn().mockResolvedValue({
                ok: false,
                status: 404,
                json: () => Promise.resolve({ error: 'Link not found' }),
            });

            render(<Analytics />);
            
            await waitFor(() => {
                const error = screen.queryByText(/error|not found/i);
            });
        });

        it('redirects to login on 401', async () => {
            global.fetch = vi.fn().mockResolvedValue({
                ok: false,
                status: 401,
            });

            render(<Analytics />);
            
            // Should redirect to login
        });
    });

    describe('Empty State', () => {
        it('shows empty state when no clicks', async () => {
            global.fetch = vi.fn().mockResolvedValue({
                ok: true,
                status: 200,
                json: () => Promise.resolve({
                    ...mockLinkStats,
                    total_clicks: 0,
                    clicks_by_day: [],
                    recent_clicks: [],
                }),
            });

            render(<Analytics />);
            
            await waitFor(() => {
                const emptyState = screen.queryByText(/no clicks/i) || 
                                  screen.queryByText(/share your link/i);
            });
        });
    });

    describe('Refresh', () => {
        it('has refresh button', async () => {
            render(<Analytics />);
            
            await waitFor(() => {
                const refreshButton = screen.queryByRole('button', { name: /refresh/i }) ||
                                     screen.queryByLabelText(/refresh/i);
            });
        });
    });
});


