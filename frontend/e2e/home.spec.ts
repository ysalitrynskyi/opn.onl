import { randomUUID } from 'node:crypto';
import { test as base, expect, type APIRequestContext, type PlaywrightWorkerArgs } from '@playwright/test';
import {
    API_URL,
    TEST_PASSWORD,
    api,
    clientIpHeader,
    createUser,
    signIn,
    uniqueEmail,
    type TestUser,
} from './support/api';

/**
 * The dashboard is a lazy route, compiled by the dev server on first use; that
 * can take well over the default 5s on a cold or busy machine. Checks that it
 * has loaded wait this long; checks of what the page then does keep the default.
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

// Registration is slow (bcrypt), so the signed-in tests of one worker share an
// account; each one shortens a URL of its own and finds it by code.
const test = base.extend<Record<never, never>, { account: TestUser }>({
    account: [
        async ({ playwright }, provide) => {
            await provide(await withRequest(playwright, (request) => createUser(request)));
        },
        { scope: 'worker', timeout: 60_000 },
    ],
});

// Give every page its own client address. The backend rate-limits by
// CF-Connecting-IP (link creation: 100 an hour; everything under /auth/: 10 a
// minute); without this every browser in the suite would share 127.0.0.1's.
test.beforeEach(async ({ page }) => {
    await page.setExtraHTTPHeaders(clientIpHeader());
});

/** A destination the backend accepts; the tests never load it. */
function longUrl(): string {
    return `https://www.iana.org/help/example-domains?utm_source=e2e&run=${randomUUID()}`;
}

test.describe('Home page', () => {
    test.beforeEach(async ({ page }) => {
        await page.goto('/');
    });

    test('leads with the hero', async ({ page }) => {
        await expect(page).toHaveTitle('opn.onl - Open Source URL Shortener');
        await expect(page.getByRole('heading', { level: 1 })).toHaveText(/^Short links that\s*answer to you\.$/);
        await expect(page.getByText('A privacy-first URL shortener you actually own.')).toBeVisible();
    });

    test('shows the shortener form', async ({ page }) => {
        const input = page.getByRole('textbox', { name: 'URL to shorten' });
        await expect(input).toBeVisible();
        await expect(input).toHaveAttribute('placeholder', 'https://your-very-long-link.com/…');
        await expect(page.getByRole('button', { name: 'Shorten' })).toBeEnabled();
    });

    test('lists the core features', async ({ page }) => {
        for (const title of [
            'Rust-fast redirects',
            'Privacy by default',
            'Honest analytics',
            'Branded QR codes',
            'Password & limits',
            'Expiring links',
        ]) {
            await expect(page.getByRole('heading', { level: 3, name: title })).toBeVisible();
        }
    });

    test('header links to Features', async ({ page }) => {
        await page.getByRole('banner').getByRole('link', { name: 'Features' }).click();
        await expect(page).toHaveURL('/features');
    });

    test('header links to Pricing', async ({ page }) => {
        await page.getByRole('banner').getByRole('link', { name: 'Pricing' }).click();
        await expect(page).toHaveURL('/pricing');
    });

    test('header links to Log in', async ({ page }) => {
        await page.getByRole('banner').getByRole('link', { name: 'Log in' }).click();
        await expect(page).toHaveURL('/login');
    });

    test('header links to Sign up', async ({ page }) => {
        await page.getByRole('banner').getByRole('link', { name: 'Sign up' }).click();
        await expect(page).toHaveURL('/register');
    });

    test('sends a signed-out visitor to sign up instead of shortening', async ({ page }) => {
        const requests: string[] = [];
        page.on('request', (r) => {
            if (r.url().startsWith(API_URL)) requests.push(r.url());
        });

        await page.getByRole('textbox', { name: 'URL to shorten' }).fill(longUrl());
        await page.getByRole('button', { name: 'Shorten' }).click();

        await expect(page).toHaveURL('/register');
        expect(requests, 'nothing is sent to the API without an account').toEqual([]);
    });

    test('links the terms and privacy policy under the form', async ({ page }) => {
        const consent = page.getByText('By shortening a link you accept our');
        await expect(consent.getByRole('link', { name: 'Privacy Policy' })).toHaveAttribute('href', '/privacy');

        await consent.getByRole('link', { name: 'Terms' }).click();
        await expect(page).toHaveURL('/terms');
        await expect(page.getByRole('heading', { level: 1, name: 'Terms of Service' })).toBeVisible();
    });

    test('footer links reach About and Contact', async ({ page }) => {
        await page.getByRole('contentinfo').getByRole('link', { name: 'About' }).click();
        await expect(page).toHaveURL('/about');
        await expect(page.getByRole('heading', { level: 1, name: 'About opn.onl' })).toBeVisible();

        await page.getByRole('contentinfo').getByRole('link', { name: 'Contact' }).click();
        await expect(page).toHaveURL('/contact');
        await expect(page.getByRole('heading', { level: 1, name: 'Contact Us' })).toBeVisible();
    });
});

test.describe('Home page shortener with a real account', () => {
    // Real accounts: every registration is a bcrypt hash on the backend (about
    // a second on a debug build, several under a parallel run), so these tests
    // get more than the default 30s.
    test.describe.configure({ timeout: 90_000 });

    test('shortens a URL for a signed-in user', async ({ page, request, account }) => {
        await signIn(page, account);
        await page.goto('/');

        const url = longUrl();
        await page.getByRole('textbox', { name: 'URL to shorten' }).fill(url);
        const created = page.waitForResponse(
            (r) => r.url() === `${API_URL}/links` && r.request().method() === 'POST',
        );
        await page.getByRole('button', { name: 'Shorten' }).click();

        const response = await created;
        expect(response.status()).toBe(201);
        const link = await response.json();

        const result = page.getByRole('link', { name: link.short_url });
        await expect(result).toBeVisible();
        await expect(result).toHaveAttribute('href', link.short_url);
        expect(link.short_url.endsWith(`/${link.code}`)).toBe(true);
        await expect(page.getByRole('button', { name: 'Copy' })).toBeVisible();

        // The link now belongs to this account and points where the user asked.
        const mine = await api(request, 'get', '/links', account.token);
        expect(mine.status()).toBe(200);
        const owned = (await mine.json()).find((l: { code: string }) => l.code === link.code);
        expect(owned?.original_url).toBe(url);
    });

    test('shows why the backend refused a URL', async ({ page, account }) => {
        await signIn(page, account);
        await page.goto('/');

        await page.getByRole('textbox', { name: 'URL to shorten' }).fill('http://localhost:8080/admin');
        await page.getByRole('button', { name: 'Shorten' }).click();

        await expect(page.getByRole('alert')).toHaveText('Links to local/internal hosts are not allowed');
        await expect(page.getByRole('button', { name: 'Copy' })).toHaveCount(0);
    });

    test('a URL pasted while signed out is waiting in the dashboard after sign-up', async ({ page }) => {
        await page.goto('/');
        const url = longUrl();

        await page.getByRole('textbox', { name: 'URL to shorten' }).fill(url);
        await page.getByRole('button', { name: 'Shorten' }).click();
        await expect(page).toHaveURL('/register');

        await page.getByLabel('Email address').fill(uniqueEmail('home'));
        await page.getByLabel('Password').fill(TEST_PASSWORD);
        // Registration hashes the password with bcrypt, which can outlast the
        // default expect timeout on a loaded backend: wait for the response.
        const registered = page.waitForResponse((r) => r.url() === `${API_URL}/auth/register`);
        await page.getByRole('button', { name: 'Create account' }).click();
        expect((await registered).status()).toBe(201);
        await expect(page.getByRole('heading', { name: 'Check your email' })).toBeVisible();

        await page.getByRole('button', { name: 'Continue to dashboard' }).click();
        await expect(page).toHaveURL('/dashboard');
        await expect(page.getByPlaceholder('https://example.com/long-url')).toHaveValue(url, LAZY_PAGE);
    });
});

test.describe('Home page on a phone', () => {
    test.use({ viewport: { width: 375, height: 667 } });

    test('collapses the navigation behind a menu button', async ({ page }) => {
        await page.goto('/');

        await expect(page.getByRole('button', { name: 'Toggle menu' })).toBeVisible();
        await expect(page.getByRole('banner').getByRole('link', { name: 'Features' })).toBeHidden();
        await expect(page.getByRole('banner').getByRole('link', { name: 'Sign up' })).toBeHidden();
    });

    test('opens the menu and navigates from it', async ({ page }) => {
        await page.goto('/');
        await page.getByRole('button', { name: 'Toggle menu' }).click();

        const header = page.getByRole('banner');
        for (const name of ['Features', 'Pricing', 'Developers', 'Docs', 'FAQ', 'Log in', 'Sign up']) {
            await expect(header.getByRole('link', { name, exact: true })).toBeVisible();
        }

        await header.getByRole('link', { name: 'Pricing', exact: true }).click();
        await expect(page).toHaveURL('/pricing');
        // Following a link closes the menu again.
        await expect(header.getByRole('link', { name: 'Features', exact: true })).toBeHidden();
    });
});
