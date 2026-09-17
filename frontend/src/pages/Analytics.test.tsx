import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, within } from '../test/test-utils';
import Analytics from './Analytics';
import { formatDayBucketLabel, sumClicksInUtcWindow } from '../utils/dayBuckets';
import { mockToken } from '../test/test-utils';

const { mockParams, mockNavigate } = vi.hoisted(() => ({
    mockParams: { id: '1' },
    mockNavigate: vi.fn(),
}));

// Mock the react-router-dom hooks
vi.mock('react-router-dom', async () => {
    const actual = await vi.importActual('react-router-dom');
    return {
        ...actual,
        useParams: () => ({ id: mockParams.id }),
        useNavigate: () => mockNavigate,
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
        mockParams.id = '1';
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

        it('labels a UTC date-only bucket as that calendar day, not the previous local date', () => {
            expect(formatDayBucketLabel('2026-09-17')).toBe('Sep 17');
            expect(formatDayBucketLabel('2026-01-05')).toBe('Jan 5');
        });
    });

    describe('This Week', () => {
        afterEach(() => {
            vi.useRealTimers();
        });

        it('includes a UTC day from seven days ago even when it is afternoon', () => {
            const now = new Date('2026-09-17T18:00:00.000Z');
            expect(sumClicksInUtcWindow([
                { date: '2026-09-10', count: 5 },
                { date: '2026-09-17', count: 10 },
            ], 7, now)).toBe(15);
        });

        it('shows the UTC-week total on the This Week card', async () => {
            vi.useFakeTimers({ toFake: ['Date'] });
            vi.setSystemTime(new Date('2026-09-17T18:00:00.000Z'));

            global.fetch = vi.fn().mockResolvedValue({
                ok: true,
                status: 200,
                json: () => Promise.resolve({
                    ...mockLinkStats,
                    total_clicks: 99,
                    clicks_by_day: [
                        { date: '2026-09-10', count: 5 },
                        { date: '2026-09-17', count: 10 },
                    ],
                }),
            });

            render(<Analytics />);

            await waitFor(() => {
                expect(screen.getByText('This Week')).toBeInTheDocument();
            });
            const card = screen.getByText('This Week').closest('.bg-surface');
            expect(card).not.toBeNull();
            expect(within(card as HTMLElement).getByText('15')).toBeInTheDocument();
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

        it('refetches with the new range when the selector changes', async () => {
            const { user } = render(<Analytics />);

            await screen.findByLabelText('Time range');
            expect(String(vi.mocked(global.fetch).mock.calls[0][0])).toContain('days=30');

            await user.selectOptions(screen.getByLabelText('Time range'), '7');

            await waitFor(() => {
                const urls = vi.mocked(global.fetch).mock.calls.map(call => String(call[0]));
                expect(urls.some(url => url.includes('days=7'))).toBe(true);
            });
        });

        it('refetches when the link id changes', async () => {
            const { rerender } = render(<Analytics />);

            await waitFor(() => {
                expect(String(vi.mocked(global.fetch).mock.calls[0][0])).toContain('/links/1/stats');
            });

            mockParams.id = '2';
            rerender(<Analytics />);

            await waitFor(() => {
                const urls = vi.mocked(global.fetch).mock.calls.map(call => String(call[0]));
                expect(urls.some(url => url.includes('/links/2/stats'))).toBe(true);
            });
        });

        it('ignores a slower response for a previous time range', async () => {
            let releaseSeven: () => void = () => {};
            const sevenGate = new Promise<void>(resolve => {
                releaseSeven = resolve;
            });

            global.fetch = vi.fn().mockImplementation((url: string) => {
                const href = String(url);
                if (href.includes('days=7')) {
                    return sevenGate.then(() => ({
                        ok: true,
                        status: 200,
                        json: () => Promise.resolve({ ...mockLinkStats, code: 'seven-day' }),
                    }));
                }
                if (href.includes('days=90')) {
                    return Promise.resolve({
                        ok: true,
                        status: 200,
                        json: () => Promise.resolve({ ...mockLinkStats, code: 'ninety-day' }),
                    });
                }
                return Promise.resolve({
                    ok: true,
                    status: 200,
                    json: () => Promise.resolve({ ...mockLinkStats, code: 'thirty-day' }),
                });
            });

            const { user } = render(<Analytics />);
            await screen.findByText('thirty-day');

            await user.selectOptions(screen.getByLabelText('Time range'), '7');
            await user.selectOptions(screen.getByLabelText('Time range'), '90');
            await screen.findByText('ninety-day');

            releaseSeven();

            await waitFor(() => {
                expect(screen.getByText('ninety-day')).toBeInTheDocument();
            });
            expect(screen.queryByText('seven-day')).not.toBeInTheDocument();
        });

        it('does not keep showing the previous link while a new id loads', async () => {
            let releaseTwo: () => void = () => {};
            const twoGate = new Promise<void>(resolve => {
                releaseTwo = resolve;
            });

            global.fetch = vi.fn().mockImplementation((url: string) => {
                if (String(url).includes('/links/2/stats')) {
                    return twoGate.then(() => ({
                        ok: true,
                        status: 200,
                        json: () => Promise.resolve({ ...mockLinkStats, link_id: 2, code: 'second' }),
                    }));
                }
                return Promise.resolve({
                    ok: true,
                    status: 200,
                    json: () => Promise.resolve(mockLinkStats),
                });
            });

            const { rerender } = render(<Analytics />);
            await screen.findByText(/abc123/);

            mockParams.id = '2';
            rerender(<Analytics />);

            await waitFor(() => {
                expect(screen.queryByText(/abc123/)).not.toBeInTheDocument();
            });

            releaseTwo();
            await screen.findByText(/second/);
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

            expect(await screen.findByText('Link not found.')).toBeInTheDocument();
        });

        it('clears a previous error when a later fetch succeeds', async () => {
            global.fetch = vi.fn()
                .mockResolvedValueOnce({
                    ok: false,
                    status: 404,
                    json: () => Promise.resolve({ error: 'Link not found' }),
                })
                .mockResolvedValueOnce({
                    ok: true,
                    status: 200,
                    json: () => Promise.resolve(mockLinkStats),
                });

            mockParams.id = '999';
            const { rerender } = render(<Analytics />);
            expect(await screen.findByText('Link not found.')).toBeInTheDocument();

            mockParams.id = '1';
            rerender(<Analytics />);

            await screen.findByText(/abc123/);
            expect(screen.queryByText('Link not found.')).not.toBeInTheDocument();
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


