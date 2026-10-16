import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor, within, mockToken } from '../test/test-utils';
import Overview from './Overview';
import { API_ENDPOINTS } from '../config/api';

describe('Overview page', () => {
    const stats = {
        total_links: 12,
        total_clicks: 3400,
        active_links: 9,
        clicks_today: 4,
        clicks_this_week: 28,
        clicks_this_month: 300,
        top_links: [
            { id: 7, code: 'abc123', original_url: 'https://example.com/docs', click_count: 80 },
        ],
        clicks_by_day: [
            { date: '2026-10-16', count: 4 },
        ],
        top_countries: [
            { country: 'United States', count: 10, percentage: 40.5 },
        ],
        top_browsers: [
            { browser: 'Chrome', count: 6, percentage: 24 },
        ],
    };

    beforeEach(() => {
        vi.clearAllMocks();
        localStorage.setItem('token', mockToken);
        global.fetch = vi.fn().mockResolvedValue({
            ok: true,
            status: 200,
            json: () => Promise.resolve(stats),
        });
    });

    afterEach(() => {
        vi.useRealTimers();
        window.history.pushState({}, '', '/');
    });

    function card(title: string): HTMLElement {
        const found = screen.getByText(title).closest('.bg-surface');
        expect(found).not.toBeNull();
        return found as HTMLElement;
    }

    it('renders totals from GET /analytics/dashboard', async () => {
        render(<Overview />);

        expect(await screen.findByRole('heading', { level: 1, name: 'Overview' })).toBeInTheDocument();
        expect(within(card('Total Links')).getByText('12')).toBeInTheDocument();
        expect(within(card('Active Links')).getByText('9')).toBeInTheDocument();
        expect(within(card('Total Clicks')).getByText('3,400')).toBeInTheDocument();
        expect(within(card('Today')).getByText('4')).toBeInTheDocument();
        expect(within(card('Today')).getByText('UTC day')).toBeInTheDocument();
        expect(within(card('Last 7 Days')).getByText('28')).toBeInTheDocument();
        expect(within(card('Last 30 Days')).getByText('300')).toBeInTheDocument();

        const topLink = screen.getByRole('link', { name: /abc123/ });
        expect(topLink).toHaveAttribute('href', '/analytics/7');
        expect(topLink).toHaveAccessibleName('abc123 https://example.com/docs 80 clicks');
        expect(screen.getByText('https://example.com/docs')).toBeInTheDocument();
        expect(screen.getByRole('heading', { name: 'Clicks over time' }).parentElement).toHaveTextContent('Last 30 days');
        expect(screen.getByRole('heading', { name: 'Top Links' }).parentElement).toHaveTextContent('All-time');
        expect(screen.getByRole('heading', { name: 'Top Countries' }).parentElement).toHaveTextContent('Last 30 days');
        expect(screen.getByRole('heading', { name: 'Browsers' }).parentElement).toHaveTextContent('Last 30 days');
        expect(screen.getAllByText('Last 30 days')).toHaveLength(3);
        expect(screen.getByText('United States')).toBeInTheDocument();
        expect(screen.getByText('40.5%')).toBeInTheDocument();
        expect(screen.getByText('Chrome')).toBeInTheDocument();
        expect(screen.getByText('24.0%')).toBeInTheDocument();

        expect(global.fetch).toHaveBeenCalledTimes(1);
        const [url, init] = vi.mocked(global.fetch).mock.calls[0];
        expect(url).toBe(API_ENDPOINTS.dashboardStats);
        expect(init).toEqual(expect.objectContaining({
            headers: expect.objectContaining({ Authorization: `Bearer ${mockToken}` }),
        }));
        expect((init as RequestInit).method).toBeUndefined();
        expect((init as RequestInit).body).toBeUndefined();
    });

    it('fills gaps between the server days and does not use the browser clock', async () => {
        // A clock a month ahead of the payload used to drop Sep 15 and invent
        // days through November. The series is the payload's own span.
        vi.useFakeTimers({ toFake: ['Date'] });
        vi.setSystemTime(new Date('2026-11-01T00:00:30.000Z'));
        global.fetch = vi.fn().mockResolvedValue({
            ok: true,
            status: 200,
            json: () => Promise.resolve({
                ...stats,
                clicks_by_day: [
                    { date: '2026-09-15', count: 9 },
                    { date: '2026-10-14', count: 2 },
                    { date: '2026-10-16', count: 4 },
                    { date: '2026-10-17', count: 1 },
                ],
            }),
        });

        render(<Overview />);

        const table = await screen.findByRole('table', { name: 'Clicks by day' });
        const headers = within(table).getAllByRole('rowheader');
        // Sep 15 through Oct 17 inclusive.
        expect(headers).toHaveLength(33);
        expect(headers[0]).toHaveTextContent('Sep 15');
        expect(headers[headers.length - 1]).toHaveTextContent('Oct 17');
        expect(within(table).queryByRole('rowheader', { name: 'Sep 14' })).not.toBeInTheDocument();
        expect(within(table).queryByRole('rowheader', { name: 'Oct 18' })).not.toBeInTheDocument();

        const cell = (label: string) => {
            const header = within(table).getByRole('rowheader', { name: label });
            return within(header.closest('tr') as HTMLElement).getByRole('cell');
        };
        expect(cell('Sep 15')).toHaveTextContent('9');
        expect(cell('Sep 16')).toHaveTextContent('0');
        expect(cell('Oct 14')).toHaveTextContent('2');
        expect(cell('Oct 15')).toHaveTextContent('0');
        expect(cell('Oct 16')).toHaveTextContent('4');
        expect(cell('Oct 17')).toHaveTextContent('1');
    });

    it('shows an empty state for an account with no links', async () => {
        global.fetch = vi.fn().mockResolvedValue({
            ok: true,
            status: 200,
            json: () => Promise.resolve({
                ...stats,
                total_links: 0,
                total_clicks: 0,
                active_links: 0,
                clicks_today: 0,
                clicks_this_week: 0,
                clicks_this_month: 0,
                top_links: [],
                clicks_by_day: [],
                top_countries: [],
                top_browsers: [],
            }),
        });

        render(<Overview />);

        expect(await screen.findByRole('heading', { name: 'No links yet' })).toBeInTheDocument();
        expect(screen.getByText('Create a short link and its clicks will show up here.')).toBeInTheDocument();
        expect(screen.getByRole('link', { name: 'Go to Dashboard' })).toHaveAttribute('href', '/dashboard');
        expect(within(card('Total Links')).getByText('0')).toBeInTheDocument();
        expect(screen.queryByRole('heading', { name: 'Clicks over time' })).not.toBeInTheDocument();
        expect(screen.queryByRole('heading', { name: 'Top Links' })).not.toBeInTheDocument();
        expect(screen.queryByText('All-time')).not.toBeInTheDocument();
        expect(screen.queryByText('Last 30 days')).not.toBeInTheDocument();
        expect(screen.queryByRole('table', { name: 'Clicks by day' })).not.toBeInTheDocument();
    });

    it('shows an error when the dashboard request fails', async () => {
        global.fetch = vi.fn().mockResolvedValue({
            ok: false,
            status: 500,
            json: () => Promise.resolve({ error: 'Database error' }),
        });

        render(<Overview />);

        expect(await screen.findByText('Failed to load overview.')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Retry' })).toBeInTheDocument();
        expect(screen.queryByText('Total Clicks')).not.toBeInTheDocument();
        expect(screen.queryByText('Database error')).not.toBeInTheDocument();
    });

    it('retries a failed overview load', async () => {
        global.fetch = vi.fn()
            .mockResolvedValueOnce({
                ok: false,
                status: 500,
                json: () => Promise.resolve({ error: 'Database error' }),
            })
            .mockResolvedValueOnce({
                ok: true,
                status: 200,
                json: () => Promise.resolve(stats),
            });

        const { user } = render(<Overview />);

        await user.click(await screen.findByRole('button', { name: 'Retry' }));

        expect(await screen.findByRole('heading', { level: 1, name: 'Overview' })).toBeInTheDocument();
        expect(screen.queryByText('Failed to load overview.')).not.toBeInTheDocument();
        expect(global.fetch).toHaveBeenCalledTimes(2);
        for (const [url, init] of vi.mocked(global.fetch).mock.calls) {
            expect(url).toBe(API_ENDPOINTS.dashboardStats);
            expect((init as RequestInit | undefined)?.method).toBeUndefined();
            expect((init as RequestInit | undefined)?.body).toBeUndefined();
        }
    });

    it('shows a network error when the request never answers', async () => {
        global.fetch = vi.fn().mockRejectedValue(new Error('offline'));

        render(<Overview />);

        expect(await screen.findByText('Network error. Please try again.')).toBeInTheDocument();
    });

    it('says the session ended instead of a network error when the stored session is rejected', async () => {
        global.fetch = vi.fn().mockResolvedValue({
            ok: false,
            status: 401,
            json: () => Promise.resolve({ error: 'Invalid or expired credentials' }),
        });

        render(<Overview />);

        expect(await screen.findByText('Session expired. Please log in again.')).toBeInTheDocument();
        expect(screen.queryByText(/network error/i)).not.toBeInTheDocument();
        expect(screen.queryByText('Failed to load overview.')).not.toBeInTheDocument();
    });

    it('sends a signed-out visitor to login without requesting stats', async () => {
        localStorage.removeItem('token');

        render(<Overview />);

        await waitFor(() => {
            expect(window.location.pathname).toBe('/login');
        });
        expect(global.fetch).not.toHaveBeenCalled();
    });
});
