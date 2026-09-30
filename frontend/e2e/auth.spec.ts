import { test as base, expect, type APIRequestContext, type Page, type PlaywrightWorkerArgs } from '@playwright/test';
import { API_URL, TEST_PASSWORD, api, clientIpHeader, createUser, uniqueEmail, type TestUser } from './support/api';

/**
 * The dashboard, settings, analytics and admin pages are lazy routes, compiled
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

// Registration is slow (bcrypt), so the tests of one worker that only sign in
// to an existing account share one. Signing in and out changes nothing on it.
const test = base.extend<Record<never, never>, { account: TestUser }>({
    account: [
        async ({ playwright }, provide) => {
            await provide(await withRequest(playwright, (request) => createUser(request)));
        },
        { scope: 'worker', timeout: 60_000 },
    ],
});

// Give every page its own client address. The backend rate-limits by
// CF-Connecting-IP, and everything under /auth/ shares one bucket of 10
// requests a minute per IP: login, register and the /auth/settings call the
// dashboard makes on load. Without this every browser in the suite would share
// 127.0.0.1's.
test.beforeEach(async ({ page }) => {
    await page.setExtraHTTPHeaders(clientIpHeader());
});

/** URLs of every request this page sends to `path` on the backend. */
function recordRequests(page: Page, path: string): string[] {
    const seen: string[] = [];
    page.on('request', (r) => {
        if (r.url() === `${API_URL}${path}`) seen.push(r.url());
    });
    return seen;
}

function storedToken(page: Page): Promise<string | null> {
    return page.evaluate(() => localStorage.getItem('token'));
}

test.describe('Login page', () => {
    test.beforeEach(async ({ page }) => {
        await page.goto('/login');
    });

    test('shows the sign-in form', async ({ page }) => {
        await expect(page.getByRole('heading', { level: 1, name: 'Sign in to opn.onl' })).toBeVisible();
        await expect(page.getByText('Welcome back')).toBeVisible();
        await expect(page.getByLabel('Email address')).toBeVisible();
        await expect(page.getByLabel('Password')).toBeVisible();
        await expect(page.getByRole('button', { name: 'Sign in', exact: true })).toBeEnabled();
        await expect(page.getByRole('link', { name: 'Forgot?' })).toHaveAttribute('href', '/forgot-password');
    });

    test('links to registration', async ({ page }) => {
        await page.getByRole('main').getByRole('link', { name: 'Create an account' }).click();
        await expect(page).toHaveURL('/register');
    });

    test('requires an email before sending anything', async ({ page }) => {
        const logins = recordRequests(page, '/auth/login');
        await page.getByLabel('Password').fill(TEST_PASSWORD);
        await page.getByRole('button', { name: 'Sign in', exact: true }).click();

        const email = page.getByLabel('Email address');
        await expect(email).toBeFocused();
        expect(await email.evaluate((el: HTMLInputElement) => el.validity.valueMissing)).toBe(true);
        expect(logins).toEqual([]);
    });

    test('requires a password before sending anything', async ({ page }) => {
        const logins = recordRequests(page, '/auth/login');
        await page.getByLabel('Email address').fill(uniqueEmail());
        await page.getByRole('button', { name: 'Sign in', exact: true }).click();

        const password = page.getByLabel('Password');
        await expect(password).toBeFocused();
        expect(await password.evaluate((el: HTMLInputElement) => el.validity.valueMissing)).toBe(true);
        expect(logins).toEqual([]);
    });

    test('rejects a malformed email before sending anything', async ({ page }) => {
        const logins = recordRequests(page, '/auth/login');
        await page.getByLabel('Email address').fill('invalid-email');
        await page.getByLabel('Password').fill(TEST_PASSWORD);
        await page.getByRole('button', { name: 'Sign in', exact: true }).click();

        const email = page.getByLabel('Email address');
        await expect(email).toBeFocused();
        expect(await email.evaluate((el: HTMLInputElement) => el.validity.typeMismatch)).toBe(true);
        expect(logins).toEqual([]);
    });

    test('offers passkey sign-in once an email is entered', async ({ page }) => {
        const passkey = page.getByRole('button', { name: 'Sign in with passkey' });
        await expect(passkey).toBeVisible();
        await expect(passkey).toBeDisabled();
        await expect(page.getByText('Enter your email first, then use your passkey.')).toBeVisible();

        await page.getByLabel('Email address').fill(uniqueEmail());
        await expect(passkey).toBeEnabled();
    });
});

test.describe('Signing in against the real backend', () => {
    // Real accounts: every register, login and password check is a bcrypt
    // hash on the backend (about a second each on a debug build, several under
    // a parallel run), so these tests get more than the default 30s.
    test.describe.configure({ timeout: 90_000 });

    test('signs a registered user in and lands on the dashboard', async ({ page, request, account }) => {
        await page.goto('/login');

        await page.getByLabel('Email address').fill(account.email);
        await page.getByLabel('Password').fill(account.password);
        const login = page.waitForResponse((r) => r.url() === `${API_URL}/auth/login`);
        await page.getByRole('button', { name: 'Sign in', exact: true }).click();
        expect((await login).status()).toBe(200);

        await expect(page).toHaveURL('/dashboard');
        await expect(page.getByRole('heading', { name: 'Create new link' })).toBeVisible(LAZY_PAGE);
        await expect(page.getByRole('button', { name: 'Account menu' })).toBeVisible();

        // The session the page stored is a working one for this account.
        const me = await api(request, 'get', '/auth/me', (await storedToken(page))!);
        expect(me.status()).toBe(200);
        expect((await me.json()).email).toBe(account.email);
    });

    test('refuses a wrong password', async ({ page, account }) => {
        await page.goto('/login');

        await page.getByLabel('Email address').fill(account.email);
        await page.getByLabel('Password').fill('not-the-password');
        const login = page.waitForResponse((r) => r.url() === `${API_URL}/auth/login`);
        await page.getByRole('button', { name: 'Sign in', exact: true }).click();
        expect((await login).status()).toBe(401);

        await expect(page.getByRole('alert')).toHaveText('Invalid credentials');
        await expect(page).toHaveURL('/login');
        expect(await storedToken(page)).toBeNull();
    });

    test('asks an unverified user to verify, and lets them continue', async ({ page, request }) => {
        const user = await createUser(request, { verified: false });
        await page.goto('/login');

        await page.getByLabel('Email address').fill(user.email);
        await page.getByLabel('Password').fill(user.password);
        const login = page.waitForResponse((r) => r.url() === `${API_URL}/auth/login`);
        await page.getByRole('button', { name: 'Sign in', exact: true }).click();
        expect((await login).status()).toBe(200);

        await expect(page.getByRole('heading', { name: 'Verify your email' })).toBeVisible();
        await expect(page.getByRole('button', { name: /resend verification email/i })).toBeVisible();

        await page.getByRole('button', { name: 'Continue to dashboard' }).click();
        await expect(page).toHaveURL('/dashboard');
        await expect(page.getByRole('heading', { name: 'Create new link' })).toBeVisible(LAZY_PAGE);
    });

    test('signs out from the account menu', async ({ page, account }) => {
        // Seed the session once rather than with signIn(): its init script
        // would put the token back on the next page load and hide a logout
        // that does not stick.
        await page.goto('/');
        await page.evaluate((token) => {
            localStorage.setItem('token', token);
            localStorage.setItem('is_admin', 'false');
        }, account.token);
        await page.goto('/dashboard');
        await expect(page.getByRole('heading', { name: 'Create new link' })).toBeVisible(LAZY_PAGE);

        await page.getByRole('button', { name: 'Account menu' }).click();
        await page.getByRole('button', { name: 'Log out' }).click();

        await expect(page).toHaveURL('/login');
        expect(await storedToken(page)).toBeNull();
        await expect(page.getByRole('banner').getByRole('link', { name: 'Log in' })).toBeVisible();

        await page.goto('/dashboard');
        await expect(page).toHaveURL('/login');
    });
});

test.describe('Register page', () => {
    test.beforeEach(async ({ page }) => {
        await page.goto('/register');
    });

    test('shows the registration form', async ({ page }) => {
        await expect(page.getByRole('heading', { level: 1, name: 'Create your account' })).toBeVisible();
        await expect(page.getByText('Free forever')).toBeVisible();
        await expect(page.getByLabel('Email address')).toBeVisible();
        await expect(page.getByLabel('Password')).toBeVisible();
        await expect(page.getByRole('button', { name: 'Create account' })).toBeVisible();
    });

    test('links to login', async ({ page }) => {
        await page.getByRole('main').getByRole('link', { name: 'Log in' }).click();
        await expect(page).toHaveURL('/login');
    });

    test('shows the 8-character rule as the password is typed', async ({ page }) => {
        const rule = page.getByText('At least 8 characters');
        await expect(rule).toHaveCount(0);

        await page.getByLabel('Password').fill('short');
        await expect(rule).toBeVisible();
        await expect(rule).toHaveClass(/text-faint/);

        await page.getByLabel('Password').fill('long enough');
        await expect(rule).toHaveClass(/text-success/);
    });

    test('links the terms and privacy policy', async ({ page }) => {
        const form = page.getByRole('main');
        await expect(form.getByRole('link', { name: 'Privacy Policy' })).toHaveAttribute('href', '/privacy');

        await form.getByRole('link', { name: 'Terms' }).click();
        await expect(page).toHaveURL('/terms');
    });

    test('enables Create account only from 8 characters', async ({ page }) => {
        const submit = page.getByRole('button', { name: 'Create account' });
        await page.getByLabel('Email address').fill(uniqueEmail());

        await page.getByLabel('Password').fill('1234567');
        await expect(submit).toBeDisabled();

        await page.getByLabel('Password').fill('12345678');
        await expect(submit).toBeEnabled();
    });
});

test.describe('Registering against the real backend', () => {
    // Real accounts: every register, login and password check is a bcrypt
    // hash on the backend (about a second each on a debug build, several under
    // a parallel run), so these tests get more than the default 30s.
    test.describe.configure({ timeout: 90_000 });

    test('creates the account and asks the new user to verify the email', async ({ page, request }) => {
        await page.goto('/register');
        const email = uniqueEmail('register');

        await page.getByLabel('Email address').fill(email);
        await page.getByLabel('Password').fill(TEST_PASSWORD);
        const registered = page.waitForResponse((r) => r.url() === `${API_URL}/auth/register`);
        await page.getByRole('button', { name: 'Create account' }).click();
        expect((await registered).status()).toBe(201);

        await expect(page.getByRole('heading', { name: 'Check your email' })).toBeVisible();
        await expect(page.getByText(email)).toBeVisible();

        // The page is now signed in as the new, still unverified, account.
        const me = await api(request, 'get', '/auth/me', (await storedToken(page))!);
        expect(me.status()).toBe(200);
        const profile = await me.json();
        expect(profile.email).toBe(email);
        expect(profile.email_verified).toBe(false);

        await page.getByRole('button', { name: 'Continue to dashboard' }).click();
        await expect(page).toHaveURL('/dashboard');
        await expect(page.getByRole('heading', { name: 'Create new link' })).toBeVisible(LAZY_PAGE);
    });

    test('refuses an email that is already registered', async ({ page, account }) => {
        await page.goto('/register');

        await page.getByLabel('Email address').fill(account.email);
        await page.getByLabel('Password').fill(TEST_PASSWORD);
        const registered = page.waitForResponse((r) => r.url() === `${API_URL}/auth/register`);
        await page.getByRole('button', { name: 'Create account' }).click();
        expect((await registered).status()).toBe(409);

        await expect(page.getByRole('alert')).toHaveText('Email already exists');
        await expect(page.getByRole('heading', { level: 1, name: 'Create your account' })).toBeVisible();
        expect(await storedToken(page)).toBeNull();
    });
});

test.describe('Protected routes', () => {
    for (const path of ['/dashboard', '/settings', '/analytics/1', '/admin']) {
        test(`${path} sends a signed-out visitor to login`, async ({ page }) => {
            await page.goto(path);
            await expect(page).toHaveURL('/login', LAZY_PAGE);
            await expect(page.getByRole('heading', { level: 1, name: 'Sign in to opn.onl' })).toBeVisible();
        });
    }
});
