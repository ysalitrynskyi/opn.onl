import { randomBytes } from 'node:crypto';
import {
    test as base,
    expect,
    type APIRequestContext,
    type Page,
    type PlaywrightWorkerArgs,
} from '@playwright/test';
import { API_URL, api, clientIpHeader, createLink, createUser, signIn, type TestUser, WEB_URL } from './support/api';

/**
 * The link analytics page (/analytics/:id) against the real backend.
 *
 * Clicks are real: each one is a GET of the short link through the backend's
 * redirect, carrying the visitor's User-Agent, Referer and address
 * (CF-Connecting-IP, which the e2e backend trusts as it would behind
 * Cloudflare). The page then shows whatever the backend recorded.
 */

const CHROME_ON_WINDOWS =
    'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36';
const SAFARI_ON_IPHONE =
    'Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1';
const FIREFOX_ON_LINUX = 'Mozilla/5.0 (X11; Linux x86_64; rv:127.0) Gecko/20100101 Firefox/127.0';

interface Visit {
    userAgent: string;
    ip: string;
    referer?: string;
}

interface CreatedLink {
    id: number;
    code: string;
    original_url: string;
}

/** The fields of GET /links/:id/stats these tests read (backend/src/handlers/analytics.rs). */
interface LinkStats {
    total_clicks: number;
    clicks_by_day: { date: string; count: number }[];
    recent_clicks: { id: number; timestamp: string }[];
}

/**
 * `count` addresses in different /24 networks. The backend stores a visitor's
 * address truncated to its /24 and counts unique visitors on that, so two
 * addresses in one /24 would be one visitor. Private addresses are never
 * geolocated, so location stays "Unknown" whatever GeoIP database is present.
 */
function visitorAddresses(count: number): string[] {
    const [a, b] = randomBytes(2);
    return Array.from({ length: count }, (_, i) => `10.${a}.${(b + i) % 256}.${i + 1}`);
}

/** Six clicks from four visitors, with no ties in any breakdown. */
function sixVisits(): Visit[] {
    const [first, second, third, fourth] = visitorAddresses(4);
    return [
        // The first Chrome visitor comes back from a second tweet: two clicks, one visitor.
        { userAgent: CHROME_ON_WINDOWS, ip: first, referer: 'https://twitter.com/opn_onl/status/1' },
        { userAgent: CHROME_ON_WINDOWS, ip: first, referer: 'https://twitter.com/opn_onl/status/2' },
        { userAgent: CHROME_ON_WINDOWS, ip: second, referer: 'https://twitter.com/someone/status/3' },
        { userAgent: SAFARI_ON_IPHONE, ip: third, referer: 'https://www.google.com/search?q=private+words' },
        { userAgent: SAFARI_ON_IPHONE, ip: third, referer: 'https://www.google.com/' },
        { userAgent: FIREFOX_ON_LINUX, ip: fourth },
    ];
}

/** Follow the short link through the backend once per visit, as that visitor. */
async function visit(request: APIRequestContext, code: string, visits: Visit[]): Promise<void> {
    for (const v of visits) {
        const res = await request.get(`${API_URL}/${code}`, {
            maxRedirects: 0,
            headers: {
                'User-Agent': v.userAgent,
                'CF-Connecting-IP': v.ip,
                ...(v.referer ? { Referer: v.referer } : {}),
            },
        });
        expect(res.status(), `redirect /${code}`).toBe(307);
    }
}

/**
 * Wait until the stats hold `clicks` clicks and return them. The backend
 * buffers clicks and writes them every CLICK_FLUSH_INTERVAL (5 s by default).
 */
async function flushedStats(
    request: APIRequestContext,
    token: string,
    linkId: number,
    clicks: number,
): Promise<LinkStats> {
    let stats: LinkStats | undefined;
    await expect
        .poll(
            async () => {
                const res = await api(request, 'get', `/links/${linkId}/stats`, token);
                if (!res.ok()) return `HTTP ${res.status()}`;
                stats = await res.json();
                return stats!.total_clicks;
            },
            { message: `stats of link ${linkId} should reach ${clicks} clicks`, timeout: 20_000 },
        )
        .toBe(clicks);
    return stats!;
}

interface WorkerFixtures {
    /** A verified account per worker. Tests add their own links to it but never change another test's. */
    owner: TestUser;
    /** A link of `owner` with six real clicks. Read-only: no test changes it or clicks it again. */
    clicked: { link: CreatedLink; stats: LinkStats };
}

/** Run `fn` with an API request context of its own (worker fixtures cannot use `request`). */
async function withRequest<T>(
    playwright: PlaywrightWorkerArgs['playwright'],
    fn: (request: APIRequestContext) => Promise<T>,
): Promise<T> {
    const request = await playwright.request.newContext();
    try {
        return await fn(request);
    } finally {
        await request.dispose();
    }
}

// Registration is slow (bcrypt) and clicks take a buffer flush to show up, so
// the account and the clicked link are built once per worker, not per test.
const test = base.extend<Record<never, never>, WorkerFixtures>({
    owner: [
        async ({ playwright }, provide) => {
            await provide(await withRequest(playwright, (request) => createUser(request)));
        },
        { scope: 'worker', timeout: 60_000 },
    ],
    clicked: [
        async ({ playwright, owner }, provide) => {
            const clicked = await withRequest(playwright, async (request) => {
                const link: CreatedLink = await createLink(request, owner.token);
                await visit(request, link.code, sixVisits());
                return { link, stats: await flushedStats(request, owner.token, link.id, 6) };
            });
            await provide(clicked);
        },
        { scope: 'worker', timeout: 60_000 },
    ],
});

// Give every page its own client address. The backend rate-limits by
// CF-Connecting-IP, so without this all browser traffic in the suite shares
// the 127.0.0.1 buckets (10 requests a second, 100 a minute).
test.beforeEach(async ({ page }) => {
    await page.setExtraHTTPHeaders(clientIpHeader());
});

/** Open the analytics page of `linkId` and wait for the backend to answer its stats request. */
async function openAnalytics(page: Page, linkId: number) {
    const answered = page.waitForResponse(
        (res) => new URL(res.url()).pathname === `/links/${linkId}/stats` && res.request().method() === 'GET',
    );
    await page.goto(`/analytics/${linkId}`);
    return answered;
}

/** The number on a metric card ("Total Clicks", "Today", ...). */
function metric(page: Page, title: string) {
    return page.getByText(title, { exact: true }).locator('xpath=../following-sibling::div[1]');
}

/** Assert a breakdown card lists exactly `rows` of [label, clicks, share], in order. */
async function expectBreakdown(page: Page, title: string, rows: [string, string, string][]) {
    const items = page
        .getByRole('heading', { name: title, exact: true })
        .locator('xpath=following-sibling::div[1]/div');
    await expect(items).toHaveCount(rows.length);
    for (const [i, row] of rows.entries()) {
        await expect(items.nth(i).locator('span')).toHaveText(row);
    }
}

/** A backend click timestamp (UTC, written without a zone) as an unambiguous ISO instant. */
function utcInstant(timestamp: string): string {
    return /(Z|[+-]\d\d:?\d\d)$/.test(timestamp) ? timestamp : `${timestamp.replace(' ', 'T')}Z`;
}

test.describe('Analytics Page', () => {
    test.beforeEach(async ({ page, owner, clicked }) => {
        await signIn(page, owner);
        await openAnalytics(page, clicked.link.id);
    });

    test('should display page title', async ({ page, clicked: { link } }) => {
        await expect(page.getByRole('heading', { level: 1 })).toHaveText(`${new URL(WEB_URL).host}/${link.code}`);
        await expect(page.getByText(link.original_url, { exact: true })).toBeVisible();
        await expect(page).toHaveTitle(`Analytics — /${link.code} | opn.onl`);
    });

    test('should display total clicks', async ({ page }) => {
        await expect(metric(page, 'Total Clicks')).toHaveText('6');
    });

    test('should display unique vs total visitors', async ({ page }) => {
        // Six clicks, but two of the four visitors clicked twice.
        await expect(metric(page, 'Total Clicks')).toHaveText('6');
        await expect(metric(page, 'Unique Visitors')).toHaveText('4');
    });

    test('should display today clicks', async ({ page }) => {
        await expect(metric(page, 'Today')).toHaveText('6');
    });

    test('should display 7-day clicks', async ({ page }) => {
        await expect(metric(page, 'This Week')).toHaveText('6');
    });

    test('should display click trend chart', async ({ page, clicked: { stats } }) => {
        const chart = page.getByRole('heading', { name: 'Clicks over time' }).locator('..');
        await expect(chart.locator('svg.recharts-surface')).toBeVisible();
        // One x-axis tick per UTC day with clicks, labelled like "Sep 30".
        expect(stats.clicks_by_day.length).toBeGreaterThan(0);
        for (const { date } of stats.clicks_by_day) {
            const label = new Date(`${date}T00:00:00Z`).toLocaleDateString('en-US', {
                month: 'short',
                day: 'numeric',
                timeZone: 'UTC',
            });
            await expect(chart.getByText(label, { exact: true })).toBeVisible();
        }
    });

    test('should display devices breakdown', async ({ page }) => {
        await expectBreakdown(page, 'Devices', [
            ['Desktop', '4', '66.7%'],
            ['Mobile', '2', '33.3%'],
        ]);
    });

    test('should display browsers breakdown', async ({ page }) => {
        await expectBreakdown(page, 'Browsers', [
            ['Chrome', '3', '50.0%'],
            ['Safari', '2', '33.3%'],
            ['Firefox', '1', '16.7%'],
        ]);
    });

    test('should display operating system distribution', async ({ page }) => {
        await expectBreakdown(page, 'Operating Systems', [
            ['Windows 10', '3', '50.0%'],
            ['iOS', '2', '33.3%'],
            ['Linux', '1', '16.7%'],
        ]);
    });

    test('should display top referrers', async ({ page }) => {
        // Only the referring host is kept (no tweet paths, no search terms);
        // clicks without a Referer count as Direct.
        await expectBreakdown(page, 'Top Referrers', [
            ['twitter.com', '3', '50.0%'],
            ['www.google.com', '2', '33.3%'],
            ['Direct', '1', '16.7%'],
        ]);
    });

    test('should display recent clicks table', async ({ page }) => {
        const table = page.getByRole('table');
        await expect(page.getByRole('heading', { name: 'Recent clicks' })).toBeVisible();
        await expect(table.getByRole('columnheader')).toHaveText(['Time', 'Location', 'Device', 'Browser', 'Referrer']);
        // Newest first: the reverse of the order the visits were made in.
        const rows = table.locator('tbody tr');
        const time = /\d{1,2}:\d{2}:\d{2}/;
        const expected = [
            [time, 'Unknown', 'Desktop', 'Firefox / Linux', 'Direct'],
            [time, 'Unknown', 'Mobile', 'Safari / iOS', 'www.google.com'],
            [time, 'Unknown', 'Mobile', 'Safari / iOS', 'www.google.com'],
            [time, 'Unknown', 'Desktop', 'Chrome / Windows 10', 'twitter.com'],
            [time, 'Unknown', 'Desktop', 'Chrome / Windows 10', 'twitter.com'],
            [time, 'Unknown', 'Desktop', 'Chrome / Windows 10', 'twitter.com'],
        ];
        await expect(rows).toHaveCount(expected.length);
        for (const [i, cells] of expected.entries()) {
            await expect(rows.nth(i).getByRole('cell')).toHaveText(cells);
        }
    });

    test.describe('viewed from Tokyo (UTC+9)', () => {
        test.use({ timezoneId: 'Asia/Tokyo' });

        test('should show when each click happened in local time', async ({ page, clicked: { stats } }) => {
            const newest = stats.recent_clicks[0];
            const local = await page.evaluate((instant) => new Date(instant).toLocaleString(), utcInstant(newest.timestamp));
            await expect(page.getByRole('table').locator('tbody tr').first().getByRole('cell').first()).toHaveText(local);
        });
    });

    test('should reload the stats for the chosen time range', async ({ page, clicked: { link } }) => {
        const range = page.getByLabel('Time range');
        await expect(range).toHaveValue('30');
        await expect(metric(page, 'Total Clicks')).toHaveText('6');

        const reloaded = page.waitForResponse(
            (res) => res.url().endsWith(`/links/${link.id}/stats?days=7`) && res.request().method() === 'GET',
        );
        await range.selectOption({ label: 'Last 7 days' });
        expect((await reloaded).status()).toBe(200);
        // Every click is from today, so the 7-day window still holds all six.
        await expect(metric(page, 'Total Clicks')).toHaveText('6');
        await expect(range).toHaveValue('7');
    });

    test('should navigate back to dashboard', async ({ page, clicked: { link } }) => {
        await page.getByRole('link', { name: 'Back to Dashboard' }).click();
        await expect(page).toHaveURL(/\/dashboard$/);
        await expect(page.getByRole('heading', { name: 'Dashboard', level: 1 })).toBeVisible();
        await expect(page.locator(`a[href="/analytics/${link.id}"]`)).toHaveText('6');
    });
});

test.describe('Analytics Page - Empty State', () => {
    test.beforeEach(async ({ page, request, owner }) => {
        const link: CreatedLink = await createLink(request, owner.token);
        await signIn(page, owner);
        await openAnalytics(page, link.id);
        await expect(page.getByRole('heading', { level: 1 })).toHaveText(`${new URL(WEB_URL).host}/${link.code}`);
    });

    test('should show zero clicks', async ({ page }) => {
        for (const title of ['Total Clicks', 'Unique Visitors', 'Today', 'This Week']) {
            await expect(metric(page, title), title).toHaveText('0');
        }
    });

    test('should show no clicks message', async ({ page }) => {
        await expect(page.getByRole('heading', { name: 'No clicks yet' })).toBeVisible();
        await expect(page.getByText('Share your link to start seeing analytics.')).toBeVisible();
    });

    test('should leave out the chart, breakdowns and recent clicks', async ({ page }) => {
        await expect(page.getByRole('heading', { name: 'No clicks yet' })).toBeVisible();
        for (const title of [
            'Clicks over time',
            'Top Countries',
            'Top Cities',
            'Devices',
            'Browsers',
            'Operating Systems',
            'Top Referrers',
            'Recent clicks',
        ]) {
            await expect(page.getByRole('heading', { name: title, exact: true }), title).toHaveCount(0);
        }
    });
});

test.describe('Analytics Page - Error Handling', () => {
    test('should show error for non-existent link', async ({ page, request, owner }) => {
        const link: CreatedLink = await createLink(request, owner.token);
        const deleted = await api(request, 'delete', `/links/${link.id}`, owner.token);
        expect(deleted.status()).toBe(200);

        await signIn(page, owner);
        expect((await openAnalytics(page, link.id)).status()).toBe(404);
        await expect(page.getByText('Link not found.', { exact: true })).toBeVisible();
        await expect(page.getByRole('link', { name: 'Back to Dashboard' })).toBeVisible();
        await expect(page.getByRole('heading', { level: 1 })).toHaveCount(0);
    });

    test('should show error for unauthorized access', async ({ page, request, owner }) => {
        const link: CreatedLink = await createLink(request, owner.token);
        const intruder = await createUser(request);

        await signIn(page, intruder);
        expect((await openAnalytics(page, link.id)).status()).toBe(403);
        await expect(
            page.getByText("You do not have permission to view this link's analytics.", { exact: true }),
        ).toBeVisible();
        // None of the owner's numbers leak into the page.
        await expect(page.getByRole('heading', { level: 1 })).toHaveCount(0);
        await expect(page.getByText('Total Clicks', { exact: true })).toHaveCount(0);
    });
});
