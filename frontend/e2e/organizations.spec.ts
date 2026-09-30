import { randomUUID } from 'node:crypto';
import { test, expect, type APIRequestContext, type Locator, type Page } from '@playwright/test';
import { adminToken, api, clientIpHeader, createLink, createUser, signIn, type TestUser } from './support/api';

/**
 * Organizations as the product shows them.
 *
 * The SPA has no organization management screens: organizations, members
 * (owner / admin / editor / viewer) and ownership transfers are driven through
 * the API (/orgs/*). Where organizations do surface in the browser is the admin
 * panel: the Organizations tab, the "org" marker on links, the Orgs column on
 * users, and the refusal to delete a user who still owns an organization with
 * other members. These tests build the organizations through the real API with
 * real roles and check what the admin panel makes of them.
 */

// The backend trusts CF-Connecting-IP in e2e (TRUST_PROXY_HEADERS=true). Give
// every page its own client address so this file's browser traffic does not
// spend 127.0.0.1's per-IP buckets that the rest of the suite shares.
test.beforeEach(async ({ page }) => {
    await page.setExtraHTTPHeaders(clientIpHeader());
});

interface Org {
    id: number;
    name: string;
    slug: string;
    owner_id: number;
}

async function createOrg(request: APIRequestContext, owner: TestUser): Promise<Org> {
    const suffix = randomUUID().slice(0, 8);
    const res = await api(request, 'post', '/orgs', owner.token, { name: `Acme ${suffix}`, slug: `acme-${suffix}` });
    expect(res.status(), await res.text()).toBe(201);
    return res.json();
}

async function addMember(
    request: APIRequestContext,
    org: Org,
    by: TestUser,
    member: TestUser,
    role: 'admin' | 'editor' | 'viewer',
): Promise<void> {
    const res = await api(request, 'post', `/orgs/${org.id}/members`, by.token, { email: member.email, role });
    expect(res.status(), await res.text()).toBe(201);
    expect((await res.json()).role).toBe(role);
}

type AdminTab = 'Users' | 'Links' | 'Organizations';

async function switchAdminTab(page: Page, tab: AdminTab): Promise<void> {
    await page.getByRole('button', { name: tab, exact: true }).click();
}

/** Open the admin panel as the suite's admin (see global-setup) on `tab`. */
async function openAdminTab(page: Page, tab: AdminTab): Promise<void> {
    await signIn(page, { token: adminToken(), isAdmin: true });
    await page.goto('/admin');
    await expect(page.getByRole('heading', { name: 'Admin Dashboard' })).toBeVisible({ timeout: 15_000 });
    await switchAdminTab(page, tab);
}

/** The cell of `row` under the column headed `column` (headers are shown upper-cased by CSS). */
async function cell(page: Page, row: Locator, column: string): Promise<Locator> {
    await expect(row).toHaveCount(1);
    const headers = await page.getByRole('columnheader').allTextContents();
    const index = headers.findIndex((h) => h.trim().toLowerCase() === column.toLowerCase());
    expect(index, `column "${column}" in ${JSON.stringify(headers)}`).toBeGreaterThanOrEqual(0);
    return row.getByRole('cell').nth(index);
}

test.describe('Organizations in the admin panel', () => {
    test('an organization is listed with its owner, members and links until it is deleted', async ({ page, request }) => {
        const [owner, orgAdmin, editor, viewer] = await Promise.all([
            createUser(request),
            createUser(request),
            createUser(request),
            createUser(request),
        ]);
        const org = await createOrg(request, owner);
        await addMember(request, org, owner, orgAdmin, 'admin');
        await addMember(request, org, orgAdmin, editor, 'editor'); // org admins can invite too
        await addMember(request, org, owner, viewer, 'viewer');
        // Editors may create organization links.
        const orgLink = await createLink(request, editor.token, {
            original_url: 'https://www.iana.org/domains',
            org_id: org.id,
        });
        expect(orgLink.org_id).toBe(org.id);

        await openAdminTab(page, 'Organizations');
        await page.getByPlaceholder('Search name or slug…').fill(org.slug);
        const row = page.getByRole('row').filter({ hasText: org.slug });
        await expect(row).toHaveCount(1);
        await expect(page.getByText('1 organizations', { exact: true })).toBeVisible();
        const name = await cell(page, row, 'Name');
        await expect(name.getByText(org.name, { exact: true })).toBeVisible();
        await expect(name.getByText(org.slug, { exact: true })).toBeVisible();
        await expect(await cell(page, row, 'Owner')).toHaveText(owner.email);
        await expect(await cell(page, row, 'Members')).toHaveText('4');
        await expect(await cell(page, row, 'Links')).toHaveText('1');

        // In the Links tab the link is listed under its creator and marked as an organization link.
        await switchAdminTab(page, 'Links');
        await page.getByPlaceholder('Search code, URL, title, or owner email…').fill(orgLink.code);
        const linkRow = page.getByRole('row').filter({ hasText: `/${orgLink.code}` });
        await expect(linkRow).toHaveCount(1);
        const linkOwner = await cell(page, linkRow, 'Owner');
        await expect(linkOwner).toContainText(editor.email);
        await expect(linkOwner.getByText('org', { exact: true })).toBeVisible();

        const deleted = await api(request, 'delete', `/orgs/${org.id}`, owner.token);
        expect(deleted.status()).toBe(204);
        await page.reload();
        await expect(page.getByRole('heading', { name: 'Admin Dashboard' })).toBeVisible({ timeout: 15_000 });
        await switchAdminTab(page, 'Organizations');
        await page.getByPlaceholder('Search name or slug…').fill(org.slug);
        await expect(page.getByText('No organizations found.')).toBeVisible();
        await expect(page.getByText('0 organizations', { exact: true })).toBeVisible();
    });

    test('an owner cannot be deleted while the organization has other members, until ownership is transferred', async ({ page, request }) => {
        const [owner, member] = await Promise.all([createUser(request), createUser(request)]);
        const org = await createOrg(request, owner);
        await addMember(request, org, owner, member, 'admin');
        page.on('dialog', (dialog) => void dialog.accept()); // the panel confirms deletes

        await openAdminTab(page, 'Users');
        await page.getByPlaceholder('Search email, name, or bio username…').fill(owner.email);
        const ownerRow = page.getByRole('row').filter({ hasText: owner.email });
        await expect(ownerRow).toHaveCount(1);
        await expect(await cell(page, ownerRow, 'Orgs')).toHaveText('1');

        await ownerRow.getByRole('button', { name: 'Delete', exact: true }).click();
        await expect(
            page.getByText(
                `User ${owner.userId} still owns organizations with other members: ${org.slug}. ` +
                    'Transfer ownership or delete them first.',
            ),
        ).toBeVisible();
        await expect(ownerRow.getByText('Deleted', { exact: true })).toHaveCount(0);

        // The owner hands the organization to the member; the old owner stays on as an admin.
        const transfer = await api(request, 'post', `/orgs/${org.id}/transfer-ownership`, owner.token, {
            new_owner_user_id: member.userId,
        });
        expect(transfer.status(), await transfer.text()).toBe(200);
        expect((await transfer.json()).owner_id).toBe(member.userId);
        const members = await (await api(request, 'get', `/orgs/${org.id}/members`, member.token)).json();
        expect(members.map((m: { user_id: number; role: string }) => [m.user_id, m.role]).sort()).toEqual(
            [
                [owner.userId, 'admin'],
                [member.userId, 'owner'],
            ].sort(),
        );

        await switchAdminTab(page, 'Organizations');
        await page.getByPlaceholder('Search name or slug…').fill(org.slug);
        const orgRow = page.getByRole('row').filter({ hasText: org.slug });
        await expect(await cell(page, orgRow, 'Owner')).toHaveText(member.email);
        await expect(await cell(page, orgRow, 'Members')).toHaveText('2');

        await switchAdminTab(page, 'Users');
        await expect(ownerRow).toHaveCount(1);
        await expect(await cell(page, ownerRow, 'Orgs')).toHaveText('0');
        await ownerRow.getByRole('button', { name: 'Delete', exact: true }).click();
        await expect(page.getByText(`User ${owner.userId} soft deleted`)).toBeVisible();
        await expect(ownerRow.getByText('Deleted', { exact: true })).toBeVisible();
    });
});
