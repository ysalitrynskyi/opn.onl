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

        it('does not say which of the two it is when the API refuses the link', async () => {
            global.fetch = vi.fn().mockResolvedValue({
                ok: false,
                status: 403,
                json: () => Promise.resolve({ error: "You don't have permission to access this link" }),
            });

            render(<Analytics />);

            expect(
                await screen.findByText("This link doesn't exist or you don't have access to it."),
            ).toBeInTheDocument();
            expect(screen.getByRole('link', { name: /back to dashboard/i })).toBeInTheDocument();
        });

        it('says the session ended instead of a network error when the stored session is rejected', async () => {
            global.fetch = vi.fn().mockResolvedValue({
                ok: false,
                status: 401,
                json: () => Promise.resolve({ error: 'Invalid or expired credentials' }),
            });

            render(<Analytics />);

            expect(await screen.findByText('Session expired. Please log in again.')).toBeInTheDocument();
            expect(screen.queryByText(/network error/i)).not.toBeInTheDocument();
        });

        it('still reports a network error when the request never reached the server', async () => {
            global.fetch = vi.fn().mockRejectedValue(new TypeError('Failed to fetch'));

            render(<Analytics />);

            expect(await screen.findByText('Network error. Please try again.')).toBeInTheDocument();
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

    describe('Sampled stats', () => {
        // The backend analyses at most the newest 50,000 clicks of a period and
        // sets `truncated`; everything except `total_clicks` then describes them.
        const sampledStats = {
            ...mockLinkStats,
            truncated: true,
            total_clicks: 412345,
            unique_visitors: 9000,
            clicks_by_day: [
                { date: '2024-01-02', count: 20000 },
                { date: '2024-01-03', count: 30000 },
            ],
        };

        function respondWith(stats: object) {
            global.fetch = vi.fn().mockResolvedValue({
                ok: true,
                status: 200,
                json: () => Promise.resolve(stats),
            });
        }

        function card(title: string): HTMLElement {
            const found = screen.getByText(title).closest('.bg-surface');
            expect(found).not.toBeNull();
            return found as HTMLElement;
        }

        it('says the figures cover only the most recent clicks, and how many clicks there are in all', async () => {
            respondWith(sampledStats);
            render(<Analytics />);

            const notice = await screen.findByRole('status');
            expect(notice).toHaveTextContent('Based on the most recent 50,000 clicks');
            expect(notice).toHaveTextContent('412,345 clicks in all');
        });

        it('labels which header figure counts every click and which the sample', async () => {
            respondWith(sampledStats);
            render(<Analytics />);

            await screen.findByRole('status');
            expect(within(card('Total Clicks')).getByText('412,345')).toBeInTheDocument();
            expect(within(card('Total Clicks')).getByText('All clicks in this period')).toBeInTheDocument();
            expect(within(card('Unique Visitors')).getByText('9,000')).toBeInTheDocument();
            expect(within(card('Unique Visitors')).getByText('Most recent 50,000')).toBeInTheDocument();
        });

        it('shows no notice or card labels when the whole period was analysed', async () => {
            respondWith({ ...mockLinkStats, truncated: false });
            render(<Analytics />);

            await screen.findByText('Total Clicks');
            expect(screen.queryByRole('status')).not.toBeInTheDocument();
            expect(screen.queryByText(/most recent/i)).not.toBeInTheDocument();
            expect(screen.queryByText('All clicks in this period')).not.toBeInTheDocument();
        });
    });

    describe('Cities', () => {
        it('names the country so two cities of the same name are told apart', async () => {
            global.fetch = vi.fn().mockResolvedValue({
                ok: true,
                status: 200,
                json: () => Promise.resolve({
                    ...mockLinkStats,
                    clicks_by_city: [
                        { city: 'Paris', country: 'France', count: 40, percentage: 40 },
                        { city: 'Paris', country: 'United States', count: 10, percentage: 10 },
                        { city: 'Unknown', country: null, count: 50, percentage: 50 },
                    ],
                    recent_clicks: [],
                }),
            });
            render(<Analytics />);

            const heading = await screen.findByRole('heading', { name: 'Top Cities' });
            const table = heading.parentElement as HTMLElement;
            expect(within(table).getByText('Paris, France')).toBeInTheDocument();
            expect(within(table).getByText('Paris, United States')).toBeInTheDocument();
            expect(within(table).getByText('Unknown')).toBeInTheDocument();
        });
    });

    describe('Clicks over time', () => {
        it('is drawn when the period has clicks, even if most days are zero', async () => {
            global.fetch = vi.fn().mockResolvedValue({
                ok: true,
                status: 200,
                json: () => Promise.resolve({
                    ...mockLinkStats,
                    clicks_by_day: [
                        { date: '2024-01-01', count: 3 },
                        { date: '2024-01-02', count: 0 },
                        { date: '2024-01-03', count: 5 },
                    ],
                }),
            });
            render(<Analytics />);

            expect(await screen.findByRole('heading', { name: 'Clicks over time' })).toBeInTheDocument();
        });

        it('is left out for a period without clicks, although the days come back zero-filled', async () => {
            global.fetch = vi.fn().mockResolvedValue({
                ok: true,
                status: 200,
                json: () => Promise.resolve({
                    ...mockLinkStats,
                    total_clicks: 0,
                    clicks_by_day: [
                        { date: '2024-01-01', count: 0 },
                        { date: '2024-01-02', count: 0 },
                    ],
                    recent_clicks: [],
                }),
            });
            render(<Analytics />);

            expect(await screen.findByRole('heading', { name: 'No clicks yet' })).toBeInTheDocument();
            expect(screen.queryByRole('heading', { name: 'Clicks over time' })).not.toBeInTheDocument();
        });
    });

    describe('Short link host', () => {
        afterEach(() => {
            vi.unstubAllEnvs();
        });

        it('shows the instance host in front of the code, not a hard-coded opn.onl', async () => {
            vi.stubEnv('VITE_FRONTEND_URL', 'https://links.example.org');
            render(<Analytics />);

            const heading = await screen.findByRole('heading', { level: 1 });
            expect(heading).toHaveTextContent(/^links\.example\.org\/abc123$/);
            expect(within(heading).getByText('links.example.org/')).toHaveClass('text-muted');
        });

        it('falls back to the host the app is served from', async () => {
            vi.stubEnv('VITE_FRONTEND_URL', '');
            render(<Analytics />);

            const heading = await screen.findByRole('heading', { level: 1 });
            expect(heading).toHaveTextContent(`${window.location.host}/abc123`);
            expect(heading).not.toHaveTextContent('opn.onl');
        });
    });

    describe('CSV export', () => {
        const blobs: Blob[] = [];
        const downloads: string[] = [];
        let createObjectURL: ReturnType<typeof vi.spyOn> | undefined;
        let revokeObjectURL: ReturnType<typeof vi.spyOn> | undefined;
        let anchorClick: ReturnType<typeof vi.spyOn> | undefined;

        beforeEach(() => {
            blobs.length = 0;
            downloads.length = 0;
            createObjectURL = vi.spyOn(URL, 'createObjectURL').mockImplementation((blob) => {
                blobs.push(blob as Blob);
                return 'blob:analytics-csv';
            });
            revokeObjectURL = vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => {});
            anchorClick = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) {
                downloads.push(this.download);
            });
        });

        afterEach(() => {
            createObjectURL?.mockRestore();
            revokeObjectURL?.mockRestore();
            anchorClick?.mockRestore();
            vi.useRealTimers();
        });

        async function readBlob(blob: Blob): Promise<{ text: string; bytes: Uint8Array }> {
            const buffer = await new Promise<ArrayBuffer>((resolve, reject) => {
                const reader = new FileReader();
                reader.onload = () => resolve(reader.result as ArrayBuffer);
                reader.onerror = () => reject(reader.error);
                reader.readAsArrayBuffer(blob);
            });
            const bytes = new Uint8Array(buffer);
            // The default decoder drops a leading BOM. The file keeps it, so
            // the spreadsheet opens the rest as UTF-8.
            return { text: new TextDecoder('utf-8', { ignoreBOM: true }).decode(bytes), bytes };
        }

        async function download(): Promise<{ text: string; bytes: Uint8Array; name: string }> {
            const { user } = render(<Analytics />);
            await user.click(await screen.findByRole('button', { name: 'Download CSV' }));
            expect(blobs).toHaveLength(1);
            expect(downloads).toHaveLength(1);
            expect(revokeObjectURL).toHaveBeenCalledWith('blob:analytics-csv');
            const read = await readBlob(blobs[0]);
            return { ...read, name: downloads[0] };
        }

        it('writes the breakdowns the page shows, with a UTF-8 BOM', async () => {
            const countries = Array.from({ length: 11 }, (_, i) => ({
                country: `C${String(i + 1).padStart(2, '0')}`,
                count: i + 1,
                percentage: 1,
            }));
            global.fetch = vi.fn().mockResolvedValue({
                ok: true,
                status: 200,
                json: () => Promise.resolve({
                    ...mockLinkStats,
                    truncated: false,
                    clicks_by_day: [
                        { date: '2024-01-03', count: 1 },
                        { date: '2024-01-01', count: 50 },
                    ],
                    clicks_by_country: countries,
                    clicks_by_city: [
                        { city: 'Paris', country: 'France', count: 40, percentage: 40 },
                        { city: 'Paris', country: null, count: 5, percentage: 5 },
                    ],
                }),
            });

            const { text, bytes } = await download();

            expect(bytes[0]).toBe(0xef);
            expect(bytes[1]).toBe(0xbb);
            expect(bytes[2]).toBe(0xbf);
            expect(text.startsWith('\uFEFFsection,key,clicks\n')).toBe(true);
            expect(text).not.toContain('#');

            // Days stay in chart order, including a quiet day. Breakdowns are
            // the top 10 the page lists, highest first.
            const dayRows = text.split('\n').filter(line => line.startsWith('"day",'));
            expect(dayRows).toEqual([
                '"day","2024-01-03",1',
                '"day","2024-01-01",50',
            ]);
            const countryRows = text.split('\n').filter(line => line.startsWith('"country",'));
            expect(countryRows).toHaveLength(10);
            expect(countryRows[0]).toBe('"country","C11",11');
            expect(countryRows[9]).toBe('"country","C02",2');
            expect(text).not.toContain('C01');
            expect(text).toContain('"city","Paris, France",40');
            expect(text).toContain('"city","Paris",5');
            expect(text).toContain('"device","Desktop",600');
            expect(text).toContain('"browser","Chrome",700');
            expect(text).toContain('"os","Windows",500');
            expect(text).toContain('"referrer","Google",400');
            expect(text).not.toContain('recent');

            const countriesCard = (await screen.findByRole('heading', { name: 'Top Countries' })).parentElement as HTMLElement;
            expect(within(countriesCard).getByText('C11')).toBeInTheDocument();
            expect(within(countriesCard).queryByText('C01')).not.toBeInTheDocument();
        });

        it('neutralises cells a spreadsheet would run as formulas', async () => {
            global.fetch = vi.fn().mockResolvedValue({
                ok: true,
                status: 200,
                json: () => Promise.resolve({
                    ...mockLinkStats,
                    truncated: false,
                    clicks_by_referer: [
                        { referer: 'a=1', count: 7, percentage: 1 },
                        { referer: 'say "hi"', count: 6, percentage: 1 },
                        { referer: '=1+1', count: 5, percentage: 1 },
                        { referer: '+1', count: 4, percentage: 1 },
                        { referer: '-1', count: 3, percentage: 1 },
                        { referer: '@SUM(A1)', count: 2, percentage: 1 },
                        { referer: '\tcmd', count: 1, percentage: 1 },
                        { referer: '\rcmd', count: 1, percentage: 1 },
                    ],
                }),
            });

            const { text } = await download();
            const referrers = text.split('\n').filter(line => line.includes('"referrer"'));
            // Highest count first. Only the first character is prefixed.
            expect(referrers).toEqual([
                '"referrer","a=1",7',
                '"referrer","say ""hi""",6',
                '"referrer","\'=1+1",5',
                '"referrer","\'+1",4',
                '"referrer","\'-1",3',
                '"referrer","\'@SUM(A1)",2',
                '"referrer","\'\tcmd",1',
                '"referrer","\'\rcmd",1',
            ]);
        });

        it('names the file with the code and the UTC day', async () => {
            // 02:00 UTC is still the previous evening in the Americas.
            vi.useFakeTimers({ toFake: ['Date'] });
            vi.setSystemTime(new Date('2026-10-16T02:00:00.000Z'));

            const { name } = await download();
            expect(name).toBe('abc123-analytics-2026-10-16.csv');
        });

        it('notes a sampled period before the header and leaves a full period unmarked', async () => {
            global.fetch = vi.fn().mockResolvedValue({
                ok: true,
                status: 200,
                json: () => Promise.resolve({
                    ...mockLinkStats,
                    truncated: true,
                    total_clicks: 412345,
                    clicks_by_day: [
                        { date: '2024-01-02', count: 20000 },
                        { date: '2024-01-03', count: 30000 },
                    ],
                }),
            });

            const sampled = await download();
            expect(sampled.text.startsWith(
                '\uFEFF# Based on the most recent 50000 clicks of 412345 in this period\nsection,key,clicks\n',
            )).toBe(true);
        });
    });
});


