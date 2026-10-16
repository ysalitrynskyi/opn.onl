import { randomUUID } from 'node:crypto';
import { test, expect, type APIRequestContext, type Locator, type Page } from '@playwright/test';
import { adminToken, api, clientIpHeader, createLink, createUser, signIn, type TestUser } from './support/api';

/**
 * Organizations as the product shows them.
 *
 * The Teams page (/organizations) creates a team and manages its members.
 * The admin panel lists every organization on the instance, marks organization
 * links, and refuses to delete a user who still owns an organization with
 * other members. Both use real accounts. Admin-panel cases build the
 * organizations through the API; the Teams cases drive the page itself.
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
        await expect(page.getByText('1 organization', { exact: true })).toBeVisible();
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

const LAZY_PAGE = { timeout: 20_000 };

test.describe('Teams page', () => {
    test('creates a team, invites a member, changes their role and removes them', async ({ page, request }) => {
        const [owner, member] = await Promise.all([createUser(request), createUser(request)]);
        await signIn(page, owner);
        await page.goto('/dashboard');
        await expect(page.getByRole('heading', { name: 'Create new link' })).toBeVisible(LAZY_PAGE);
        await page.getByRole('button', { name: 'Account menu' }).click();
        await page.getByRole('link', { name: 'Teams' }).click();
        await expect(page).toHaveURL('/organizations');
        await expect(page.getByRole('heading', { level: 1, name: 'Teams' })).toBeVisible();

        const suffix = randomUUID().slice(0, 8);
        const name = `Acme ${suffix}`;
        const slug = `acme-${suffix}`;
        await page.getByLabel('Team name').fill(name);
        await page.getByLabel('Team slug').fill(slug);
        const created = page.waitForResponse(
            (res) => res.url().endsWith('/orgs') && res.request().method() === 'POST',
        );
        await page.getByRole('button', { name: 'Create team' }).click();
        const createdResponse = await created;
        expect(createdResponse.status()).toBe(201);
        const org = await createdResponse.json() as Org;
        await expect(page.getByRole('heading', { name, exact: true })).toBeVisible();

        await page.getByLabel('Member email').fill(member.email);
        await page.getByLabel('Member role').selectOption('editor');
        const invited = page.waitForResponse(
            (res) => res.url().includes(`/orgs/${org.id}/members`) && res.request().method() === 'POST',
        );
        await page.getByRole('button', { name: 'Add member' }).click();
        expect((await invited).status()).toBe(201);
        const members = page.getByRole('list', { name: 'Members' });
        await expect(members.getByRole('listitem').filter({ hasText: member.email })).toBeVisible();
        await expect(page.getByLabel(`Role for ${member.email}`)).toHaveValue('editor');

        const updated = page.waitForResponse(
            (res) => res.url().includes(`/orgs/${org.id}/members/`) && res.request().method() === 'PUT',
        );
        await page.getByLabel(`Role for ${member.email}`).selectOption('viewer');
        expect((await updated).status()).toBe(200);
        await expect(page.getByLabel(`Role for ${member.email}`)).toHaveValue('viewer');

        await page.getByRole('button', { name: `Remove ${member.email}` }).click();
        const removed = page.waitForResponse(
            (res) => res.url().includes(`/orgs/${org.id}/members/`) && res.request().method() === 'DELETE',
        );
        await page.getByRole('dialog', { name: 'Remove member' }).getByRole('button', { name: 'Remove member' }).click();
        expect((await removed).status()).toBe(204);
        await expect(members.getByRole('listitem').filter({ hasText: member.email })).toHaveCount(0);
        await expect(page.getByRole('link', { name: 'View links' })).toHaveAttribute('href', `/dashboard?org=${org.id}`);
    });

    test('a viewer can open the team but cannot manage it', async ({ page, request }) => {
        const [owner, viewer] = await Promise.all([createUser(request), createUser(request)]);
        const org = await createOrg(request, owner);
        await addMember(request, org, owner, viewer, 'viewer');

        await signIn(page, viewer);
        await page.goto('/organizations');
        await expect(page.getByRole('heading', { level: 1, name: 'Teams' })).toBeVisible(LAZY_PAGE);
        await page.getByRole('button', { name: org.name }).click();
        await expect(page.getByRole('heading', { name: org.name, exact: true })).toBeVisible();
        await expect(page.getByText(owner.email)).toBeVisible();
        await expect(page.getByRole('link', { name: 'View links' })).toHaveAttribute('href', `/dashboard?org=${org.id}`);
        await expect(page.getByRole('button', { name: 'Leave team' })).toBeVisible();
        await expect(page.getByRole('button', { name: 'Add member' })).toHaveCount(0);
        await expect(page.getByRole('button', { name: 'Delete team' })).toHaveCount(0);
        await expect(page.getByRole('button', { name: 'Transfer ownership' })).toHaveCount(0);
        await expect(page.getByRole('heading', { name: 'Activity' })).toHaveCount(0);
    });

    test("a viewer sees the team's links without the controls the API refuses them", async ({ page, request }) => {
        const [owner, viewer] = await Promise.all([createUser(request), createUser(request)]);
        const org = await createOrg(request, owner);
        await addMember(request, org, owner, viewer, 'viewer');
        const link = await createLink(request, owner.token, { org_id: org.id, title: `Team page ${randomUUID().slice(0, 8)}` });

        await signIn(page, viewer);
        await page.goto(`/dashboard?org=${org.id}`);
        await expect(page.getByText(link.title)).toBeVisible(LAZY_PAGE);
        await expect(page.getByText(`You can view the links in ${org.name}. Editors and admins can add and change them.`)).toBeVisible();

        await expect(page.getByPlaceholder('https://example.com/long-url')).toHaveCount(0);
        await expect(page.getByRole('button', { name: 'Bulk Import' })).toHaveCount(0);
        await expect(page.getByRole('button', { name: 'Manage' })).toHaveCount(0);
        await expect(page.getByRole('checkbox', { name: `Select link ${link.code}` })).toHaveCount(0);
        await expect(page.getByRole('button', { name: `Edit link ${link.code}` })).toHaveCount(0);
        await expect(page.getByRole('button', { name: `Delete link ${link.code}` })).toHaveCount(0);
        await expect(page.getByRole('button', { name: `Pin link ${link.code}` })).toHaveCount(0);
        // What a viewer may do is still there.
        await expect(page.getByRole('button', { name: `Copy short URL for ${link.code}` })).toBeVisible();
        await expect(page.getByRole('button', { name: 'Export CSV' })).toBeVisible();
    });
});
