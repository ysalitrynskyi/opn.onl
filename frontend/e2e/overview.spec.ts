import { test, expect, type Page } from '@playwright/test';
import { API_URL, api, clientIpHeader, createLink, createUser, signIn } from './support/api';

/**
 * The account overview (/overview) against the real backend.
 *
 * One real click: a GET of the short link. The page then shows the totals
 * from GET /analytics/dashboard once the click buffer has flushed.
 */

const CHROME_ON_WINDOWS =
    'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36';

/** The number on a metric card. The title's row is the card's first line; the figure follows it. */
function metric(page: Page, title: string) {
    return page.getByText(title, { exact: true }).locator('xpath=../following-sibling::div[1]');
}

test.beforeEach(async ({ page }) => {
    await page.setExtraHTTPHeaders(clientIpHeader());
});

test('shows totals for an account with one link and one click', async ({ page, request }) => {
    const user = await createUser(request);
    const link: { id: number; code: string } = await createLink(request, user.token);
    const redirect = await request.get(`${API_URL}/${link.code}`, {
        maxRedirects: 0,
        headers: {
            'User-Agent': CHROME_ON_WINDOWS,
            ...clientIpHeader(),
        },
    });
    expect(redirect.status(), `redirect /${link.code}`).toBe(307);

    await expect.poll(async () => {
        const res = await api(request, 'get', '/analytics/dashboard', user.token);
        if (!res.ok()) return `HTTP ${res.status()}`;
        const body = await res.json();
        return body.total_clicks;
    }, {
        message: 'dashboard total_clicks should reach 1',
        timeout: 20_000,
    }).toBe(1);

    await signIn(page, user);
    const answered = page.waitForResponse(
        (res) => new URL(res.url()).pathname === '/analytics/dashboard' && res.request().method() === 'GET',
    );
    await page.goto('/overview');
    expect((await answered).status()).toBe(200);

    await expect(page.getByRole('heading', { level: 1, name: 'Overview' })).toBeVisible();
    await expect(page).toHaveTitle('Overview | opn.onl');
    await expect(metric(page, 'Total Links')).toHaveText('1');
    await expect(metric(page, 'Active Links')).toHaveText('1');
    await expect(metric(page, 'Total Clicks')).toHaveText('1');
    await expect(metric(page, 'Today')).toHaveText('1');
    await expect(metric(page, 'Last 7 Days')).toHaveText('1');
    await expect(metric(page, 'Last 30 Days')).toHaveText('1');
    await expect(page.getByRole('link', { name: `${link.code}, 1 click` })).toHaveAttribute('href', `/analytics/${link.id}`);
    await expect(page.getByRole('heading', { name: 'Clicks over time' })).toBeVisible();

    await page.getByRole('button', { name: 'Account menu' }).click();
    await expect(page.getByRole('link', { name: 'Overview', exact: true })).toBeVisible();
    await expect(page.getByRole('link', { name: 'Overview', exact: true })).toHaveAttribute('href', '/overview');
});
