import { randomUUID } from 'node:crypto';
import { test as base, expect, type APIRequestContext, type PlaywrightWorkerArgs } from '@playwright/test';
import { API_URL, clientIpHeader, createLink, createUser, type TestUser } from './support/api';

/**
 * Password-protected short links: the prompt page (/password/:code) and the
 * whole visitor journey through the real backend (/:code/preview,
 * /:code/verify and the unlock redirect).
 */

const LINK_PASSWORD = 'correct horse battery staple';

/**
 * The password prompt and the short-link redirect page are lazy routes, compiled
 * by the dev server on first use; that can take well over the default 5s on a
 * cold or busy machine. Checks that one has loaded wait this long; checks of
 * what the page then does keep the default.
 */
const LAZY_PAGE = { timeout: 20_000 };

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

// Registration is slow (bcrypt), so the tests of one worker share the account
// that owns their links; every test creates a link of its own.
const test = base.extend<Record<never, never>, { owner: TestUser }>({
    owner: [
        async ({ playwright }, provide) => {
            await provide(await withRequest(playwright, (request) => createUser(request)));
        },
        { scope: 'worker', timeout: 60_000 },
    ],
});

// Give every page its own client address. The backend rate-limits by
// CF-Connecting-IP, and password checks have the tightest buckets (20 a minute
// per IP, 5 a minute per IP and code); without this every browser in the suite
// would share 127.0.0.1's.
test.beforeEach(async ({ page }) => {
    await page.setExtraHTTPHeaders(clientIpHeader());
});

/**
 * A destination the browser can be sent to but can never load. Playwright does
 * not route redirect targets, so the destination host cannot simply be
 * aborted; `.alt` is reserved for names outside the DNS (RFC 9476), so the
 * lookup fails and nothing external is fetched. The backend accepts it.
 */
function unreachableDestination(): string {
    return `https://landing.opn-e2e.alt/welcome?run=${randomUUID()}`;
}

test.describe('Password prompt page', () => {
    test.beforeEach(async ({ page }) => {
        // The prompt renders without asking the backend anything; whether the
        // code exists is only learned on submit (covered below).
        await page.goto('/password/abc123');
        await expect(page.getByRole('heading', { name: 'Password Protected' })).toBeVisible(LAZY_PAGE);
    });

    test('asks for the link password', async ({ page }) => {
        await expect(page.getByRole('heading', { name: 'Password Protected' })).toBeVisible();
        await expect(page.getByText('This link is protected. Enter the password to continue.')).toBeVisible();
        await expect(page.getByLabel('Password')).toHaveAttribute('placeholder', 'Enter the link password');
        await expect(page.getByLabel('Password')).toBeFocused();
        await expect(page.getByText(/connection is secure/i)).toBeVisible();
    });

    test('enables Continue only while a password is typed', async ({ page }) => {
        const button = page.getByRole('button', { name: 'Continue' });
        await expect(button).toBeDisabled();

        await page.getByLabel('Password').fill('something');
        await expect(button).toBeEnabled();

        await page.getByLabel('Password').fill('');
        await expect(button).toBeDisabled();
    });

    test('offers a way back to the homepage', async ({ page }) => {
        await page.getByRole('button', { name: 'Go to homepage' }).click();
        await expect(page).toHaveURL('/');
        await expect(page.getByRole('heading', { level: 1 })).toContainText('Short links that');
    });
});

test.describe('Password-protected link against the real backend', () => {
    // Every password check is a bcrypt verification on the backend (about a
    // second each on a debug build, several under a parallel run), so these
    // tests get more than the default 30s.
    test.describe.configure({ timeout: 90_000 });

    test('visitor opens the short link, is refused a wrong password and sent on with the right one', async ({
        page,
        request,
        owner,
    }) => {
        const destination = unreachableDestination();
        const link = await createLink(request, owner.token, {
            original_url: destination,
            password: LINK_PASSWORD,
        });

        // A visitor only has the short link; the SPA learns from the preview
        // endpoint that it is protected and asks for the password.
        await page.goto(`/${link.code}`);
        await expect(page).toHaveURL(`/password/${link.code}`, LAZY_PAGE);
        await expect(page.getByRole('heading', { name: 'Password Protected' })).toBeVisible();

        await page.getByLabel('Password').fill('not the password');
        await page.getByRole('button', { name: 'Continue' }).click();
        await expect(page.getByText('Incorrect password. Please try again.')).toBeVisible();
        await expect(page).toHaveURL(`/password/${link.code}`);

        // The right password buys an unlock token; the backend redirect
        // endpoint checks it and sends the browser to the destination.
        const toDestination = page.waitForRequest((r) => r.url() === destination);
        await page.getByLabel('Password').fill(LINK_PASSWORD);
        await page.getByRole('button', { name: 'Continue' }).click();

        const redirectedFrom = (await toDestination).redirectedFrom();
        expect(redirectedFrom, 'the destination is reached through a backend redirect').not.toBeNull();
        expect(redirectedFrom!.url().startsWith(`${API_URL}/${link.code}?unlock=`)).toBe(true);
    });

    test('locks the prompt after five wrong guesses, even for the right password', async ({
        page,
        request,
        owner,
    }) => {
        const link = await createLink(request, owner.token, {
            original_url: unreachableDestination(),
            password: LINK_PASSWORD,
        });
        await page.goto(`/password/${link.code}`);
        await expect(page.getByRole('heading', { name: 'Password Protected' })).toBeVisible(LAZY_PAGE);

        const password = page.getByLabel('Password');
        const submit = page.getByRole('button', { name: 'Continue' });
        const verifyUrl = `${API_URL}/${link.code}/verify`;

        for (let guess = 1; guess <= 5; guess++) {
            const verified = page.waitForResponse((r) => r.url() === verifyUrl);
            await password.fill(`wrong guess ${guess}`);
            await submit.click();
            expect((await verified).status()).toBe(401);
            await expect(page.getByText('Incorrect password. Please try again.')).toBeVisible();
        }

        const verified = page.waitForResponse((r) => r.url() === verifyUrl);
        await password.fill(LINK_PASSWORD);
        await submit.click();
        expect((await verified).status()).toBe(429);
        await expect(page.getByText(/^Too many attempts\. Please try again in \d+ seconds\.$/)).toBeVisible();
        await expect(page).toHaveURL(`/password/${link.code}`);
    });

    test('tells a locked-out visitor the wait the server asks for', async ({ page, request, owner }) => {
        const link = await createLink(request, owner.token, {
            original_url: unreachableDestination(),
            password: LINK_PASSWORD,
        });

        // Spend this visitor's five guesses for the code before the page opens.
        const visitor = clientIpHeader();
        await page.setExtraHTTPHeaders(visitor);
        for (let guess = 1; guess <= 5; guess++) {
            const res = await request.post(`${API_URL}/${link.code}/verify`, {
                data: { password: `wrong guess ${guess}` },
                headers: visitor,
            });
            expect(res.status()).toBe(401);
        }

        await page.goto(`/password/${link.code}`);
        await expect(page.getByRole('heading', { name: 'Password Protected' })).toBeVisible(LAZY_PAGE);
        const verified = page.waitForResponse((r) => r.url() === `${API_URL}/${link.code}/verify`);
        await page.getByLabel('Password').fill(LINK_PASSWORD);
        await page.getByRole('button', { name: 'Continue' }).click();

        const response = await verified;
        expect(response.status()).toBe(429);
        const wait = response.headers()['retry-after'];
        expect(wait).toMatch(/^\d+$/);
        await expect(page.getByText(`Too many attempts. Please try again in ${wait} seconds.`)).toBeVisible();
    });

    test('says so when a protected link has expired', async ({ page, request, owner }) => {
        const link = await createLink(request, owner.token, {
            original_url: unreachableDestination(),
            password: LINK_PASSWORD,
            expires_at: new Date(Date.now() - 60_000).toISOString(),
        });
        await page.goto(`/password/${link.code}`);
        await expect(page.getByRole('heading', { name: 'Password Protected' })).toBeVisible(LAZY_PAGE);

        await page.getByLabel('Password').fill(LINK_PASSWORD);
        await page.getByRole('button', { name: 'Continue' }).click();

        await expect(page.getByText('This link has expired.')).toBeVisible();
        await expect(page).toHaveURL(`/password/${link.code}`);
    });

    test('says so when the link does not exist', async ({ page }) => {
        await page.goto(`/password/missing-${randomUUID().slice(0, 8)}`);
        await expect(page.getByRole('heading', { name: 'Password Protected' })).toBeVisible(LAZY_PAGE);

        await page.getByLabel('Password').fill(LINK_PASSWORD);
        await page.getByRole('button', { name: 'Continue' }).click();

        await expect(page.getByText('Link not found.')).toBeVisible();
    });

    test('explains a network failure', async ({ page }) => {
        // A dropped connection cannot be produced on demand against the real
        // backend. The verify call is the only request this page makes, so
        // failing it covers everything the page talks to.
        await page.route(`${API_URL}/*/verify`, (route) => route.abort('internetdisconnected'));
        await page.goto('/password/abc123');
        await expect(page.getByRole('heading', { name: 'Password Protected' })).toBeVisible(LAZY_PAGE);

        await page.getByLabel('Password').fill(LINK_PASSWORD);
        await page.getByRole('button', { name: 'Continue' }).click();

        await expect(page.getByText('Network error. Please check your connection and try again.')).toBeVisible();
        await expect(page).toHaveURL('/password/abc123');
    });
});
