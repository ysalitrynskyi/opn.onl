import { randomUUID } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import {
    test as base,
    expect,
    type APIRequestContext,
    type Page,
    type PlaywrightWorkerArgs,
    type Response,
} from '@playwright/test';
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
 * The dashboard (/dashboard) against the real backend: a signed-in user's
 * links, the create form, and the per-link actions.
 */

interface CreatedLink {
    id: number;
    code: string;
    original_url: string;
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

// Registration is slow (bcrypt), so the tests of one worker share an account.
// Each test adds its own links and finds them by id; the tests that look at the
// whole list (totals, the empty state) register an account of their own.
const test = base.extend<Record<never, never>, { owner: TestUser }>({
    owner: [
        async ({ playwright }, provide) => {
            await provide(await withRequest(playwright, (request) => createUser(request)));
        },
        { scope: 'worker', timeout: 60_000 },
    ],
});

// Give every page its own client address. The backend rate-limits by
// CF-Connecting-IP; without this every dashboard load in the suite would draw
// on the 127.0.0.1 buckets, including the 10-a-minute auth bucket that
// GET /auth/settings counts against.
test.beforeEach(async ({ page }) => {
    await page.setExtraHTTPHeaders(clientIpHeader());
});

/**
 * A destination no other link uses. The backend refuses to shorten the same
 * URL more than 10 times in 10 minutes for one account, and the shared account
 * creates more links than that.
 */
function uniqueDestination(): string {
    return `https://www.iana.org/help/example-domains?e2e=${randomUUID().slice(0, 8)}`;
}

/** The short URL as the dashboard prints it: the frontend origin without the scheme. */
function shortText(code: string): string {
    return `${new URL(WEB_URL).host}/${code}`;
}

/** The destination as the dashboard prints it. */
function destinationText(url: string): string {
    return url.replace(/^https?:\/\//, '').substring(0, 50) + (url.length > 60 ? '...' : '');
}

/** A link's row in the list, found through its analytics link (which carries the id). */
function linkRow(page: Page, link: { id: number }) {
    return page.locator('div.group', { has: page.locator(`a[href="/analytics/${link.id}"]`) });
}

function isApiCall(res: Response, method: string, path: string): boolean {
    return res.url() === `${API_URL}${path}` && res.request().method() === method;
}

/** Open the dashboard and wait for the backend to answer its links request. */
async function openDashboard(page: Page) {
    const answered = page.waitForResponse((res) => isApiCall(res, 'GET', '/links'));
    await page.goto('/dashboard');
    return answered;
}

/** `YYYY-MM-DD`, `days` from now. */
function dayFromNow(days: number): string {
    return new Date(Date.now() + days * 86_400_000).toISOString().slice(0, 10);
}

/** Click `code` `times` times through the backend redirect, each from a new address. */
async function click(request: APIRequestContext, code: string, times: number): Promise<void> {
    for (let i = 0; i < times; i++) {
        const res = await request.get(`${API_URL}/${code}`, { maxRedirects: 0, headers: clientIpHeader() });
        expect(res.status(), `redirect /${code}`).toBe(307);
    }
}

/**
 * Wait until GET /links reports `counts` (link id -> click_count). Clicks sit
 * in the backend's buffer for up to CLICK_FLUSH_INTERVAL (5 s by default).
 */
async function waitForClickCounts(request: APIRequestContext, token: string, counts: Record<number, number>) {
    await expect
        .poll(
            async () => {
                const res = await api(request, 'get', '/links', token);
                if (!res.ok()) return `HTTP ${res.status()}`;
                const links: { id: number; click_count: number }[] = await res.json();
                return Object.fromEntries(links.filter((l) => l.id in counts).map((l) => [l.id, l.click_count]));
            },
            { message: 'click counts should be recorded', timeout: 20_000 },
        )
        .toEqual(counts);
}

test.describe('Dashboard Page', () => {
    test('should display dashboard header', async ({ page, owner }) => {
        await signIn(page, owner);
        await openDashboard(page);
        await expect(page.getByRole('heading', { name: 'Dashboard', level: 1 })).toBeVisible();
        await expect(page).toHaveURL(/\/dashboard$/);
    });

    test('should display link statistics', async ({ page, request }) => {
        const user = await createUser(request);
        const busy: CreatedLink = await createLink(request, user.token);
        const quiet: CreatedLink = await createLink(request, user.token);
        // Not active until next week.
        const scheduled: CreatedLink = await createLink(request, user.token, {
            starts_at: `${dayFromNow(7)}T12:00:00.000Z`,
        });
        await click(request, busy.code, 3);
        await click(request, quiet.code, 1);
        await waitForClickCounts(request, user.token, { [busy.id]: 3, [quiet.id]: 1, [scheduled.id]: 0 });

        await signIn(page, user);
        await openDashboard(page);
        await expect(page.getByText('3 links', { exact: true })).toBeVisible();
        await expect(page.getByText('2 active', { exact: true })).toBeVisible();
        await expect(page.getByText('4 clicks', { exact: true })).toBeVisible();
        await expect(linkRow(page, scheduled).getByText('Inactive', { exact: true })).toBeVisible();
    });

    test("should display the user's links and no one else's", async ({ page, request, owner }) => {
        const mine: CreatedLink[] = [
            await createLink(request, owner.token, { original_url: uniqueDestination() }),
            await createLink(request, owner.token, { original_url: uniqueDestination() }),
        ];
        const stranger = await createUser(request);
        const theirs: CreatedLink = await createLink(request, stranger.token);

        await signIn(page, owner);
        await openDashboard(page);
        for (const link of mine) {
            await expect(page.getByRole('link', { name: shortText(link.code), exact: true })).toBeVisible();
            await expect(
                linkRow(page, link).getByRole('link', { name: destinationText(link.original_url), exact: true }),
            ).toBeVisible();
        }
        await expect(page.locator(`a[href$="/${theirs.code}"]`)).toHaveCount(0);
    });

    test('should display click counts', async ({ page, request, owner }) => {
        const clicked: CreatedLink = await createLink(request, owner.token, { original_url: uniqueDestination() });
        const unclicked: CreatedLink = await createLink(request, owner.token, { original_url: uniqueDestination() });
        await click(request, clicked.code, 2);
        await waitForClickCounts(request, owner.token, { [clicked.id]: 2, [unclicked.id]: 0 });

        await signIn(page, owner);
        await openDashboard(page);
        await expect(page.locator(`a[href="/analytics/${clicked.id}"]`)).toHaveText('2');
        await expect(page.locator(`a[href="/analytics/${unclicked.id}"]`)).toHaveText('0');
    });

    test('should show password protected badge', async ({ page, request, owner }) => {
        const locked: CreatedLink = await createLink(request, owner.token, {
            original_url: uniqueDestination(),
            password: 'correct-horse-battery',
        });
        const open: CreatedLink = await createLink(request, owner.token, { original_url: uniqueDestination() });

        await signIn(page, owner);
        await openDashboard(page);
        await expect(linkRow(page, locked).getByText('Protected', { exact: true })).toBeVisible();
        await expect(linkRow(page, open)).toBeVisible();
        await expect(linkRow(page, open).getByText('Protected', { exact: true })).toHaveCount(0);
    });

    test('should show expiration badge', async ({ page, request, owner }) => {
        // Noon UTC falls on the same calendar day in every time zone the suite runs in.
        const expiresAt = `${dayFromNow(45)}T12:00:00.000Z`;
        const expiring: CreatedLink = await createLink(request, owner.token, {
            original_url: uniqueDestination(),
            expires_at: expiresAt,
        });

        await signIn(page, owner);
        await openDashboard(page);
        const day = await page.evaluate((instant) => new Date(instant).toLocaleDateString(), expiresAt);
        await expect(linkRow(page, expiring)).toContainText(day);
    });

    test.describe('in New York, west of UTC', () => {
        test.use({ timezoneId: 'America/New_York' });

        test('should show the expiry date the user picked', async ({ page, owner }) => {
            await signIn(page, owner);
            await openDashboard(page);

            const picked = dayFromNow(45);
            await page.getByPlaceholder('https://example.com/long-url').fill(uniqueDestination());
            await page.getByRole('button', { name: 'Advanced options' }).click();
            // The time defaults to 23:59 local, the end of the picked day.
            await page.getByLabel('Expiration', { exact: true }).fill(picked);
            const created = page.waitForResponse((res) => isApiCall(res, 'POST', '/links'));
            await page.getByRole('button', { name: 'Create', exact: true }).click();
            const res = await created;
            expect(res.status()).toBe(201);
            const link: CreatedLink = await res.json();

            const pickedDay = await page.evaluate((ymd) => {
                const [y, m, d] = ymd.split('-').map(Number);
                return new Date(y, m - 1, d).toLocaleDateString();
            }, picked);
            await expect(linkRow(page, link)).toContainText(pickedDay);
        });
    });

    test('should have create link form', async ({ page, owner }) => {
        await signIn(page, owner);
        await openDashboard(page);
        await expect(page.getByRole('heading', { name: 'Create new link' })).toBeVisible();
        const url = page.getByPlaceholder('https://example.com/long-url');
        await expect(url).toBeVisible();
        await expect(url).toHaveAttribute('type', 'url');
        await expect(url).toHaveAttribute('required', '');
        await expect(page.getByPlaceholder(/^alias \(\d+-\d+ chars\)$/)).toBeVisible();
        await expect(page.getByRole('button', { name: 'Create', exact: true })).toBeEnabled();
    });

    test('should toggle advanced options', async ({ page, owner }) => {
        await signIn(page, owner);
        await openDashboard(page);
        const toggle = page.getByRole('button', { name: 'Advanced options' });
        const fields = [
            page.getByLabel('Title (private, only visible to you)'),
            page.getByLabel('Password Protection'),
            page.getByLabel('Expiration', { exact: true }),
            page.getByLabel('Expiration time'),
        ];
        for (const field of fields) await expect(field).toBeHidden();

        await toggle.click();
        for (const field of fields) await expect(field).toBeVisible();

        await toggle.click();
        for (const field of fields) await expect(field).toBeHidden();
    });

    test('should filter links by search', async ({ page, request, owner }) => {
        const tag = randomUUID().slice(0, 8);
        const alpha: CreatedLink = await createLink(request, owner.token, {
            original_url: uniqueDestination(),
            custom_alias: `alpha${tag}`,
        });
        const bravo: CreatedLink = await createLink(request, owner.token, {
            original_url: uniqueDestination(),
            custom_alias: `bravo${tag}`,
        });

        await signIn(page, owner);
        await openDashboard(page);
        const search = page.getByPlaceholder('Search links, notes, tags...');
        await search.fill(`alpha${tag}`);
        await expect(page.getByRole('link', { name: shortText(alpha.code), exact: true })).toBeVisible();
        await expect(page.getByRole('link', { name: shortText(bravo.code), exact: true })).toHaveCount(0);

        await search.fill(`zulu${tag}`);
        await expect(page.getByText(`No links found matching "zulu${tag}"`)).toBeVisible();
        await expect(page.getByRole('link', { name: shortText(alpha.code), exact: true })).toHaveCount(0);
    });

    test('should clear search filter', async ({ page, request, owner }) => {
        const tag = randomUUID().slice(0, 8);
        const alpha: CreatedLink = await createLink(request, owner.token, {
            original_url: uniqueDestination(),
            custom_alias: `alpha${tag}`,
        });
        const bravo: CreatedLink = await createLink(request, owner.token, {
            original_url: uniqueDestination(),
            custom_alias: `bravo${tag}`,
        });

        await signIn(page, owner);
        await openDashboard(page);
        const search = page.getByPlaceholder('Search links, notes, tags...');
        await search.fill(`alpha${tag}`);
        await expect(page.getByRole('link', { name: shortText(bravo.code), exact: true })).toHaveCount(0);

        await search.clear();
        await expect(page.getByRole('link', { name: shortText(alpha.code), exact: true })).toBeVisible();
        await expect(page.getByRole('link', { name: shortText(bravo.code), exact: true })).toBeVisible();
    });

    test('should copy link to clipboard', async ({ page, request, owner }) => {
        await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
        const link: CreatedLink = await createLink(request, owner.token, { original_url: uniqueDestination() });

        await signIn(page, owner);
        await openDashboard(page);
        await linkRow(page, link).getByRole('button', { name: 'Copy short URL' }).click();
        await expect(page.getByText('Short link copied!')).toBeVisible();
        const copied = await page.evaluate(() => navigator.clipboard.readText());
        expect(copied).toBe(`${new URL(page.url()).origin}/${link.code}`);
    });

    test('should export links as CSV', async ({ page, request, owner }) => {
        const link: CreatedLink = await createLink(request, owner.token, { original_url: uniqueDestination() });

        await signIn(page, owner);
        await openDashboard(page);
        const download = page.waitForEvent('download');
        await page.getByRole('button', { name: 'Export CSV' }).click();
        const file = await download;
        expect(file.suggestedFilename()).toBe('opn_onl_links.csv');

        const [header, ...rows] = (await readFile(await file.path(), 'utf8')).trim().split('\n');
        expect(header).toBe(
            'ID,Code,Original URL,Short URL,Click Count,Created At,Expires At,Has Password,Notes,Folder ID,Max Clicks,Starts At',
        );
        expect(rows.find((row) => row.startsWith(`${link.id},`))).toContain(
            `${link.id},"${link.code}","${link.original_url}",`,
        );
    });

    test('should create new link', async ({ page, owner }) => {
        await signIn(page, owner);
        await openDashboard(page);
        const destination = uniqueDestination();
        const url = page.getByPlaceholder('https://example.com/long-url');
        await url.fill(destination);
        const created = page.waitForResponse((res) => isApiCall(res, 'POST', '/links'));
        await page.getByRole('button', { name: 'Create', exact: true }).click();
        const res = await created;
        expect(res.status()).toBe(201);
        const link: CreatedLink = await res.json();
        expect(link.original_url).toBe(destination);

        await expect(page.getByRole('link', { name: shortText(link.code), exact: true })).toBeVisible();
        await expect(
            linkRow(page, link).getByRole('link', { name: destinationText(destination), exact: true }),
        ).toBeVisible();
        await expect(url).toHaveValue('');
    });

    test('should create link with custom alias', async ({ page, request, owner }) => {
        await signIn(page, owner);
        await openDashboard(page);
        const destination = uniqueDestination();
        const alias = `e2e${randomUUID().slice(0, 8)}`;
        await page.getByPlaceholder('https://example.com/long-url').fill(destination);
        await page.getByPlaceholder(/^alias \(/).fill(alias);
        const created = page.waitForResponse((res) => isApiCall(res, 'POST', '/links'));
        await page.getByRole('button', { name: 'Create', exact: true }).click();
        const res = await created;
        expect(res.status()).toBe(201);
        expect((await res.json()).code).toBe(alias);

        await expect(page.getByRole('link', { name: shortText(alias), exact: true })).toBeVisible();
        // The alias is live: the backend redirects it to the destination.
        const redirect = await request.get(`${API_URL}/${alias}`, { maxRedirects: 0, headers: clientIpHeader() });
        expect(redirect.status()).toBe(307);
        expect(redirect.headers()['location']).toBe(destination);
    });
});

test.describe('Dashboard - Empty State', () => {
    test('should show empty state', async ({ page, request }) => {
        const newcomer = await createUser(request);
        await signIn(page, newcomer);
        await openDashboard(page);
        await expect(page.getByText('0 links', { exact: true })).toBeVisible();
        await expect(page.getByRole('heading', { name: 'No links yet' })).toBeVisible();
        await expect(page.getByText('Create your first shortened link above.')).toBeVisible();

        await page.getByRole('button', { name: 'Start shortening' }).click();
        await expect(page.getByPlaceholder('https://example.com/long-url')).toBeFocused();
    });
});

test.describe('Dashboard - Link Actions', () => {
    let link: CreatedLink;

    test.beforeEach(async ({ page, request, owner }) => {
        link = await createLink(request, owner.token, { original_url: uniqueDestination() });
        await signIn(page, owner);
        await openDashboard(page);
    });

    test('should navigate to analytics', async ({ page }) => {
        await page.locator(`a[href="/analytics/${link.id}"]`).click();
        await expect(page).toHaveURL(new RegExp(`/analytics/${link.id}$`));
        await expect(page.getByRole('heading', { level: 1 })).toHaveText(`${new URL(WEB_URL).host}/${link.code}`);
    });

    test('should open QR code modal', async ({ page }) => {
        const qrLoaded = page.waitForResponse(
            (res) => new URL(res.url()).pathname === `/links/${link.id}/qr` && res.request().method() === 'GET',
        );
        await linkRow(page, link).getByRole('button', { name: 'Show QR code' }).click();
        const dialog = page.getByRole('dialog');
        await expect(dialog.getByRole('heading', { name: 'QR code' })).toBeVisible();
        await expect(dialog.getByText(link.code, { exact: true })).toBeVisible();
        expect((await qrLoaded).status()).toBe(200);
        const qr = dialog.getByRole('img', { name: 'QR Code' });
        await expect(qr).toBeVisible();
        await expect.poll(() => qr.evaluate((img: HTMLImageElement) => img.naturalWidth)).toBeGreaterThan(0);
    });

    test('should edit a link destination', async ({ page, request }) => {
        await linkRow(page, link).getByRole('button', { name: 'Edit link' }).click();
        const dialog = page.getByRole('dialog');
        await expect(dialog.getByRole('heading', { name: 'Edit link' })).toBeVisible();
        const destination = dialog.getByLabel('Destination URL');
        await expect(destination).toHaveValue(link.original_url);

        const next = uniqueDestination();
        await destination.fill(next);
        const saved = page.waitForResponse((res) => isApiCall(res, 'PUT', `/links/${link.id}`));
        await dialog.getByRole('button', { name: 'Save changes' }).click();
        expect((await saved).status()).toBe(200);
        await expect(dialog).toBeHidden();
        await expect(linkRow(page, link).getByRole('link', { name: destinationText(next), exact: true })).toBeVisible();

        // The short link now redirects to the new destination.
        const redirect = await request.get(`${API_URL}/${link.code}`, { maxRedirects: 0, headers: clientIpHeader() });
        expect(redirect.status()).toBe(307);
        expect(redirect.headers()['location']).toBe(next);
    });
});
