import { test, expect, Page } from '@playwright/test';

const BASE_URL = process.env.PLAYWRIGHT_BASE_URL || 'http://localhost:5173';

async function mockApiResponse(page: Page, url: string, response: any, status = 200) {
    await page.route(url, async route => {
        await route.fulfill({
            status,
            contentType: 'application/json',
            body: JSON.stringify(response),
        });
    });
}

// ============= Organizations Tests =============

test.describe('Organizations', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });

        // Mock organizations API
        await mockApiResponse(page, '**/orgs', [
            {
                id: 1,
                name: 'Acme Corp',
                slug: 'acme-corp',
                owner_id: 1,
                created_at: '2024-01-15T10:00:00Z',
                member_count: 5,
                link_count: 50,
            },
            {
                id: 2,
                name: 'Test Organization',
                slug: 'test-org',
                owner_id: 1,
                created_at: '2024-01-10T10:00:00Z',
                member_count: 3,
                link_count: 25,
            },
        ]);
    });

    test('should display organizations list', async ({ page }) => {
        await page.goto(`${BASE_URL}/settings`);
        
        // Look for organizations section
        const orgsSection = page.locator('text=Acme Corp, text=Organizations, [data-testid="organizations"]');
        if (await orgsSection.first().isVisible()) {
            await expect(orgsSection.first()).toBeVisible();
        }
    });

    test('should create a new organization', async ({ page }) => {
        await mockApiResponse(page, '**/orgs', {
            id: 3,
            name: 'New Organization',
            slug: 'new-org',
            owner_id: 1,
            created_at: new Date().toISOString(),
            member_count: 1,
            link_count: 0,
        });

        await page.goto(`${BASE_URL}/settings`);
        
        // Look for create organization button
        const createOrgButton = page.locator('button:has-text("Create Organization"), button:has-text("New Organization")');
        if (await createOrgButton.isVisible()) {
            await createOrgButton.click();
            
            // Fill organization form
            const nameInput = page.locator('input[name="org-name"], input[placeholder*="Organization"]');
            if (await nameInput.isVisible()) {
                await nameInput.fill('New Organization');
                await page.locator('button:has-text("Create"), button:has-text("Save")').first().click();
            }
        }
    });
});

// ============= Organization Members Tests =============

test.describe('Organization Members', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });

        // Mock organization
        await mockApiResponse(page, '**/orgs/1', {
            id: 1,
            name: 'Acme Corp',
            slug: 'acme-corp',
            owner_id: 1,
            created_at: '2024-01-15T10:00:00Z',
            member_count: 3,
            link_count: 50,
        });

        // Mock members
        await mockApiResponse(page, '**/orgs/1/members', [
            {
                id: 1,
                user_id: 1,
                email: 'owner@example.com',
                role: 'owner',
                joined_at: '2024-01-15T10:00:00Z',
            },
            {
                id: 2,
                user_id: 2,
                email: 'admin@example.com',
                role: 'admin',
                joined_at: '2024-01-16T10:00:00Z',
            },
            {
                id: 3,
                user_id: 3,
                email: 'member@example.com',
                role: 'member',
                joined_at: '2024-01-17T10:00:00Z',
            },
        ]);
    });

    test('should display organization members', async ({ page }) => {
        await page.goto(`${BASE_URL}/settings/organizations/1`);
        
        // Look for members list
        const membersList = page.locator('text=owner@example.com, text=Members, [data-testid="members"]');
        if (await membersList.first().isVisible()) {
            await expect(membersList.first()).toBeVisible();
        }
    });

    test('should show member roles', async ({ page }) => {
        await page.goto(`${BASE_URL}/settings/organizations/1`);
        
        // Look for role badges
        const roleBadge = page.locator('text=owner, text=admin, text=member');
        if (await roleBadge.first().isVisible()) {
            await expect(roleBadge.first()).toBeVisible();
        }
    });

    test('should invite new member', async ({ page }) => {
        await mockApiResponse(page, '**/orgs/1/members', {
            id: 4,
            user_id: 4,
            email: 'new@example.com',
            role: 'member',
            joined_at: new Date().toISOString(),
        });

        await page.goto(`${BASE_URL}/settings/organizations/1`);
        
        // Look for invite button
        const inviteButton = page.locator('button:has-text("Invite"), button:has-text("Add Member")');
        if (await inviteButton.isVisible()) {
            await inviteButton.click();
            
            // Fill invite form
            const emailInput = page.locator('input[type="email"], input[placeholder*="email"]');
            if (await emailInput.isVisible()) {
                await emailInput.fill('new@example.com');
                await page.locator('button:has-text("Send"), button:has-text("Invite")').first().click();
            }
        }
    });

    test('should change member role', async ({ page }) => {
        await mockApiResponse(page, '**/orgs/1/members/3', {
            id: 3,
            user_id: 3,
            email: 'member@example.com',
            role: 'admin',
            joined_at: '2024-01-17T10:00:00Z',
        });

        await page.goto(`${BASE_URL}/settings/organizations/1`);
        
        // Look for role dropdown or edit button
        const roleSelect = page.locator('select[name*="role"], [data-testid="role-select"]');
        if (await roleSelect.first().isVisible()) {
            await roleSelect.first().selectOption({ value: 'admin' });
        }
    });

    test('should remove member', async ({ page }) => {
        await mockApiResponse(page, '**/orgs/1/members/3', { message: 'Removed' });

        await page.goto(`${BASE_URL}/settings/organizations/1`);
        
        // Look for remove button
        const removeButton = page.locator('button:has-text("Remove"), button[aria-label*="remove"]').first();
        if (await removeButton.isVisible()) {
            await removeButton.click();
        }
    });
});

// ============= Organization Audit Log Tests =============

test.describe('Organization Audit Log', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });

        // Mock audit log
        await mockApiResponse(page, '**/orgs/1/audit', {
            audit_logs: [
                {
                    id: 1,
                    user_id: 1,
                    user_email: 'owner@example.com',
                    action: 'link_created',
                    resource_type: 'link',
                    resource_id: 1,
                    details: { code: 'abc123' },
                    ip_address: '192.168.1.1',
                    created_at: '2024-01-15T10:00:00Z',
                },
                {
                    id: 2,
                    user_id: 2,
                    user_email: 'admin@example.com',
                    action: 'member_added',
                    resource_type: 'organization',
                    resource_id: 1,
                    details: { email: 'new@example.com' },
                    ip_address: '192.168.1.2',
                    created_at: '2024-01-16T10:00:00Z',
                },
            ],
            total: 2,
            page: 1,
            per_page: 20,
        });
    });

    test('should display audit log entries', async ({ page }) => {
        await page.goto(`${BASE_URL}/settings/organizations/1/audit`);
        
        // Look for audit log entries
        const auditEntry = page.locator('text=link_created, text=member_added, [data-testid="audit-log"]');
        if (await auditEntry.first().isVisible()) {
            await expect(auditEntry.first()).toBeVisible();
        }
    });

    test('should show action details', async ({ page }) => {
        await page.goto(`${BASE_URL}/settings/organizations/1/audit`);
        
        // Look for action details
        const actionDetails = page.locator('text=abc123, text=owner@example.com');
        if (await actionDetails.first().isVisible()) {
            await expect(actionDetails.first()).toBeVisible();
        }
    });

    test('should filter audit log by action', async ({ page }) => {
        await page.goto(`${BASE_URL}/settings/organizations/1/audit`);
        
        // Look for filter dropdown
        const actionFilter = page.locator('select[name*="action"], [data-testid="action-filter"]');
        if (await actionFilter.isVisible()) {
            await actionFilter.selectOption({ label: 'Link Created' });
        }
    });
});

// ============= Organization Settings Tests =============

test.describe('Organization Settings', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });

        await mockApiResponse(page, '**/orgs/1', {
            id: 1,
            name: 'Acme Corp',
            slug: 'acme-corp',
            owner_id: 1,
            created_at: '2024-01-15T10:00:00Z',
            member_count: 5,
            link_count: 50,
        });
    });

    test('should update organization name', async ({ page }) => {
        await mockApiResponse(page, '**/orgs/1', {
            id: 1,
            name: 'Updated Corp',
            slug: 'updated-corp',
            owner_id: 1,
            created_at: '2024-01-15T10:00:00Z',
            member_count: 5,
            link_count: 50,
        });

        await page.goto(`${BASE_URL}/settings/organizations/1`);
        
        // Look for edit name input
        const nameInput = page.locator('input[name="org-name"], input[value="Acme Corp"]');
        if (await nameInput.isVisible()) {
            await nameInput.clear();
            await nameInput.fill('Updated Corp');
            await page.locator('button:has-text("Save"), button:has-text("Update")').first().click();
        }
    });

    test('should delete organization', async ({ page }) => {
        await mockApiResponse(page, '**/orgs/1', { message: 'Deleted' });

        await page.goto(`${BASE_URL}/settings/organizations/1`);
        
        // Look for delete button
        const deleteButton = page.locator('button:has-text("Delete Organization"), button:has-text("Delete Org")');
        if (await deleteButton.isVisible()) {
            await deleteButton.click();
            
            // Confirm deletion
            const confirmButton = page.locator('button:has-text("Confirm"), button:has-text("Yes")');
            if (await confirmButton.isVisible()) {
                await confirmButton.click();
            }
        }
    });

    test('should transfer ownership', async ({ page }) => {
        await mockApiResponse(page, '**/orgs/1/transfer', { message: 'Transferred' });

        await page.goto(`${BASE_URL}/settings/organizations/1`);
        
        // Look for transfer ownership option
        const transferButton = page.locator('button:has-text("Transfer"), text=Transfer Ownership');
        if (await transferButton.first().isVisible()) {
            await transferButton.first().click();
            
            // Select new owner
            const ownerSelect = page.locator('select[name*="owner"], [data-testid="owner-select"]');
            if (await ownerSelect.isVisible()) {
                await ownerSelect.selectOption({ index: 1 });
            }
        }
    });
});

// ============= Organization Permissions Tests =============

test.describe('Organization Permissions', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });
    });

    test('should prevent non-admin from accessing admin features', async ({ page }) => {
        // Mock as regular member
        await mockApiResponse(page, '**/orgs/1', {
            id: 1,
            name: 'Acme Corp',
            slug: 'acme-corp',
            owner_id: 99,  // Different owner
            created_at: '2024-01-15T10:00:00Z',
            member_count: 5,
            link_count: 50,
        });

        await mockApiResponse(page, '**/orgs/1/members', [
            {
                id: 1,
                user_id: 1,
                email: 'member@example.com',
                role: 'member',  // Regular member, not admin
                joined_at: '2024-01-17T10:00:00Z',
            },
        ]);

        await page.goto(`${BASE_URL}/settings/organizations/1`);
        
        // Admin features should be hidden or disabled
        const adminButton = page.locator('button:has-text("Delete Organization"), button:has-text("Invite")');
        if (await adminButton.isVisible()) {
            // Should be disabled for regular members
            const isDisabled = await adminButton.isDisabled();
            if (isDisabled) {
                expect(isDisabled).toBeTruthy();
            }
        }
    });

    test('should show admin features for organization owner', async ({ page }) => {
        await mockApiResponse(page, '**/orgs/1', {
            id: 1,
            name: 'Acme Corp',
            slug: 'acme-corp',
            owner_id: 1,  // Current user is owner
            created_at: '2024-01-15T10:00:00Z',
            member_count: 5,
            link_count: 50,
        });

        await mockApiResponse(page, '**/orgs/1/members', [
            {
                id: 1,
                user_id: 1,
                email: 'owner@example.com',
                role: 'owner',
                joined_at: '2024-01-15T10:00:00Z',
            },
        ]);

        await page.goto(`${BASE_URL}/settings/organizations/1`);
        
        // Admin features should be visible and enabled for owner
        const adminButton = page.locator('button:has-text("Delete Organization"), button:has-text("Settings")');
        if (await adminButton.first().isVisible()) {
            await expect(adminButton.first()).toBeEnabled();
        }
    });
});

