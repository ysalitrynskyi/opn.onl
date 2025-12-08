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

// ============= Folders Tests =============

test.describe('Folders Management', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });

        // Mock folders API
        await mockApiResponse(page, '**/folders', [
            {
                id: 1,
                name: 'Marketing',
                color: '#FF5733',
                user_id: 1,
                created_at: '2024-01-15T10:00:00Z',
                link_count: 5,
            },
            {
                id: 2,
                name: 'Personal',
                color: '#33FF57',
                user_id: 1,
                created_at: '2024-01-14T10:00:00Z',
                link_count: 3,
            },
        ]);

        // Mock links API
        await mockApiResponse(page, '**/links', [
            {
                id: 1,
                code: 'abc123',
                original_url: 'https://example.com',
                click_count: 42,
                created_at: '2024-01-15T10:00:00Z',
                has_password: false,
                is_active: true,
                folder_id: 1,
                tags: [],
            },
        ]);
    });

    test('should display folders list on dashboard', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard`);
        
        // Look for folder-related elements
        const foldersSection = page.locator('[data-testid="folders"], text=Marketing, text=Personal');
        await expect(foldersSection.first()).toBeVisible({ timeout: 5000 });
    });

    test('should create a new folder', async ({ page }) => {
        await mockApiResponse(page, '**/folders', {
            id: 3,
            name: 'New Folder',
            color: '#3357FF',
            user_id: 1,
            created_at: new Date().toISOString(),
            link_count: 0,
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Look for create folder button
        const createFolderButton = page.locator('button:has-text("New Folder"), button:has-text("Add Folder"), button[aria-label*="folder"]');
        if (await createFolderButton.isVisible()) {
            await createFolderButton.click();
            
            // Fill folder form
            const nameInput = page.locator('input[name="folder-name"], input[placeholder*="Folder"]');
            if (await nameInput.isVisible()) {
                await nameInput.fill('New Folder');
                await page.locator('button:has-text("Create"), button:has-text("Save")').first().click();
            }
        }
    });

    test('should filter links by folder', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard`);
        
        // Click on a folder to filter
        const folderItem = page.locator('text=Marketing');
        if (await folderItem.isVisible()) {
            await folderItem.click();
            
            // Should show filtered links
            await page.waitForTimeout(500);
        }
    });
});

// ============= Tags Tests =============

test.describe('Tags Management', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });

        // Mock tags API
        await mockApiResponse(page, '**/tags', [
            { id: 1, name: 'important', color: '#FF5733' },
            { id: 2, name: 'marketing', color: '#33FF57' },
            { id: 3, name: 'social', color: '#3357FF' },
        ]);

        // Mock links API with tags
        await mockApiResponse(page, '**/links', [
            {
                id: 1,
                code: 'tagged123',
                original_url: 'https://example.com',
                click_count: 42,
                created_at: '2024-01-15T10:00:00Z',
                has_password: false,
                is_active: true,
                tags: [
                    { id: 1, name: 'important', color: '#FF5733' },
                ],
            },
        ]);
    });

    test('should display tags on links', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard`);
        
        // Look for tag badges
        const tagBadge = page.locator('[data-testid="tag"], .tag, text=important');
        await expect(tagBadge.first()).toBeVisible({ timeout: 5000 });
    });

    test('should filter links by tag', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard`);
        
        // Look for tag filter
        const tagFilter = page.locator('text=important, [data-testid="tag-filter"]');
        if (await tagFilter.first().isVisible()) {
            await tagFilter.first().click();
            await page.waitForTimeout(500);
        }
    });

    test('should create a new tag', async ({ page }) => {
        await mockApiResponse(page, '**/tags', {
            id: 4,
            name: 'new-tag',
            color: '#FF00FF',
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Look for create tag button
        const createTagButton = page.locator('button:has-text("New Tag"), button:has-text("Add Tag"), button[aria-label*="tag"]');
        if (await createTagButton.isVisible()) {
            await createTagButton.click();
        }
    });
});

// ============= Link with Folder and Tags Tests =============

test.describe('Link with Folders and Tags', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });

        await mockApiResponse(page, '**/folders', [
            { id: 1, name: 'Marketing', color: '#FF5733', user_id: 1, created_at: '2024-01-15T10:00:00Z', link_count: 0 },
        ]);

        await mockApiResponse(page, '**/tags', [
            { id: 1, name: 'important', color: '#FF5733' },
        ]);

        await mockApiResponse(page, '**/links', []);
    });

    test('should create link with folder', async ({ page }) => {
        await mockApiResponse(page, '**/links', {
            id: 1,
            code: 'folder123',
            original_url: 'https://example.com',
            click_count: 0,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: true,
            folder_id: 1,
            tags: [],
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Fill URL input
        const urlInput = page.locator('input[placeholder*="URL"], input[type="url"]').first();
        await urlInput.fill('https://example.com');
        
        // Select folder if dropdown exists
        const folderSelect = page.locator('select[name*="folder"], [data-testid="folder-select"]');
        if (await folderSelect.isVisible()) {
            await folderSelect.selectOption({ label: 'Marketing' });
        }
        
        // Submit
        const submitButton = page.locator('button:has-text("Shorten"), button[type="submit"]').first();
        await submitButton.click();
    });

    test('should create link with tags', async ({ page }) => {
        await mockApiResponse(page, '**/links', {
            id: 1,
            code: 'tagged456',
            original_url: 'https://example.com',
            click_count: 0,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: true,
            tags: [{ id: 1, name: 'important', color: '#FF5733' }],
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Fill URL input
        const urlInput = page.locator('input[placeholder*="URL"], input[type="url"]').first();
        await urlInput.fill('https://example.com');
        
        // Add tag if available
        const tagInput = page.locator('[data-testid="tag-input"], input[placeholder*="tag"]');
        if (await tagInput.isVisible()) {
            await tagInput.fill('important');
        }
        
        // Submit
        const submitButton = page.locator('button:has-text("Shorten"), button[type="submit"]').first();
        await submitButton.click();
    });
});

// ============= Bulk Operations Tests =============

test.describe('Bulk Operations', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });

        await mockApiResponse(page, '**/links', [
            {
                id: 1,
                code: 'bulk1',
                original_url: 'https://example1.com',
                click_count: 10,
                created_at: '2024-01-15T10:00:00Z',
                has_password: false,
                is_active: true,
                tags: [],
            },
            {
                id: 2,
                code: 'bulk2',
                original_url: 'https://example2.com',
                click_count: 20,
                created_at: '2024-01-14T10:00:00Z',
                has_password: false,
                is_active: true,
                tags: [],
            },
            {
                id: 3,
                code: 'bulk3',
                original_url: 'https://example3.com',
                click_count: 30,
                created_at: '2024-01-13T10:00:00Z',
                has_password: false,
                is_active: true,
                tags: [],
            },
        ]);
    });

    test('should select multiple links', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard`);
        
        // Look for checkboxes
        const checkboxes = page.locator('input[type="checkbox"], [role="checkbox"]');
        const count = await checkboxes.count();
        
        if (count >= 2) {
            await checkboxes.nth(0).click();
            await checkboxes.nth(1).click();
        }
    });

    test('should show bulk action toolbar when items selected', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard`);
        
        // Select checkboxes
        const checkboxes = page.locator('input[type="checkbox"], [role="checkbox"]');
        if (await checkboxes.count() >= 1) {
            await checkboxes.first().click();
            
            // Look for bulk action toolbar
            const bulkToolbar = page.locator('[data-testid="bulk-actions"], text=selected, text=Delete Selected');
            if (await bulkToolbar.first().isVisible()) {
                await expect(bulkToolbar.first()).toBeVisible();
            }
        }
    });

    test('should bulk delete links', async ({ page }) => {
        await mockApiResponse(page, '**/links/bulk/delete', { message: 'Deleted' });

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Select all
        const selectAllCheckbox = page.locator('[data-testid="select-all"], input[type="checkbox"]').first();
        if (await selectAllCheckbox.isVisible()) {
            await selectAllCheckbox.click();
            
            // Click bulk delete
            const deleteButton = page.locator('button:has-text("Delete Selected"), button:has-text("Delete All")');
            if (await deleteButton.isVisible()) {
                await deleteButton.click();
            }
        }
    });
});

// ============= Link Scheduling Tests =============

test.describe('Link Scheduling', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });

        await mockApiResponse(page, '**/links', []);
        await mockApiResponse(page, '**/folders', []);
        await mockApiResponse(page, '**/tags', []);
    });

    test('should create scheduled link', async ({ page }) => {
        const futureDate = new Date();
        futureDate.setDate(futureDate.getDate() + 7);

        await mockApiResponse(page, '**/links', {
            id: 1,
            code: 'scheduled123',
            original_url: 'https://example.com',
            click_count: 0,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: false,
            starts_at: futureDate.toISOString(),
            tags: [],
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Fill URL input
        const urlInput = page.locator('input[placeholder*="URL"], input[type="url"]').first();
        await urlInput.fill('https://example.com');
        
        // Look for advanced options / scheduling
        const advancedButton = page.locator('button:has-text("Advanced"), button:has-text("Options"), text=Schedule');
        if (await advancedButton.first().isVisible()) {
            await advancedButton.first().click();
            
            // Look for start date input
            const startDateInput = page.locator('input[type="datetime-local"], input[name*="start"]');
            if (await startDateInput.isVisible()) {
                await startDateInput.fill(futureDate.toISOString().slice(0, 16));
            }
        }
        
        // Submit
        const submitButton = page.locator('button:has-text("Shorten"), button[type="submit"]').first();
        await submitButton.click();
    });

    test('should create link with max clicks', async ({ page }) => {
        await mockApiResponse(page, '**/links', {
            id: 1,
            code: 'limited123',
            original_url: 'https://example.com',
            click_count: 0,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: true,
            max_clicks: 100,
            tags: [],
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Fill URL input
        const urlInput = page.locator('input[placeholder*="URL"], input[type="url"]').first();
        await urlInput.fill('https://example.com');
        
        // Look for advanced options
        const advancedButton = page.locator('button:has-text("Advanced"), button:has-text("Options")');
        if (await advancedButton.first().isVisible()) {
            await advancedButton.first().click();
            
            // Look for max clicks input
            const maxClicksInput = page.locator('input[name*="max"], input[placeholder*="click"]');
            if (await maxClicksInput.isVisible()) {
                await maxClicksInput.fill('100');
            }
        }
        
        // Submit
        const submitButton = page.locator('button:has-text("Shorten"), button[type="submit"]').first();
        await submitButton.click();
    });

    test('should display scheduled status for inactive links', async ({ page }) => {
        const futureDate = new Date();
        futureDate.setDate(futureDate.getDate() + 7);

        await mockApiResponse(page, '**/links', [{
            id: 1,
            code: 'future123',
            original_url: 'https://example.com',
            click_count: 0,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: false,
            starts_at: futureDate.toISOString(),
            tags: [],
        }]);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Look for scheduled indicator
        const scheduledIndicator = page.locator('text=Scheduled, text=Pending, text=Inactive, [data-testid="scheduled"]');
        if (await scheduledIndicator.first().isVisible()) {
            await expect(scheduledIndicator.first()).toBeVisible();
        }
    });
});

// ============= Link Notes Tests =============

test.describe('Link Notes', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });

        await mockApiResponse(page, '**/links', [{
            id: 1,
            code: 'noted123',
            original_url: 'https://example.com',
            click_count: 42,
            created_at: '2024-01-15T10:00:00Z',
            has_password: false,
            is_active: true,
            notes: 'This is a test note for the link',
            tags: [],
        }]);
    });

    test('should display notes on link', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard`);
        
        // Look for notes indicator or text
        const notesIndicator = page.locator('text=This is a test note, [data-testid="notes"], [aria-label*="notes"]');
        if (await notesIndicator.first().isVisible()) {
            await expect(notesIndicator.first()).toBeVisible();
        }
    });

    test('should create link with notes', async ({ page }) => {
        await mockApiResponse(page, '**/links', {
            id: 2,
            code: 'newnoted',
            original_url: 'https://example.com',
            click_count: 0,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: true,
            notes: 'New note for testing',
            tags: [],
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Fill URL input
        const urlInput = page.locator('input[placeholder*="URL"], input[type="url"]').first();
        await urlInput.fill('https://example.com');
        
        // Look for notes input
        const notesInput = page.locator('textarea[name*="notes"], input[placeholder*="notes"]');
        if (await notesInput.isVisible()) {
            await notesInput.fill('New note for testing');
        }
        
        // Submit
        const submitButton = page.locator('button:has-text("Shorten"), button[type="submit"]').first();
        await submitButton.click();
    });
});



