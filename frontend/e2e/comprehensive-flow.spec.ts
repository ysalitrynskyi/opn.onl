import { randomUUID } from 'node:crypto';
import { test, expect } from '@playwright/test';
import { API_URL, TEST_PASSWORD, api, uniqueEmail, verifyEmail } from './support/api';

/**
 * The suite's one end-to-end journey, against the real backend and database:
 * sign up through the form, get verified, sign in through the form, shorten a
 * link in the dashboard, follow it through the backend redirect, see the click
 * counted, delete the link. Everything a single feature spec tests in depth
 * (form validation, dashboard widgets, analytics charts) lives in that spec;
 * this file proves the pieces work together.
 */

/**
 * A client address owned by this test run. The backend trusts
 * CF-Connecting-IP in e2e (TRUST_PROXY_HEADERS=true, as behind Cloudflare in
 * production), so the browser's sign-up, sign-in and page loads spend this
 * address's rate-limit buckets instead of the 127.0.0.1 ones that every other
 * browser test shares. Random in 198.18.0.0/15, a range the shared helpers
 * (10.x) never hand out.
 */
function freshClientIp(): string {
    const b = randomUUID().replace(/-/g, '');
    const octet = (i: number) => parseInt(b.slice(i * 2, i * 2 + 2), 16);
    return `198.${18 + (octet(0) & 1)}.${octet(1)}.${octet(2) || 1}`;
}

interface CreatedLink {
    id: number;
    code: string;
    original_url: string;
}

test('a visitor signs up, is verified, signs in, shortens a link and sees its click counted', async ({
    page,
    request,
}) => {
    // Two bcrypt rounds per sign-in/sign-up on a debug backend, plus the click
    // buffer's flush interval: slower than a single-page test.
    test.slow();

    const email = uniqueEmail('journey');
    const destination = `https://www.iana.org/help/example-domains?journey=${randomUUID()}`;
    await page.setExtraHTTPHeaders({ 'CF-Connecting-IP': freshClientIp() });

    // The page's API calls are cross-origin, so each non-simple one is preceded
    // by an OPTIONS preflight to the same URL; match on the method as well.
    const apiResponse = (method: string, path: string) =>
        page.waitForResponse((r) => r.request().method() === method && r.url() === `${API_URL}${path}`);
    // The dashboard and analytics pages are lazy chunks that load their data on
    // mount. Waiting for that data (no timeout but the test's) rather than for
    // the first element keeps a slow first compile from failing an assertion.
    const linkListLoaded = () => apiResponse('GET', '/links');
    const urlInput = page.getByPlaceholder('https://example.com/long-url');
    const createButton = page.getByRole('button', { name: 'Create', exact: true });

    let userId = 0;
    await test.step('sign up with the registration form', async () => {
        await page.goto('/register');
        await page.getByLabel('Email address').fill(email);
        await page.getByLabel('Password').fill(TEST_PASSWORD);
        const registered = apiResponse('POST', '/auth/register');
        await page.getByRole('button', { name: 'Create account' }).click();

        const res = await registered;
        expect(res.status()).toBe(201);
        const body = await res.json();
        expect(body).toMatchObject({ email, email_verified: false, is_admin: false });
        userId = body.user_id;

        await expect(page.getByRole('heading', { name: 'Check your email' })).toBeVisible();
        await expect(page.getByText(email)).toBeVisible();
    });

    await test.step('an unverified account cannot shorten links yet', async () => {
        const listed = linkListLoaded();
        await page.getByRole('button', { name: 'Continue to dashboard' }).click();
        expect((await listed).status()).toBe(200);
        await expect(page).toHaveURL(/\/dashboard$/);
        await expect(page.getByText('No links yet')).toBeVisible();

        await urlInput.fill(destination);
        const refused = apiResponse('POST', '/links');
        await createButton.click();
        expect((await refused).status()).toBe(403);
        await expect(page.getByRole('alert')).toHaveText('Please verify your email address before creating links');
        await expect(urlInput).toHaveValue(destination);
    });

    await test.step('an admin verifies the address (no SMTP in e2e)', async () => {
        await verifyEmail(request, userId);
    });

    let token = '';
    await test.step('sign out, then sign back in, mistyping the password once', async () => {
        await page.getByRole('button', { name: 'Account menu' }).click();
        await page.getByRole('button', { name: 'Log out' }).click();
        await expect(page).toHaveURL(/\/login$/);

        await page.getByLabel('Email address').fill(email);
        await page.getByLabel('Password').fill(`${TEST_PASSWORD}-typo`);
        const rejected = apiResponse('POST', '/auth/login');
        await page.getByRole('button', { name: 'Sign in', exact: true }).click();
        expect((await rejected).status()).toBe(401);
        await expect(page.getByRole('alert')).toHaveText('Invalid credentials');
        await expect(page).toHaveURL(/\/login$/);

        await page.getByLabel('Password').fill(TEST_PASSWORD);
        const accepted = apiResponse('POST', '/auth/login');
        const listed = linkListLoaded();
        await page.getByLabel('Password').press('Enter');
        const res = await accepted;
        expect(res.status()).toBe(200);
        const body = await res.json();
        expect(body).toMatchObject({ user_id: userId, email, email_verified: true });
        token = body.token;
        expect((await listed).status()).toBe(200);
        await expect(page).toHaveURL(/\/dashboard$/);
    });

    let link: CreatedLink = { id: 0, code: '', original_url: '' };
    await test.step('shorten the link from the dashboard', async () => {
        await expect(page.getByText('No links yet')).toBeVisible();
        await urlInput.fill(destination);
        const created = apiResponse('POST', '/links');
        const relisted = linkListLoaded();
        await createButton.click();

        const res = await created;
        expect(res.status()).toBe(201);
        link = await res.json();
        expect(link.original_url).toBe(destination);
        expect((await relisted).status()).toBe(200);

        // The dashboard shows the short link on the frontend origin, with no clicks yet.
        const origin = new URL(page.url()).origin;
        const shortLink = page.getByRole('link', { name: `${new URL(origin).host}/${link.code}`, exact: true });
        await expect(shortLink).toHaveAttribute('href', `${origin}/${link.code}`);
        await expect(page.locator(`a[href="/analytics/${link.id}"]`)).toHaveText('0');
        await expect(urlInput).toHaveValue('');
        await expect(page.getByText('No links yet')).toBeHidden();
    });

    await test.step('the short link redirects to the destination', async () => {
        const res = await request.get(`${API_URL}/${link.code}`, {
            maxRedirects: 0,
            headers: { 'CF-Connecting-IP': freshClientIp() },
        });
        expect(res.status()).toBe(307);
        expect(res.headers()['location']).toBe(destination);
    });

    await test.step('the dashboard counts the click', async () => {
        // Clicks are buffered and written to the database every few seconds;
        // wait for the write through the API so the page is loaded only once.
        await expect
            .poll(
                async () => {
                    const res = await api(request, 'get', '/links', token);
                    const links: Array<{ id: number; click_count: number }> = await res.json();
                    return links.find((l) => l.id === link.id)?.click_count;
                },
                { message: 'click_count of the new link', timeout: 20_000 },
            )
            .toBe(1);

        const listed = linkListLoaded();
        await page.reload();
        expect((await listed).status()).toBe(200);
        await expect(page.locator(`a[href="/analytics/${link.id}"]`)).toHaveText('1');
        await expect(page.getByText(/^1 clicks?$/)).toBeVisible();
    });

    await test.step("the link's analytics page shows the click", async () => {
        const statsLoaded = page.waitForResponse(
            (r) => r.request().method() === 'GET' && r.url().startsWith(`${API_URL}/links/${link.id}/stats?`),
        );
        await page.locator(`a[href="/analytics/${link.id}"]`).click();
        expect((await statsLoaded).status()).toBe(200);
        await expect(page).toHaveURL(new RegExp(`/analytics/${link.id}$`));
        await expect(page.getByRole('heading', { level: 1 })).toContainText(link.code);
        // StatCard: title span inside a header row, value in the card below it.
        const totalClicks = page.getByText('Total Clicks', { exact: true }).locator('xpath=../..');
        await expect(totalClicks).toHaveText(/^Total Clicks\s*1$/);
    });

    await test.step('deleting the link retires the short URL', async () => {
        const listed = linkListLoaded();
        await page.goBack();
        expect((await listed).status()).toBe(200);
        await expect(page).toHaveURL(/\/dashboard$/);

        const dialogs: string[] = [];
        page.once('dialog', (dialog) => {
            dialogs.push(dialog.message());
            void dialog.accept();
        });
        const deleted = apiResponse('DELETE', `/links/${link.id}`);
        await page.getByRole('button', { name: 'Delete link' }).click();
        expect((await deleted).status()).toBe(200);
        expect(dialogs).toEqual(['Are you sure you want to delete this link?']);
        await expect(page.getByText('No links yet')).toBeVisible();

        const res = await request.get(`${API_URL}/${link.code}`, {
            maxRedirects: 0,
            headers: { 'CF-Connecting-IP': freshClientIp() },
        });
        expect(res.status()).toBe(404);
    });
});
