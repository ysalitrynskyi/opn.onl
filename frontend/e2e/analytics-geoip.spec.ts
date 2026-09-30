import { randomBytes } from 'node:crypto';
import {
    test as base,
    expect,
    type APIRequestContext,
    type Page,
    type PlaywrightWorkerArgs,
} from '@playwright/test';
import { API_URL, api, clientIpHeader, createLink, createUser, signIn, type TestUser } from './support/api';

/**
 * Where clicks came from, on the link analytics page (/analytics/:id).
 *
 * The backend geolocates a click with a MaxMind GeoLite2 database when one is
 * installed. The e2e backend has none, and private addresses are never looked
 * up anyway, so real clicks here always land in "Unknown". The tests that need
 * real places keep the real stats response and fill in only the location
 * fields a GeoIP database would have supplied (see `locate`).
 */

const CHROME_ON_WINDOWS =
    'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36';

interface CreatedLink {
    id: number;
    code: string;
}

interface RecentClick {
    id: number;
    timestamp: string;
    country: string | null;
    city: string | null;
    device: string | null;
    browser: string | null;
    os: string | null;
    referer: string | null;
}

interface GeoPoint {
    latitude: number;
    longitude: number;
    city: string | null;
    country: string | null;
    count: number;
}

/** GET /links/:id/stats (LinkStatsResponse in backend/src/handlers/analytics.rs). */
interface LinkStats {
    link_id: number;
    code: string;
    original_url: string;
    total_clicks: number;
    truncated: boolean;
    unique_visitors: number;
    clicks_by_day: { date: string; count: number }[];
    clicks_by_country: { country: string; count: number; percentage: number }[];
    clicks_by_city: { city: string; country: string | null; count: number; percentage: number }[];
    clicks_by_device: { device: string; count: number; percentage: number }[];
    clicks_by_browser: { browser: string; count: number; percentage: number }[];
    clicks_by_os: { os: string; count: number; percentage: number }[];
    clicks_by_referer: { referer: string; count: number; percentage: number }[];
    recent_clicks: RecentClick[];
    geo_data: GeoPoint[];
}

interface Place {
    country: string;
    city: string | null;
    latitude: number;
    longitude: number;
}

const NEW_YORK: Place = { country: 'United States', city: 'New York', latitude: 40.7128, longitude: -74.006 };
const LONDON: Place = { country: 'United Kingdom', city: 'London', latitude: 51.5074, longitude: -0.1278 };
// GeoLite2 resolves some addresses to a country but no city.
const GERMANY: Place = { country: 'Germany', city: null, latitude: 51.2993, longitude: 9.491 };

/** Where each of the six recent clicks (newest first) is placed in the resolved-location tests. */
const PLACES = [GERMANY, LONDON, LONDON, NEW_YORK, NEW_YORK, NEW_YORK];

/**
 * `stats` as get_link_stats would have returned it had GeoIP resolved recent
 * click i to places[i]: the per-click fields set, and the country, city and map
 * aggregates rebuilt the way the handler builds them (each bucket's share of
 * all loaded clicks, a click without a city counted under "Unknown" with the
 * country of the newest such click, map points clustered at two decimals).
 */
function locate(stats: LinkStats, places: Place[]): LinkStats {
    const clicks = stats.recent_clicks.map((click, i) => ({
        ...click,
        country: places[i].country,
        city: places[i].city,
    }));
    const share = (count: number) => (count / Math.max(clicks.length, 1)) * 100;

    const countries = new Map<string, number>();
    const cities = new Map<string, { count: number; country: string | null }>();
    const points = new Map<string, GeoPoint>();
    clicks.forEach((click, i) => {
        const country = click.country ?? 'Unknown';
        countries.set(country, (countries.get(country) ?? 0) + 1);

        const city = cities.get(click.city ?? 'Unknown') ?? { count: 0, country: click.country };
        city.count += 1;
        cities.set(click.city ?? 'Unknown', city);

        const { latitude, longitude } = places[i];
        const key = `${Math.trunc(latitude * 100)},${Math.trunc(longitude * 100)}`;
        const point = points.get(key) ?? { latitude, longitude, city: click.city, country: click.country, count: 0 };
        point.count += 1;
        points.set(key, point);
    });

    return {
        ...stats,
        clicks_by_country: [...countries].map(([country, count]) => ({ country, count, percentage: share(count) })),
        clicks_by_city: [...cities].map(([city, { count, country }]) => ({
            city,
            country,
            count,
            percentage: share(count),
        })),
        recent_clicks: clicks,
        geo_data: [...points.values()],
    };
}

/** Six clicks on `code` through the backend redirect, each from its own private /24. */
async function clickSixTimes(request: APIRequestContext, code: string): Promise<void> {
    const [a, b] = randomBytes(2);
    for (let i = 0; i < 6; i++) {
        const res = await request.get(`${API_URL}/${code}`, {
            maxRedirects: 0,
            headers: { 'User-Agent': CHROME_ON_WINDOWS, 'CF-Connecting-IP': `10.${a}.${(b + i) % 256}.${i + 1}` },
        });
        expect(res.status(), `redirect /${code}`).toBe(307);
    }
}

interface WorkerFixtures {
    owner: TestUser;
    /** A link of `owner` with six real clicks, and its stats once they are all recorded. Read-only. */
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

/**
 * Wait until the stats of `link` hold all six clicks and return them. The
 * backend buffers clicks and writes them every CLICK_FLUSH_INTERVAL (5 s by default).
 */
async function flushedStats(request: APIRequestContext, token: string, link: CreatedLink): Promise<LinkStats> {
    let stats: LinkStats | undefined;
    await expect
        .poll(
            async () => {
                const res = await api(request, 'get', `/links/${link.id}/stats`, token);
                if (!res.ok()) return `HTTP ${res.status()}`;
                stats = await res.json();
                return stats!.recent_clicks.length;
            },
            { message: `stats of link ${link.id} should hold six clicks`, timeout: 20_000 },
        )
        .toBe(PLACES.length);
    return stats!;
}

// Registration is slow (bcrypt) and clicks take a buffer flush to reach the
// stats, so the account and its clicked link are built once per worker.
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
                await clickSixTimes(request, link.code);
                return { link, stats: await flushedStats(request, owner.token, link) };
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

/** The Location cell of every recent-click row, newest first. */
function locations(page: Page) {
    return page.getByRole('table').locator('tbody tr td:nth-child(2)');
}

test.describe('GeoIP Analytics', () => {
    test.beforeEach(async ({ page, owner }) => {
        await signIn(page, owner);
    });

    test('should show clicks without a known location as Unknown', async ({ page, clicked: { link } }) => {
        await openAnalytics(page, link.id);
        await expectBreakdown(page, 'Top Countries', [['Unknown', '6', '100.0%']]);
        await expectBreakdown(page, 'Top Cities', [['Unknown', '6', '100.0%']]);
        await expect(locations(page)).toHaveText(Array(6).fill('Unknown'));
    });

    test.describe('with locations resolved', () => {
        // Mocked: the e2e backend has no GeoIP database, so it cannot resolve a
        // click to a country or city. The real stats response is fetched and only
        // the location fields are filled in; everything else is what the backend sent.
        test.beforeEach(async ({ page, clicked: { link } }) => {
            await page.route(
                (url) => url.pathname === `/links/${link.id}/stats`,
                async (route) => {
                    if (route.request().method() !== 'GET') return route.fallback();
                    const response = await route.fetch();
                    await route.fulfill({ response, json: locate(await response.json(), PLACES) });
                },
            );
            await openAnalytics(page, link.id);
        });

        test('should display country breakdown', async ({ page }) => {
            await expectBreakdown(page, 'Top Countries', [
                ['United States', '3', '50.0%'],
                ['United Kingdom', '2', '33.3%'],
                ['Germany', '1', '16.7%'],
            ]);
        });

        test('should display city breakdown', async ({ page }) => {
            // The click resolved to Germany alone has no city.
            await expectBreakdown(page, 'Top Cities', [
                ['New York', '3', '50.0%'],
                ['London', '2', '33.3%'],
                ['Unknown', '1', '16.7%'],
            ]);
        });

        test('should display recent clicks with location', async ({ page }) => {
            await expect(locations(page)).toHaveText([
                'Germany',
                'London, United Kingdom',
                'London, United Kingdom',
                'New York, United States',
                'New York, United States',
                'New York, United States',
            ]);
        });
    });
});
