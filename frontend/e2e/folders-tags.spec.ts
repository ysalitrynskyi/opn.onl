import { test, expect, type APIRequestContext, type Locator, type Page } from '@playwright/test';
import { WEB_URL, api, clientIpHeader, createLink, createUser, signIn, type TestUser } from './support/api';

/**
 * Tags, notes and folders as the dashboard sees them.
 *
 * The dashboard has no folder or tag management of its own (no folder list,
 * no tag picker, no bulk selection): folders and tags are created through the
 * API, as the MCP server and API clients do. What the dashboard does with them
 * is show a link's tags on its row, match tags and notes in the search box,
 * and keep listing a link whatever folder it is filed in.
 */

// The backend trusts CF-Connecting-IP in e2e (TRUST_PROXY_HEADERS=true). Give
// every page its own client address so this file's browser traffic does not
// spend 127.0.0.1's per-IP buckets (10/s, 100/min, and 10/min for /auth/*,
// which the dashboard hits on every load for /auth/settings) that the rest of
// the suite and the rate-limit specs share.
test.beforeEach(async ({ page }) => {
    await page.setExtraHTTPHeaders(clientIpHeader());
});

const SHORT_HOST = new URL(WEB_URL).host;

/** The dashboard row of the link with this short code. */
function linkRow(page: Page, code: string): Locator {
    const shortLink = page.getByRole('link', { name: `${SHORT_HOST}/${code}`, exact: true });
    return page.locator('div.group').filter({ has: shortLink });
}

/** One per rendered link row. */
function rows(page: Page): Locator {
    return page.getByRole('button', { name: 'Edit link' });
}

function searchBox(page: Page): Locator {
    return page.getByPlaceholder('Search links, notes, tags...');
}

async function openDashboard(page: Page, user: TestUser): Promise<void> {
    await signIn(page, user);
    await page.goto('/dashboard');
    await expect(page.getByRole('heading', { name: 'Dashboard', level: 1 })).toBeVisible({ timeout: 15_000 });
}

async function createTag(
    request: APIRequestContext,
    user: TestUser,
    data: { name: string; color?: string },
): Promise<{ id: number; name: string }> {
    const res = await api(request, 'post', '/tags', user.token, data);
    expect(res.status(), await res.text()).toBe(201);
    return res.json();
}

test.describe('Tags on the dashboard', () => {
    test('tags attached to a link show on its row', async ({ page, request }) => {
        const user = await createUser(request);
        const [launch, report] = await Promise.all([
            createTag(request, user, { name: 'launch', color: '#e11d48' }),
            createTag(request, user, { name: 'q3-report', color: '#16a34a' }),
        ]);
        // One tag at creation, one attached afterwards.
        const tagged = await createLink(request, user.token, {
            original_url: 'https://www.iana.org/domains',
            tag_ids: [launch.id],
        });
        const attach = await api(request, 'post', `/links/${tagged.id}/tags`, user.token, { tag_ids: [report.id] });
        expect(await attach.json()).toEqual({ added: 1 });
        const plain = await createLink(request, user.token, { original_url: 'https://www.iana.org/numbers' });

        await openDashboard(page, user);

        const row = linkRow(page, tagged.code);
        const launchChip = row.getByText('launch', { exact: true });
        await expect(launchChip).toBeVisible();
        await expect(row.getByText('q3-report', { exact: true })).toBeVisible();
        // The dot next to the name carries the tag colour (#e11d48).
        await expect(launchChip.locator('[aria-hidden="true"]')).toHaveCSS('background-color', 'rgb(225, 29, 72)');
        await expect(linkRow(page, plain.code)).toBeVisible();
        await expect(linkRow(page, plain.code).getByText(/^(launch|q3-report)$/)).toHaveCount(0);
    });

    test('search narrows the list to links carrying a tag', async ({ page, request }) => {
        const user = await createUser(request);
        const [newsletter, social] = await Promise.all([
            createTag(request, user, { name: 'newsletter' }),
            createTag(request, user, { name: 'social' }),
        ]);
        const inNewsletter = await createLink(request, user.token, {
            original_url: 'https://www.iana.org/domains',
            tag_ids: [newsletter.id],
        });
        const inSocial = await createLink(request, user.token, {
            original_url: 'https://www.iana.org/numbers',
            tag_ids: [social.id],
        });
        await createLink(request, user.token, { original_url: 'https://www.iana.org/protocols' });

        await openDashboard(page, user);
        await expect(rows(page)).toHaveCount(3);

        await searchBox(page).fill('newsletter');
        await expect(rows(page)).toHaveCount(1);
        await expect(linkRow(page, inNewsletter.code)).toBeVisible();

        // Matching ignores case.
        await searchBox(page).fill('SOCIAL');
        await expect(rows(page)).toHaveCount(1);
        await expect(linkRow(page, inSocial.code)).toBeVisible();

        await searchBox(page).fill('no-such-tag');
        await expect(rows(page)).toHaveCount(0);
        await expect(page.getByText('No links found matching "no-such-tag"')).toBeVisible();

        await searchBox(page).fill('');
        await expect(rows(page)).toHaveCount(3);
    });
});

test.describe('Notes on the dashboard', () => {
    test('search finds links by the words in their notes', async ({ page, request }) => {
        const user = await createUser(request);
        const forPartners = await createLink(request, user.token, {
            original_url: 'https://www.iana.org/domains',
            notes: 'Send to partners before the Q3 review',
        });
        const internal = await createLink(request, user.token, {
            original_url: 'https://www.iana.org/numbers',
            notes: 'Internal only, do not share',
        });

        await openDashboard(page, user);
        await expect(rows(page)).toHaveCount(2);

        await searchBox(page).fill('partners');
        await expect(rows(page)).toHaveCount(1);
        await expect(linkRow(page, forPartners.code)).toBeVisible();

        await searchBox(page).fill('do not share');
        await expect(rows(page)).toHaveCount(1);
        await expect(linkRow(page, internal.code)).toBeVisible();
    });
});

test.describe('Folders and the dashboard', () => {
    test('a link filed in a folder stays on the dashboard, and outlives the folder', async ({ page, request }) => {
        const user = await createUser(request);
        const created = await api(request, 'post', '/folders', user.token, { name: 'Campaigns', color: '#2563eb' });
        expect(created.status(), await created.text()).toBe(201);
        const folder = await created.json();
        const filed = await createLink(request, user.token, {
            original_url: 'https://www.iana.org/domains',
            folder_id: folder.id,
        });
        expect(filed.folder_id).toBe(folder.id);
        const loose = await createLink(request, user.token, { original_url: 'https://www.iana.org/numbers' });

        // The dashboard lists every link, filed or not.
        await openDashboard(page, user);
        await expect(page.getByText('2 links', { exact: true })).toBeVisible();
        await expect(linkRow(page, filed.code)).toBeVisible();
        await expect(linkRow(page, loose.code)).toBeVisible();

        // Deleting the folder unfiles its links instead of taking them with it.
        const deleted = await api(request, 'delete', `/folders/${folder.id}`, user.token);
        expect(deleted.status()).toBe(204);
        await page.reload();
        await expect(page.getByText('2 links', { exact: true })).toBeVisible();
        await expect(linkRow(page, filed.code)).toBeVisible();
        const links: Array<{ id: number; folder_id: number | null }> = await (
            await api(request, 'get', '/links', user.token)
        ).json();
        expect(links.find((l) => l.id === filed.id)?.folder_id).toBeNull();
    });
});
