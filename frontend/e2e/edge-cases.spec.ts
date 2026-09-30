import { createHmac, randomBytes } from 'node:crypto';
import { test, expect, type APIRequestContext, type Locator, type Page } from '@playwright/test';
import {
    API_URL,
    WEB_URL,
    api,
    clientIpHeader,
    createLink,
    createUser,
    signIn,
    type TestUser,
} from './support/api';

/**
 * Edge cases of the dashboard against the real backend: what the create form
 * does with awkward input, how a dead session is handled, what the page shows
 * when the links request is slow or fails, and how odd data renders.
 *
 * Dates render in the browser's zone and locale, so both are pinned. The
 * timezone test below overrides the zone on purpose.
 */
test.use({ locale: 'en-US', timezoneId: 'UTC' });

// The backend trusts CF-Connecting-IP in e2e (TRUST_PROXY_HEADERS=true). Give
// every page its own client address so this file's browser traffic does not
// spend 127.0.0.1's per-IP buckets (10/s, 100/min, and 10/min for /auth/*,
// which the dashboard hits on every load for /auth/settings) that the rest of
// the suite and the rate-limit specs share.
test.beforeEach(async ({ page }) => {
    await page.setExtraHTTPHeaders(clientIpHeader());
});

const SHORT_HOST = new URL(WEB_URL).host;

function urlInput(page: Page): Locator {
    return page.getByPlaceholder('https://example.com/long-url');
}

function createButton(page: Page): Locator {
    return page.getByRole('button', { name: 'Create', exact: true });
}

function dashboardHeading(page: Page): Locator {
    return page.getByRole('heading', { name: 'Dashboard', level: 1 });
}

/** The dashboard row of the link with this short code. */
function linkRow(page: Page, code: string): Locator {
    const shortLink = page.getByRole('link', { name: `${SHORT_HOST}/${code}`, exact: true });
    return page.locator('div.group').filter({ has: shortLink });
}

/** The M/D/YYYY expiry badge; the created date in the same row is "Mon D". */
function expiryBadge(row: Locator): Locator {
    return row.getByText(/^\d{1,2}\/\d{1,2}\/\d{4}$/);
}

async function openDashboard(page: Page, user: TestUser): Promise<void> {
    await signIn(page, user);
    await page.goto('/dashboard');
    await expect(dashboardHeading(page)).toBeVisible({ timeout: 15_000 });
}

/** Every POST /links the page sends, so a test can assert none (or one) went out. */
function recordCreateRequests(page: Page): string[] {
    const bodies: string[] = [];
    page.on('request', (req) => {
        if (req.method() === 'POST' && req.url() === `${API_URL}/links`) {
            bodies.push(req.postData() ?? '');
        }
    });
    return bodies;
}

async function linksOf(page: Page, user: TestUser) {
    const res = await api(page.request, 'get', '/links', user.token);
    expect(res.status()).toBe(200);
    return (await res.json()) as Array<Record<string, unknown>>;
}

/**
 * Put `token` in localStorage for the first document only. signIn() re-seeds on
 * every navigation, which would bounce a rejected token between /login and
 * /dashboard forever instead of letting the app sign it out.
 */
async function seedTokenOnce(page: Page, token: string): Promise<void> {
    await page.addInitScript((t) => {
        if (sessionStorage.getItem('e2e-token-seeded')) return;
        sessionStorage.setItem('e2e-token-seeded', '1');
        localStorage.setItem('token', t);
    }, token);
}

/** An HS256 JWT with the backend's claim shape, signed with a key it does not hold. */
function forgedJwt(userId: number): string {
    const b64 = (value: object) => Buffer.from(JSON.stringify(value)).toString('base64url');
    const header = b64({ alg: 'HS256', typ: 'JWT' });
    const payload = b64({
        sub: 'e2e-admin@users.opn.onl',
        user_id: userId,
        token_version: 0,
        exp: Math.floor(Date.now() / 1000) + 3600,
    });
    const signature = createHmac('sha256', randomBytes(32)).update(`${header}.${payload}`).digest('base64url');
    return `${header}.${payload}.${signature}`;
}

// ============= Input validation =============

test.describe('Create form input', () => {
    test('a value that is not a URL is stopped by the browser and never sent', async ({ page, request }) => {
        const user = await createUser(request);
        const sent = recordCreateRequests(page);
        await openDashboard(page, user);

        await urlInput(page).fill('not-a-valid-url');
        await createButton(page).click();

        await expect(urlInput(page)).toHaveJSProperty('validity.typeMismatch', true);
        await expect(urlInput(page)).toHaveValue('not-a-valid-url');
        await expect(page.getByText('No links yet')).toBeVisible();
        expect(sent).toEqual([]);
    });

    test('an empty URL is stopped by the browser and never sent', async ({ page, request }) => {
        const user = await createUser(request);
        const sent = recordCreateRequests(page);
        await openDashboard(page, user);

        await createButton(page).click();

        await expect(urlInput(page)).toHaveJSProperty('validity.valueMissing', true);
        await expect(page.getByText('No links yet')).toBeVisible();
        expect(sent).toEqual([]);
    });

    test('a URL the browser accepts but the server refuses shows the server reason', async ({ page, request }) => {
        const user = await createUser(request);
        await openDashboard(page, user);

        // type="url" accepts any absolute URL; only the backend limits schemes.
        await urlInput(page).fill('ftp://ftp.gnu.org/gnu/');
        await createButton(page).click();

        await expect(page.getByRole('alert')).toHaveText('URL must use http or https protocol');
        await expect(urlInput(page)).toHaveValue('ftp://ftp.gnu.org/gnu/');
        await expect(page.getByText('0 links', { exact: true })).toBeVisible();
    });

    test('a URL over 2048 characters is refused with the length limit', async ({ page, request }) => {
        const user = await createUser(request);
        await openDashboard(page, user);

        const tooLong = 'https://www.iana.org/' + 'a'.repeat(2100);
        await urlInput(page).fill(tooLong);
        await createButton(page).click();

        await expect(page.getByRole('alert')).toHaveText('URL is too long (max 2048 characters)');
        await expect(page.getByText('0 links', { exact: true })).toBeVisible();
        expect(await linksOf(page, user)).toEqual([]);
    });

    test('a long URL within the limit is shortened and shown truncated', async ({ page, request }) => {
        const user = await createUser(request);
        await openDashboard(page, user);

        const longUrl = 'https://www.iana.org/' + 'a'.repeat(2000);
        await urlInput(page).fill(longUrl);
        await createButton(page).click();

        await expect(page.getByText('1 links', { exact: true })).toBeVisible();
        const [link] = await linksOf(page, user);
        expect(link.original_url).toBe(longUrl);

        // The row shows the first 50 characters after the scheme; the full URL
        // stays reachable through the anchor's href and title.
        const destination = linkRow(page, String(link.code)).getByTitle(longUrl, { exact: true });
        await expect(destination).toHaveText('www.iana.org/' + 'a'.repeat(37) + '...');
        await expect(destination).toHaveAttribute('href', longUrl);
        await expect(urlInput(page)).toHaveValue('');
    });

    test('query strings and fragments are stored exactly as typed', async ({ page, request }) => {
        const user = await createUser(request);
        await openDashboard(page, user);

        const url = 'https://www.iana.org/help/example-domains?query=test&foo=bar&empty=&enc=a%20b#section-2';
        await urlInput(page).fill(url);
        await createButton(page).click();

        await expect(page.getByText('1 links', { exact: true })).toBeVisible();
        const [link] = await linksOf(page, user);
        expect(link.original_url).toBe(url);
        const destination = linkRow(page, String(link.code)).getByTitle(url, { exact: true });
        await expect(destination).toHaveAttribute('href', url);
    });

    test('a Unicode URL is stored and shown as typed, and its short link redirects to it', async ({ page, request }) => {
        const user = await createUser(request);
        await openDashboard(page, user);

        const url = 'https://ru.wikipedia.org/wiki/Москва';
        await urlInput(page).fill(url);
        await createButton(page).click();

        await expect(page.getByText('1 links', { exact: true })).toBeVisible();
        const [link] = await linksOf(page, user);
        expect(link.original_url).toBe(url);
        const destination = linkRow(page, String(link.code)).getByTitle(url, { exact: true });
        await expect(destination).toHaveText('ru.wikipedia.org/wiki/Москва');

        // The short link redirects there. The redirect is read without following
        // it (the destination is a third-party site). The Location header is
        // raw UTF-8, which Node hands over as latin1; a browser reads it as UTF-8.
        const redirect = await request.get(`${API_URL}/${link.code}`, {
            maxRedirects: 0,
            headers: clientIpHeader(),
        });
        expect(redirect.status()).toBe(307);
        const location = Buffer.from(redirect.headers()['location'], 'latin1').toString('utf8');
        expect(new URL(location).href).toBe('https://ru.wikipedia.org/wiki/%D0%9C%D0%BE%D1%81%D0%BA%D0%B2%D0%B0');
    });
});

// ============= Authentication =============

test.describe('Dead or missing session', () => {
    test('without a token the dashboard sends the visitor to sign in', async ({ page }) => {
        const listRequests: string[] = [];
        page.on('request', (req) => {
            if (req.url().startsWith(`${API_URL}/links`)) listRequests.push(req.url());
        });

        await page.goto('/dashboard');

        await expect(page).toHaveURL(/\/login$/);
        await expect(page.getByRole('heading', { name: 'Sign in to opn.onl' })).toBeVisible();
        expect(listRequests).toEqual([]);
    });

    const rejectedTokens: Array<{ kind: string; token: (request: APIRequestContext) => Promise<string> }> = [
        { kind: 'a malformed token', token: async () => 'not-a-valid-jwt' },
        {
            kind: 'a token signed with the wrong key',
            token: async () => forgedJwt(Number(process.env.E2E_ADMIN_ID ?? 1)),
        },
        {
            kind: 'a token revoked by a password change',
            token: async (request) => {
                const user = await createUser(request);
                const res = await api(request, 'post', '/auth/change-password', user.token, {
                    current_password: user.password,
                    new_password: 'E2e-Changed-Password-456!',
                });
                expect(res.status(), await res.text()).toBe(200);
                return user.token;
            },
        },
    ];

    for (const { kind, token } of rejectedTokens) {
        test(`${kind} is discarded and the visitor lands on sign-in`, async ({ page, request }) => {
            await seedTokenOnce(page, await token(request));
            const listed = page.waitForResponse(
                (res) => res.url() === `${API_URL}/links` && res.request().method() === 'GET',
            );

            await page.goto('/dashboard');

            expect((await listed).status()).toBe(401);
            await expect(page).toHaveURL(/\/login$/);
            await expect(page.getByRole('heading', { name: 'Sign in to opn.onl' })).toBeVisible();
            expect(await page.evaluate(() => localStorage.getItem('token'))).toBeNull();
        });
    }
});

// ============= Slow and failing API =============

// A slow or failing backend cannot be produced on demand, so these tests
// intercept only GET /links. Everything else the dashboard calls goes to the
// real backend for a real account.
test.describe('Links request slow or failing', () => {
    test('a slow response shows the loading skeleton until the links arrive', async ({ page, request }) => {
        const user = await createUser(request);
        const link = await createLink(request, user.token, { original_url: 'https://www.iana.org/domains' });

        let pending = 0;
        let release!: () => void;
        const held = new Promise<void>((resolve) => (release = resolve));
        await page.route(`${API_URL}/links`, async (route) => {
            if (route.request().method() === 'GET') {
                pending += 1;
                await held;
            }
            await route.continue();
        });
        await signIn(page, user);
        await page.goto('/dashboard');

        await expect.poll(() => pending).toBeGreaterThan(0);
        await expect(page.locator('.animate-pulse').first()).toBeVisible();
        await expect(dashboardHeading(page)).toHaveCount(0);

        release();
        await expect(dashboardHeading(page)).toBeVisible();
        await expect(linkRow(page, link.code)).toBeVisible();
        await expect(page.locator('.animate-pulse')).toHaveCount(0);
    });

    test('a network failure shows an error instead of an empty account', async ({ page, request }) => {
        const user = await createUser(request);
        await page.route(`${API_URL}/links`, (route) =>
            route.request().method() === 'GET' ? route.abort('failed') : route.continue(),
        );
        await openDashboard(page, user);

        await expect(page.getByRole('alert')).toHaveText('Failed to load links. Please try again.');
        await expect(page.getByText('No links yet')).toHaveCount(0);
    });

    test('a server error shows the message from the response body', async ({ page, request }) => {
        const user = await createUser(request);
        await page.route(`${API_URL}/links`, (route) =>
            route.request().method() === 'GET'
                ? route.fulfill({ status: 500, json: { error: 'Database error' } })
                : route.continue(),
        );
        await openDashboard(page, user);

        await expect(page.getByRole('alert')).toHaveText('Database error');
        await expect(page.getByText('No links yet')).toHaveCount(0);
    });

    test('an error page that is not JSON falls back to a generic message', async ({ page, request }) => {
        const user = await createUser(request);
        // What a reverse proxy returns while the backend is down.
        await page.route(`${API_URL}/links`, (route) =>
            route.request().method() === 'GET'
                ? route.fulfill({
                      status: 503,
                      contentType: 'text/html',
                      body: '<html><body><h1>503 Service Unavailable</h1></body></html>',
                  })
                : route.continue(),
        );
        await openDashboard(page, user);

        await expect(page.getByRole('alert')).toHaveText('Failed to load links. Please try again.');
        await expect(page.getByText('No links yet')).toHaveCount(0);
    });
});

// ============= Dates and schedules =============

test.describe('Expiry and schedule', () => {
    test('a link past its expiry is flagged inactive and shows the expiry date', async ({ page, request }) => {
        const user = await createUser(request);
        const link = await createLink(request, user.token, {
            original_url: 'https://www.iana.org/domains/reserved',
            expires_at: '2020-01-01T00:00:00Z',
        });
        await openDashboard(page, user);

        const row = linkRow(page, link.code);
        await expect(row.getByText('Inactive', { exact: true })).toBeVisible();
        await expect(expiryBadge(row)).toHaveText('1/1/2020');
        await expect(page.getByText('0 active', { exact: true })).toBeVisible();
    });

    test('a link expiring within the hour is still active', async ({ page, request }) => {
        const user = await createUser(request);
        const expiresAt = new Date(Date.now() + 60 * 60 * 1000);
        const link = await createLink(request, user.token, {
            original_url: 'https://www.iana.org/domains/root',
            expires_at: expiresAt.toISOString(),
        });
        await openDashboard(page, user);

        const row = linkRow(page, link.code);
        await expect(expiryBadge(row)).toHaveText(expiresAt.toLocaleDateString('en-US', { timeZone: 'UTC' }));
        await expect(row.getByText('Inactive', { exact: true })).toHaveCount(0);
        await expect(page.getByText('1 active', { exact: true })).toBeVisible();
    });

    test('a link scheduled to start in the future is shown as inactive', async ({ page, request }) => {
        const user = await createUser(request);
        const link = await createLink(request, user.token, {
            original_url: 'https://www.iana.org/about',
            starts_at: new Date(Date.now() + 365 * 24 * 60 * 60 * 1000).toISOString(),
        });
        await openDashboard(page, user);

        const row = linkRow(page, link.code);
        await expect(row.getByText('Inactive', { exact: true })).toBeVisible();
        await expect(page.getByText('0 active', { exact: true })).toBeVisible();
    });

    test.describe('outside UTC', () => {
        test.use({ timezoneId: 'America/New_York' });

        test('the expiry date on the row is the local date picked in the form', async ({ page, request }) => {
            test.fail(
                true,
                'BUG: GET /links returns expires_at as a zone-less UTC string ("2030-01-16 04:59:00") and the dashboard parses it as local time, so west of UTC the badge shows the next day',
            );
            const user = await createUser(request);
            const sent = recordCreateRequests(page);
            await openDashboard(page, user);

            await urlInput(page).fill('https://www.iana.org/domains/int');
            await page.getByRole('button', { name: 'Advanced options' }).click();
            await page.getByLabel('Expiration', { exact: true }).fill('2030-01-15');
            // The form states the expiry in the user's zone (time defaults to 23:59)...
            await expect(page.getByText(/→ Expires: 1\/15\/2030, 11:59:00\sPM/)).toBeVisible();
            await createButton(page).click();
            await expect(page.getByText('1 links', { exact: true })).toBeVisible();
            // ...and sends the matching instant.
            expect(JSON.parse(sent[0]).expires_at).toBe('2030-01-16T04:59:00.000Z');

            const [link] = await linksOf(page, user);
            await expect(expiryBadge(linkRow(page, String(link.code)))).toHaveText('1/15/2030');
        });
    });
});

// ============= Unusual data =============

test.describe('Unusual data', () => {
    test('an account without links shows the empty state', async ({ page, request }) => {
        const user = await createUser(request);
        await openDashboard(page, user);

        await expect(page.getByRole('heading', { name: 'No links yet' })).toBeVisible();
        await expect(page.getByText('Create your first shortened link above.')).toBeVisible();
        await expect(page.getByText('0 links', { exact: true })).toBeVisible();
        await expect(page.getByText('0 clicks', { exact: true })).toBeVisible();
        // Search and sort only appear once there is something to search.
        await expect(page.getByPlaceholder('Search links, notes, tags...')).toHaveCount(0);

        await page.getByRole('button', { name: 'Start shortening' }).click();
        await expect(urlInput(page)).toBeFocused();
    });

    test('a hundred links are paged twenty at a time', async ({ page, request }) => {
        const user = await createUser(request);
        const urls = Array.from({ length: 100 }, (_, i) => `https://www.iana.org/domains/page-${i}`);
        const bulk = await api(request, 'post', '/links/bulk', user.token, { urls });
        expect(bulk.status()).toBe(200);
        const created = await bulk.json();
        expect(created.errors).toEqual([]);
        expect(created.links).toHaveLength(100);

        await openDashboard(page, user);
        const rows = page.getByRole('button', { name: 'Edit link' });
        const previous = page.getByRole('button', { name: 'Previous page' });
        const next = page.getByRole('button', { name: 'Next page' });

        await expect(page.getByText('100 links', { exact: true })).toBeVisible();
        await expect(page.getByText('Showing 1-20 of 100')).toBeVisible();
        await expect(page.getByText('1 / 5', { exact: true })).toBeVisible();
        await expect(rows).toHaveCount(20);
        await expect(previous).toBeDisabled();

        await next.click();
        await expect(page.getByText('Showing 21-40 of 100')).toBeVisible();
        await expect(page.getByText('2 / 5', { exact: true })).toBeVisible();
        await expect(rows).toHaveCount(20);

        await next.click();
        await next.click();
        await next.click();
        await expect(page.getByText('Showing 81-100 of 100')).toBeVisible();
        await expect(page.getByText('5 / 5', { exact: true })).toBeVisible();
        await expect(next).toBeDisabled();
    });

    test('a link with no clicks shows zero counts', async ({ page, request }) => {
        const user = await createUser(request);
        const link = await createLink(request, user.token, { original_url: 'https://www.iana.org/numbers' });
        await openDashboard(page, user);

        const row = linkRow(page, link.code);
        await expect(row.locator(`a[href="/analytics/${link.id}"]`)).toHaveText('0');
        await expect(row.getByText('0.0/day avg')).toBeVisible();
        await expect(page.getByText('0 clicks', { exact: true })).toBeVisible();
    });

    test('a very high click count is grouped with separators', async ({ page, request }) => {
        const user = await createUser(request);
        const link = await createLink(request, user.token, { original_url: 'https://www.iana.org/protocols' });
        // Ten million real clicks are impractical to produce, so the real list
        // response is fetched and only this link's count is raised.
        await page.route(`${API_URL}/links`, async (route) => {
            if (route.request().method() !== 'GET') return route.continue();
            const response = await route.fetch();
            const links = await response.json();
            for (const l of links) if (l.id === link.id) l.click_count = 9_999_999;
            await route.fulfill({ response, json: links });
        });
        await openDashboard(page, user);

        const row = linkRow(page, link.code);
        await expect(row.locator(`a[href="/analytics/${link.id}"]`)).toHaveText('9,999,999');
        await expect(page.getByText('9,999,999 clicks', { exact: true })).toBeVisible();
    });

    test('a link with twenty tags shows two and a count of the rest', async ({ page, request }) => {
        const user = await createUser(request);
        const tagIds = await Promise.all(
            Array.from({ length: 20 }, async (_, i) => {
                const res = await api(request, 'post', '/tags', user.token, {
                    name: `tag-${String(i).padStart(2, '0')}`,
                    color: '#2563eb',
                });
                expect(res.status()).toBe(201);
                return (await res.json()).id as number;
            }),
        );
        const link = await createLink(request, user.token, {
            original_url: 'https://www.iana.org/time-zones',
            tag_ids: tagIds,
        });
        expect(link.tags).toHaveLength(20);
        await openDashboard(page, user);

        const row = linkRow(page, link.code);
        await expect(row.getByText(/^tag-\d{2}$/)).toHaveCount(2);
        await expect(row.getByText('+18', { exact: true })).toBeVisible();
    });

    test('markup in a title, notes and tag names renders as text', async ({ page, request }) => {
        const user = await createUser(request);
        const title = '<img src=x onerror="window.__xss=1">';
        const tagName = '<b onmouseover="window.__xss=2">bold</b>';
        const tag = await (await api(request, 'post', '/tags', user.token, { name: tagName })).json();
        const link = await createLink(request, user.token, {
            original_url: 'https://www.iana.org/help',
            title,
            notes: '<script>window.__xss=3</script> & "quotes"',
            tag_ids: [tag.id],
        });
        const dialogs: string[] = [];
        page.on('dialog', (dialog) => {
            dialogs.push(dialog.message());
            void dialog.dismiss();
        });
        await openDashboard(page, user);

        const row = linkRow(page, link.code);
        await expect(row.getByText(title, { exact: true })).toBeVisible();
        await expect(row.getByText(tagName, { exact: true })).toBeVisible();
        // Notes are not shown on the row, but search matches them: the markup
        // in them is held as plain data and matched as text.
        await page.getByPlaceholder('Search links, notes, tags...').fill('"quotes"');
        await expect(row).toBeVisible();

        await expect(page.locator('img[src="x"]')).toHaveCount(0);
        await expect(page.locator('b[onmouseover]')).toHaveCount(0);
        expect(await page.evaluate(() => (window as unknown as { __xss?: number }).__xss)).toBeUndefined();
        expect(dialogs).toEqual([]);
    });
});

// ============= UI state =============

test.describe('Create form state', () => {
    test('clicking Create repeatedly while it is saving submits once', async ({ page, request }) => {
        const user = await createUser(request);
        let posts = 0;
        let release!: () => void;
        const held = new Promise<void>((resolve) => (release = resolve));
        // Hold the create request so the extra clicks land while it is in flight.
        await page.route(`${API_URL}/links`, async (route) => {
            if (route.request().method() === 'POST') {
                posts += 1;
                await held;
            }
            await route.continue();
        });
        await openDashboard(page, user);

        await urlInput(page).fill('https://www.iana.org/reports');
        await createButton(page).click({ clickCount: 3 });
        await expect(createButton(page)).toBeDisabled();
        release();

        await expect(page.getByText('1 links', { exact: true })).toBeVisible();
        expect(posts).toBe(1);
        expect(await linksOf(page, user)).toHaveLength(1);
    });

    test('the form is locked while a link is being created and reset afterwards', async ({ page, request }) => {
        const user = await createUser(request);
        let pending = 0;
        let release!: () => void;
        const held = new Promise<void>((resolve) => (release = resolve));
        await page.route(`${API_URL}/links`, async (route) => {
            if (route.request().method() === 'POST') {
                pending += 1;
                await held;
            }
            await route.continue();
        });
        await openDashboard(page, user);

        await urlInput(page).fill('https://www.iana.org/performance');
        await createButton(page).click();

        await expect.poll(() => pending).toBe(1);
        await expect(createButton(page)).toBeDisabled();
        await expect(urlInput(page)).toHaveValue('https://www.iana.org/performance');

        release();
        await expect(createButton(page)).toBeEnabled();
        await expect(urlInput(page)).toHaveValue('');
        const [link] = await linksOf(page, user);
        await expect(linkRow(page, String(link.code))).toBeVisible();
    });

    test('the layout reflows between phone and desktop widths without sideways scroll', async ({ page, request }) => {
        const user = await createUser(request);
        const link = await createLink(request, user.token, {
            original_url: 'https://www.iana.org/' + 'b'.repeat(300),
            title: 'A deliberately long title that has to wrap or truncate on a narrow phone screen',
        });
        await openDashboard(page, user);
        const horizontalOverflow = () =>
            page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

        await page.setViewportSize({ width: 375, height: 800 });
        expect(await horizontalOverflow()).toBe(0);
        const deleteButton = (await linkRow(page, link.code).getByRole('button', { name: 'Delete link' }).boundingBox())!;
        expect(deleteButton.x + deleteButton.width).toBeLessThanOrEqual(375); // row actions stay on screen
        let input = (await urlInput(page).boundingBox())!;
        let button = (await createButton(page).boundingBox())!;
        expect(button.y).toBeGreaterThanOrEqual(input.y + input.height); // stacked under the URL

        await page.setViewportSize({ width: 1280, height: 800 });
        expect(await horizontalOverflow()).toBe(0);
        input = (await urlInput(page).boundingBox())!;
        button = (await createButton(page).boundingBox())!;
        expect(button.x).toBeGreaterThan(input.x + input.width); // beside it
    });
});

// ============= Concurrent changes =============

test.describe('Changes from elsewhere while editing', () => {
    test('saving the edit dialog keeps a destination changed elsewhere meanwhile', async ({ page, request }) => {
        const user = await createUser(request);
        const link = await createLink(request, user.token, { original_url: 'https://www.iana.org/domains/arpa' });
        await openDashboard(page, user);

        await linkRow(page, link.code).getByRole('button', { name: 'Edit link' }).click();
        const dialog = page.getByRole('dialog');
        await expect(dialog.getByLabel('Destination URL')).toHaveValue('https://www.iana.org/domains/arpa');

        // Another tab (or an API client) moves the destination meanwhile.
        const moved = 'https://www.iana.org/domains/idn-tables';
        const res = await api(request, 'put', `/links/${link.id}`, user.token, { original_url: moved });
        expect(res.status()).toBe(200);

        // This dialog only changes the password; it must not write back the
        // destination it loaded before the other change.
        await dialog.getByLabel('Add password').fill('e2e-link-password');
        await dialog.getByRole('button', { name: 'Save changes' }).click();
        await expect(dialog).toHaveCount(0);

        const row = linkRow(page, link.code);
        await expect(row.getByText('Protected', { exact: true })).toBeVisible();
        await expect(row.getByTitle(moved, { exact: true })).toBeVisible();
        const [stored] = await linksOf(page, user);
        expect(stored.original_url).toBe(moved);
        expect(stored.has_password).toBe(true);
    });

    test('saving a link that was deleted elsewhere reports it is gone', async ({ page, request }) => {
        const user = await createUser(request);
        const link = await createLink(request, user.token, { original_url: 'https://www.iana.org/domains/root/db' });
        await openDashboard(page, user);

        await linkRow(page, link.code).getByRole('button', { name: 'Edit link' }).click();
        const dialog = page.getByRole('dialog');
        await expect(dialog).toBeVisible();

        const res = await api(request, 'delete', `/links/${link.id}`, user.token);
        expect(res.status()).toBe(200);

        await dialog.getByLabel('Destination URL').fill('https://www.iana.org/domains/root/files');
        await dialog.getByRole('button', { name: 'Save changes' }).click();

        await expect(dialog.getByRole('alert')).toHaveText('Link not found');
        await expect(dialog).toBeVisible();
    });
});

// ============= Browser capabilities =============

test.describe('Missing browser capabilities', () => {
    test('without the Clipboard API, copying reports a failure', async ({ page, request }) => {
        const user = await createUser(request);
        const link = await createLink(request, user.token, { original_url: 'https://www.iana.org/abuse' });
        // Headless Chromium refuses clipboard writes unless granted, which would
        // fail the copy for the wrong reason. With the grant, copying works, so
        // the missing API is the only thing that can make it fail here.
        await page.context().grantPermissions(['clipboard-read', 'clipboard-write'], { origin: WEB_URL });
        await page.addInitScript(() => {
            Object.defineProperty(navigator, 'clipboard', { value: undefined, configurable: true });
        });
        await openDashboard(page, user);

        await linkRow(page, link.code).getByRole('button', { name: 'Copy short URL' }).click();

        await expect(page.getByText('Failed to copy link')).toBeVisible();
        await expect(page.getByText('Short link copied!')).toHaveCount(0);
    });

    test('with site storage blocked the public home page still renders', async ({ page }) => {
        test.fail(
            true,
            'BUG: Layout reads localStorage during render without a guard, so when storage access throws (site data blocked) every page, including the public home page, is replaced by the error boundary',
        );
        // What Chrome does when the visitor blocks site data for the origin.
        await page.addInitScript(() => {
            Object.defineProperty(window, 'localStorage', {
                get() {
                    throw new DOMException('Access is denied for this document.', 'SecurityError');
                },
            });
        });

        await page.goto('/');

        await expect(page.getByRole('heading', { level: 1, name: /Short links that\s*answer to you/ })).toBeVisible();
        await expect(page.getByText('Something went wrong')).toHaveCount(0);
    });
});
