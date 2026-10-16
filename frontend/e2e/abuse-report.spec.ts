import { randomUUID } from 'node:crypto';
import { test, expect } from '@playwright/test';
import { API_URL, adminToken, clientIpHeader, createLink, createUser, signIn } from './support/api';

/**
 * A visitor reports a live short link from its preview page. The suite admin
 * blocks that destination from the reports queue, and the short link stops
 * redirecting.
 *
 * The blocked destination is a unique URL. An exact-URL block with a query
 * matches only that query, so this does not take down the shared iana.org
 * pages other specs shorten.
 */

test.beforeEach(async ({ page }) => {
    await page.setExtraHTTPHeaders(clientIpHeader());
});

test('a visitor reports a link and an admin blocks it', async ({ page, request }) => {
    const owner = await createUser(request);
    const destination = `https://www.iana.org/help/example-domains?abuse=${randomUUID()}`;
    const link = await createLink(request, owner.token, { original_url: destination });

    const before = await request.get(`${API_URL}/${link.code}`, {
        maxRedirects: 0,
        headers: clientIpHeader(),
    });
    expect(before.status()).toBe(307);

    await page.goto(`/${link.code}+`);
    await expect(page.getByRole('heading', { name: 'Link Preview' })).toBeVisible({ timeout: 15_000 });

    await page.getByRole('button', { name: 'Report this link' }).click();
    await page.getByRole('radio', { name: 'Phishing' }).check();
    await page.getByLabel('Details').fill('e2e abuse report');
    await page.getByRole('button', { name: 'Submit report' }).click();
    await expect(page.getByText('Thanks, an administrator will review this link')).toBeVisible();

    page.on('dialog', (dialog) => dialog.accept());
    await signIn(page, { token: adminToken(), isAdmin: true });
    await page.goto('/admin');
    await expect(page.getByRole('heading', { name: 'Admin Dashboard' })).toBeVisible({ timeout: 15_000 });
    await page.getByRole('button', { name: /^Reports/ }).click();

    const row = page.getByRole('row', { name: new RegExp(link.code) });
    await expect(row).toBeVisible();
    await row.getByRole('button', { name: `Block link and resolve ${link.code}` }).click();
    await expect(page.getByText('Report resolved and the destination is blocked')).toBeVisible();

    const after = await request.get(`${API_URL}/${link.code}`, {
        maxRedirects: 0,
        headers: clientIpHeader(),
    });
    expect(after.status()).toBe(410);
});
