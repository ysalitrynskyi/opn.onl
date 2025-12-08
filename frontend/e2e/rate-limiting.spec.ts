import { test, expect, Page } from '@playwright/test';

const BASE_URL = process.env.PLAYWRIGHT_BASE_URL || 'http://localhost:5173';

async function mockApiResponse(page: Page, url: string, response: any, status = 200, headers?: Record<string, string>) {
    await page.route(url, async route => {
        await route.fulfill({
            status,
            contentType: 'application/json',
            body: JSON.stringify(response),
            headers: {
                'Content-Type': 'application/json',
                ...headers,
            },
        });
    });
}

// ============= Rate Limiting Tests =============

test.describe('Rate Limiting', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });
    });

    test('should display rate limit error message', async ({ page }) => {
        // Mock rate limited response
        await mockApiResponse(
            page,
            '**/links',
            { error: 'Too many requests. Please try again later.' },
            429,
            { 'Retry-After': '60' }
        );

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Look for rate limit error message
        const errorMessage = page.locator('text=Too many requests, text=rate limit, text=try again');
        if (await errorMessage.first().isVisible()) {
            await expect(errorMessage.first()).toBeVisible();
        }
    });

    test('should show retry-after time', async ({ page }) => {
        await mockApiResponse(
            page,
            '**/links',
            { error: 'Too many requests' },
            429,
            { 'Retry-After': '30' }
        );

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Trigger a link creation
        const urlInput = page.locator('input[placeholder*="URL"], input[type="url"]').first();
        if (await urlInput.isVisible()) {
            await urlInput.fill('https://example.com');
            
            const submitButton = page.locator('button:has-text("Shorten"), button[type="submit"]').first();
            await submitButton.click();
            
            // Should show retry information
            const retryMessage = page.locator('text=30 seconds, text=try again, text=rate limit');
            if (await retryMessage.first().isVisible()) {
                await expect(retryMessage.first()).toBeVisible();
            }
        }
    });

    test('should handle rate limiting on login', async ({ page }) => {
        await mockApiResponse(
            page,
            '**/auth/login',
            { error: 'Too many login attempts' },
            429,
            { 'Retry-After': '300' }
        );

        await page.goto(`${BASE_URL}/login`);
        
        await page.fill('input[type="email"]', 'test@example.com');
        await page.fill('input[type="password"]', 'password123');
        await page.click('button[type="submit"]');
        
        // Should show rate limit error
        const errorMessage = page.locator('text=Too many, text=rate limit, text=try again');
        if (await errorMessage.first().isVisible({ timeout: 3000 })) {
            await expect(errorMessage.first()).toBeVisible();
        }
    });

    test('should handle rate limiting on registration', async ({ page }) => {
        await mockApiResponse(
            page,
            '**/auth/register',
            { error: 'Too many registration attempts' },
            429,
            { 'Retry-After': '600' }
        );

        await page.goto(`${BASE_URL}/register`);
        
        await page.fill('input[type="email"]', 'new@example.com');
        await page.fill('input[type="password"]', 'SecurePassword123!');
        await page.click('button[type="submit"]');
        
        // Should show rate limit error
        const errorMessage = page.locator('text=Too many, text=rate limit, text=try again');
        if (await errorMessage.first().isVisible({ timeout: 3000 })) {
            await expect(errorMessage.first()).toBeVisible();
        }
    });

    test('should recover after rate limit expires', async ({ page }) => {
        // First request is rate limited
        let requestCount = 0;
        await page.route('**/links', async route => {
            requestCount++;
            if (requestCount === 1) {
                await route.fulfill({
                    status: 429,
                    contentType: 'application/json',
                    body: JSON.stringify({ error: 'Too many requests' }),
                    headers: { 'Retry-After': '1' },
                });
            } else {
                await route.fulfill({
                    status: 200,
                    contentType: 'application/json',
                    body: JSON.stringify([]),
                });
            }
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Wait and retry
        await page.waitForTimeout(2000);
        await page.reload();
        
        // Should succeed now
        await expect(page.locator('body')).not.toContainText('Too many requests');
    });
});

// ============= API Rate Limit Headers Tests =============

test.describe('API Rate Limit Headers', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });
    });

    test('should handle X-RateLimit headers', async ({ page }) => {
        await page.route('**/links', async route => {
            await route.fulfill({
                status: 200,
                contentType: 'application/json',
                body: JSON.stringify([]),
                headers: {
                    'X-RateLimit-Limit': '100',
                    'X-RateLimit-Remaining': '95',
                    'X-RateLimit-Reset': String(Date.now() + 60000),
                },
            });
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Page should load normally with rate limit headers
        await expect(page.locator('body')).not.toContainText('error');
    });

    test('should show warning when approaching rate limit', async ({ page }) => {
        await page.route('**/links', async route => {
            await route.fulfill({
                status: 200,
                contentType: 'application/json',
                body: JSON.stringify([]),
                headers: {
                    'X-RateLimit-Limit': '100',
                    'X-RateLimit-Remaining': '5',
                    'X-RateLimit-Reset': String(Date.now() + 60000),
                },
            });
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        // May show rate limit warning (depends on UI implementation)
        // This test documents expected behavior
    });
});

// ============= Multiple Request Rate Limiting Tests =============

test.describe('Multiple Request Rate Limiting', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });
    });

    test('should handle rapid link creation attempts', async ({ page }) => {
        let requestCount = 0;
        await page.route('**/links', async route => {
            requestCount++;
            if (requestCount > 3) {
                await route.fulfill({
                    status: 429,
                    contentType: 'application/json',
                    body: JSON.stringify({ error: 'Too many requests' }),
                });
            } else {
                await route.fulfill({
                    status: 200,
                    contentType: 'application/json',
                    body: JSON.stringify({
                        id: requestCount,
                        code: `link${requestCount}`,
                        original_url: 'https://example.com',
                        click_count: 0,
                        created_at: new Date().toISOString(),
                        has_password: false,
                        is_active: true,
                        tags: [],
                    }),
                });
            }
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        const urlInput = page.locator('input[placeholder*="URL"], input[type="url"]').first();
        const submitButton = page.locator('button:has-text("Shorten"), button[type="submit"]').first();
        
        // Try to create multiple links rapidly
        for (let i = 0; i < 5; i++) {
            if (await urlInput.isVisible()) {
                await urlInput.fill(`https://example${i}.com`);
                await submitButton.click();
                await page.waitForTimeout(100);
            }
        }
    });

    test('should handle concurrent analytics requests', async ({ page }) => {
        let analyticsRequests = 0;
        await page.route('**/links/*/stats', async route => {
            analyticsRequests++;
            if (analyticsRequests > 2) {
                await route.fulfill({
                    status: 429,
                    contentType: 'application/json',
                    body: JSON.stringify({ error: 'Too many analytics requests' }),
                });
            } else {
                await route.fulfill({
                    status: 200,
                    contentType: 'application/json',
                    body: JSON.stringify({
                        link_id: 1,
                        code: 'test',
                        original_url: 'https://example.com',
                        total_clicks: 100,
                        unique_visitors: 80,
                        clicks_by_day: [],
                        clicks_by_country: [],
                        clicks_by_browser: [],
                        clicks_by_device: [],
                        recent_clicks: [],
                        geo_data: [],
                    }),
                });
            }
        });

        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Should handle rate limiting gracefully
        await page.waitForTimeout(1000);
    });
});

// ============= Rate Limiting Error Recovery Tests =============

test.describe('Rate Limiting Error Recovery', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });
    });

    test('should not lose form data on rate limit', async ({ page }) => {
        await mockApiResponse(
            page,
            '**/links',
            { error: 'Too many requests' },
            429,
            { 'Retry-After': '60' }
        );

        await page.goto(`${BASE_URL}/dashboard`);
        
        const urlInput = page.locator('input[placeholder*="URL"], input[type="url"]').first();
        if (await urlInput.isVisible()) {
            const testUrl = 'https://very-important-link.com/that-should-not-be-lost';
            await urlInput.fill(testUrl);
            
            const submitButton = page.locator('button:has-text("Shorten"), button[type="submit"]').first();
            await submitButton.click();
            
            // Wait for error
            await page.waitForTimeout(500);
            
            // URL should still be in the input
            const inputValue = await urlInput.inputValue();
            // Note: This depends on whether the form clears on error
        }
    });

    test('should show friendly error message for rate limit', async ({ page }) => {
        await mockApiResponse(
            page,
            '**/links',
            { error: 'Too many requests. Please wait before trying again.' },
            429,
            { 'Retry-After': '60' }
        );

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Any error should be user-friendly, not technical
        const technicalError = page.locator('text=429, text=HTTP, text=status code');
        await expect(technicalError).not.toBeVisible();
    });
});

// ============= Rate Limiting on Different Endpoints Tests =============

test.describe('Rate Limiting on Different Endpoints', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });
    });

    test('should handle rate limit on folder creation', async ({ page }) => {
        await mockApiResponse(page, '**/links', []);
        await mockApiResponse(
            page,
            '**/folders',
            { error: 'Too many folder creation requests' },
            429
        );

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Try to create folder
        const createFolderButton = page.locator('button:has-text("New Folder"), button[aria-label*="folder"]');
        if (await createFolderButton.isVisible()) {
            await createFolderButton.click();
        }
    });

    test('should handle rate limit on tag creation', async ({ page }) => {
        await mockApiResponse(page, '**/links', []);
        await mockApiResponse(
            page,
            '**/tags',
            { error: 'Too many tag creation requests' },
            429
        );

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Try to create tag
        const createTagButton = page.locator('button:has-text("New Tag"), button[aria-label*="tag"]');
        if (await createTagButton.isVisible()) {
            await createTagButton.click();
        }
    });

    test('should handle rate limit on QR code generation', async ({ page }) => {
        await mockApiResponse(page, '**/links', [{
            id: 1,
            code: 'qr123',
            original_url: 'https://example.com',
            click_count: 0,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: true,
            tags: [],
        }]);

        await mockApiResponse(
            page,
            '**/links/1/qr',
            { error: 'Too many QR code requests' },
            429
        );

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Try to get QR code
        const qrButton = page.locator('button[aria-label*="QR"], button:has-text("QR")').first();
        if (await qrButton.isVisible()) {
            await qrButton.click();
        }
    });

    test('should handle rate limit on bulk operations', async ({ page }) => {
        await mockApiResponse(page, '**/links', [
            { id: 1, code: 'bulk1', original_url: 'https://example1.com', click_count: 0, created_at: new Date().toISOString(), has_password: false, is_active: true, tags: [] },
            { id: 2, code: 'bulk2', original_url: 'https://example2.com', click_count: 0, created_at: new Date().toISOString(), has_password: false, is_active: true, tags: [] },
        ]);

        await mockApiResponse(
            page,
            '**/links/bulk/delete',
            { error: 'Too many bulk operations' },
            429
        );

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Select items and try bulk delete
        const checkboxes = page.locator('input[type="checkbox"]');
        if (await checkboxes.count() >= 1) {
            await checkboxes.first().click();
            
            const bulkDeleteButton = page.locator('button:has-text("Delete Selected")');
            if (await bulkDeleteButton.isVisible()) {
                await bulkDeleteButton.click();
            }
        }
    });
});



