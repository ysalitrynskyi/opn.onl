import { randomUUID } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import {
    test as base,
    expect,
    type APIRequestContext,
    type Page,
    type PlaywrightWorkerArgs,
} from '@playwright/test';
import { API_URL, api, clientIpHeader, createLink, createUser, signIn, type TestUser } from './support/api';

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

// Registration is slow (bcrypt), so the tests of one worker that only look at
// the page share an account. Tests that change the account (password, API
// keys) or count what is on it (links, the export) register one of their own.
const test = base.extend<Record<never, never>, { account: TestUser }>({
    account: [
        async ({ playwright }, provide) => {
            await provide(await withRequest(playwright, (request) => createUser(request)));
        },
        { scope: 'worker', timeout: 60_000 },
    ],
});

// Give every page its own client address. The backend meters every /auth/*
// request (login, but also /auth/me, /auth/settings, /auth/passkeys and
// /auth/api-keys) against one bucket of 10 a minute per CF-Connecting-IP, and
// this page makes four of those calls per load (eight under the dev server,
// where StrictMode runs the load effect twice). Without this every browser in
// the suite would share 127.0.0.1's bucket.
test.beforeEach(async ({ page }) => {
    await page.setExtraHTTPHeaders(clientIpHeader());
});

/** Open /settings signed in as `user` and wait until the account has loaded. */
async function openSettings(page: Page, user: TestUser): Promise<void> {
    await signIn(page, user);
    await page.goto('/settings');
    // The page is lazy-loaded and makes four API calls before it renders.
    await expect(page.getByText(user.email, { exact: true })).toBeVisible({ timeout: 30_000 });
}

function login(request: Parameters<typeof api>[0], email: string, password: string) {
    return api(request, 'post', '/auth/login', undefined, { email, password });
}

test.describe('Settings page', () => {
    // Real accounts: every registration, sign-in and password check is a
    // bcrypt hash on the backend (about a second each on a debug build, several
    // under a parallel run), so these tests get more than the default 30s.
    test.describe.configure({ timeout: 90_000 });

    test('shows the signed-in account', async ({ page, request }) => {
        const user = await createUser(request);
        await createLink(request, user.token);
        await openSettings(page, user);

        await expect(page.getByRole('heading', { level: 1, name: 'Settings' })).toBeVisible();
        await expect(page.getByText('Verified', { exact: true })).toBeVisible();
        await expect(page.getByText('Total Links', { exact: true }).locator('..')).toHaveText(/^Total Links\s*1$/);
        await expect(page.getByText('Total Clicks', { exact: true }).locator('..')).toHaveText(/^Total Clicks\s*0$/);
    });

    test('marks an unverified account and offers to resend the email', async ({ page, request }) => {
        const user = await createUser(request, { verified: false });
        await openSettings(page, user);

        await expect(page.getByText('Unverified', { exact: true })).toBeVisible();
        await expect(page.getByText('Email not verified')).toBeVisible();
        await expect(page.getByRole('button', { name: 'Resend' })).toBeEnabled();
    });

    test('shows an account without passkeys and offers to add one', async ({ page, account }) => {
        await openSettings(page, account);

        await expect(page.getByRole('heading', { level: 2, name: 'Passkeys' })).toBeVisible();
        await expect(page.getByText('Passwordless authentication')).toBeVisible();
        await expect(page.getByText('No passkeys registered')).toBeVisible();
        await expect(page.getByRole('button', { name: 'Add Passkey' })).toBeEnabled();
    });

    test('changes the password', async ({ page, request }) => {
        const user = await createUser(request);
        await openSettings(page, user);
        const newPassword = `Changed-${randomUUID().slice(0, 8)}!`;

        await expect(page.getByRole('heading', { level: 2, name: 'Security' })).toBeVisible();
        await page.getByRole('button', { name: 'Change Password' }).click();
        await page.getByLabel('Current password').fill(user.password);
        await page.getByLabel('New password', { exact: true }).fill(newPassword);
        await page.getByLabel('Confirm new password').fill(newPassword);
        const changed = page.waitForResponse((r) => r.url() === `${API_URL}/auth/change-password`);
        await page.getByRole('button', { name: 'Update Password' }).click();
        expect((await changed).status()).toBe(200);

        await expect(page.getByText('Password changed successfully')).toBeVisible();
        await expect(page.getByRole('button', { name: 'Change Password' })).toBeVisible();

        expect((await login(request, user.email, newPassword)).status()).toBe(200);
        expect((await login(request, user.email, user.password)).status()).toBe(401);

        // The change revoked the old session; the page swapped in the new one.
        const stored = await page.evaluate(() => localStorage.getItem('token'));
        expect(stored).not.toBe(user.token);
        expect((await api(request, 'get', '/auth/me', stored!)).status()).toBe(200);
        expect((await api(request, 'get', '/auth/me', user.token)).status()).toBe(401);
    });

    test('keeps the password when the current one is wrong', async ({ page, request, account }) => {
        await openSettings(page, account);

        await page.getByRole('button', { name: 'Change Password' }).click();
        await page.getByLabel('Current password').fill('not-my-password');
        await page.getByLabel('New password', { exact: true }).fill('Another-Password-789!');
        await page.getByLabel('Confirm new password').fill('Another-Password-789!');
        const changed = page.waitForResponse((r) => r.url() === `${API_URL}/auth/change-password`);
        await page.getByRole('button', { name: 'Update Password' }).click();
        expect((await changed).status()).toBe(400);

        await expect(page.getByRole('alert')).toContainText('Current password is incorrect');
        await expect(page).toHaveURL('/settings');
        expect((await login(request, account.email, account.password)).status()).toBe(200);
    });

    test('checks that the new passwords match before sending them', async ({ page, account }) => {
        await openSettings(page, account);
        const sent: string[] = [];
        page.on('request', (r) => {
            if (r.url() === `${API_URL}/auth/change-password`) sent.push(r.url());
        });

        await page.getByRole('button', { name: 'Change Password' }).click();
        await page.getByLabel('Current password').fill(account.password);
        await page.getByLabel('New password', { exact: true }).fill('First-Choice-123!');
        await page.getByLabel('Confirm new password').fill('Second-Choice-123!');
        await page.getByRole('button', { name: 'Update Password' }).click();

        await expect(page.getByRole('alert')).toContainText('Passwords do not match');
        expect(sent).toEqual([]);
    });

    test('exports the links as CSV', async ({ page, request }) => {
        const user = await createUser(request);
        const link = await createLink(request, user.token, {
            original_url: `https://www.iana.org/help/example-domains?export=${randomUUID()}`,
        });
        await openSettings(page, user);

        await expect(page.getByRole('heading', { level: 2, name: 'Data' })).toBeVisible();
        const download = page.waitForEvent('download');
        await page.getByRole('button', { name: 'Export All Links (CSV)' }).click();
        const file = await download;

        expect(file.suggestedFilename()).toBe('opn_onl_links.csv');
        const csv = await readFile(await file.path(), 'utf8');
        const [header, ...rows] = csv.trim().split('\n');
        expect(header.startsWith('ID,Code,Original URL,Short URL,Click Count')).toBe(true);
        expect(rows).toHaveLength(1);
        expect(rows[0]).toContain(`"${link.code}"`);
        expect(rows[0]).toContain(`"${link.original_url}"`);
    });

    test('creates an API key, shows it once, and revokes it', async ({ page, request }) => {
        const user = await createUser(request);
        await openSettings(page, user);

        await expect(page.getByRole('heading', { level: 2, name: 'API Keys' })).toBeVisible();
        await page.getByPlaceholder('Key name (e.g. MCP on my laptop)').fill('e2e laptop');
        const created = page.waitForResponse(
            (r) => r.url() === `${API_URL}/auth/api-keys` && r.request().method() === 'POST',
        );
        await page.getByRole('button', { name: 'Create key' }).click();
        const response = await created;
        expect(response.status()).toBe(201);
        const { key, key_prefix } = await response.json();

        await expect(page.getByText("Your new API key — copy it now, it won't be shown again:")).toBeVisible();
        await expect(page.getByText(key, { exact: true })).toBeVisible();
        await expect(page.getByText(`${key_prefix}… · never used`)).toBeVisible();
        expect((await api(request, 'get', '/links', key)).status()).toBe(200);

        let confirmation = '';
        page.once('dialog', (dialog) => {
            confirmation = dialog.message();
            void dialog.accept();
        });
        const revoked = page.waitForResponse(
            (r) => r.url().startsWith(`${API_URL}/auth/api-keys/`) && r.request().method() === 'DELETE',
        );
        await page.getByRole('button', { name: 'Revoke e2e laptop' }).click();
        expect((await revoked).status()).toBe(200);

        expect(confirmation).toContain('revoke this API key');
        await expect(page.getByText('API key revoked')).toBeVisible();
        await expect(page.getByRole('button', { name: 'Revoke e2e laptop' })).toHaveCount(0);
        expect((await api(request, 'get', '/links', key)).status()).toBe(401);
    });

    test('does not offer account deletion when the instance disables it', async ({ page, request, account }) => {
        const settings = await (await api(request, 'get', '/auth/settings')).json();
        expect(
            settings.account_deletion_enabled,
            'the e2e backend runs without ENABLE_ACCOUNT_DELETION',
        ).toBe(false);

        await openSettings(page, account);

        // Rendered from the same /auth/settings response as the danger zone.
        await expect(page.getByRole('heading', { level: 2, name: 'API Keys' })).toBeVisible();
        await expect(page.getByRole('heading', { name: 'Danger Zone' })).toHaveCount(0);
        await expect(page.getByRole('button', { name: /delete account/i })).toHaveCount(0);
    });

    test('keeps working when the user opens it a few times in a minute', async ({ page, account }) => {
        await openSettings(page, account);

        for (let visit = 2; visit <= 4; visit++) {
            // A person reopening the page, not a burst. The separate 10-per-second
            // gate exists to refuse bursts, and a dev-mode load already makes eight
            // /auth reads (Strict Mode runs the load effect twice). Four paced
            // visits are still 32 reads in well under a minute, which the old
            // 10-per-minute sign-in bucket refused.
            await page.waitForTimeout(1_100);
            await page.reload();
            await expect(page.getByText(account.email, { exact: true })).toBeVisible();
            await expect(page.getByRole('heading', { level: 2, name: 'Passkeys' })).toBeVisible();
        }
    });
});
