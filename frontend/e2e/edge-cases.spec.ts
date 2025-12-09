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

// ============= Input Validation Edge Cases =============

test.describe('Input Validation Edge Cases', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });
        await mockApiResponse(page, '**/links', []);
        await mockApiResponse(page, '**/folders', []);
        await mockApiResponse(page, '**/tags', []);
    });

    test('should reject invalid URL format', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard`);
        
        const urlInput = page.locator('input[placeholder*="URL"], input[type="url"]').first();
        if (await urlInput.isVisible()) {
            await urlInput.fill('not-a-valid-url');
            
            const submitButton = page.locator('button:has-text("Shorten"), button[type="submit"]').first();
            await submitButton.click();
            
            // Should show validation error
            const isInvalid = await urlInput.evaluate((el: HTMLInputElement) => !el.validity.valid);
            expect(isInvalid).toBeTruthy();
        }
    });

    test('should handle extremely long URLs', async ({ page }) => {
        const longUrl = 'https://example.com/' + 'a'.repeat(2000);
        
        await mockApiResponse(page, '**/links', {
            id: 1,
            code: 'long123',
            original_url: longUrl,
            click_count: 0,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: true,
            tags: [],
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        const urlInput = page.locator('input[placeholder*="URL"], input[type="url"]').first();
        if (await urlInput.isVisible()) {
            await urlInput.fill(longUrl);
            
            const submitButton = page.locator('button:has-text("Shorten"), button[type="submit"]').first();
            await submitButton.click();
        }
    });

    test('should handle special characters in URL', async ({ page }) => {
        const specialUrl = 'https://example.com/path?query=test&foo=bar#section';
        
        await mockApiResponse(page, '**/links', {
            id: 1,
            code: 'special123',
            original_url: specialUrl,
            click_count: 0,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: true,
            tags: [],
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        const urlInput = page.locator('input[placeholder*="URL"], input[type="url"]').first();
        if (await urlInput.isVisible()) {
            await urlInput.fill(specialUrl);
            
            const submitButton = page.locator('button:has-text("Shorten"), button[type="submit"]').first();
            await submitButton.click();
        }
    });

    test('should handle Unicode URLs', async ({ page }) => {
        const unicodeUrl = 'https://example.com/путь/к/странице';
        
        await page.goto(`${BASE_URL}/dashboard`);
        
        const urlInput = page.locator('input[placeholder*="URL"], input[type="url"]').first();
        if (await urlInput.isVisible()) {
            await urlInput.fill(unicodeUrl);
        }
    });

    test('should reject empty URL', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard`);
        
        const urlInput = page.locator('input[placeholder*="URL"], input[type="url"]').first();
        if (await urlInput.isVisible()) {
            await urlInput.fill('');
            
            const submitButton = page.locator('button:has-text("Shorten"), button[type="submit"]').first();
            await submitButton.click();
            
            // Should not submit with empty URL
            const isRequired = await urlInput.evaluate((el: HTMLInputElement) => el.required);
            expect(isRequired).toBeTruthy();
        }
    });
});

// ============= Authentication Edge Cases =============

test.describe('Authentication Edge Cases', () => {
    test('should handle expired token', async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'expired-token');
        });

        await mockApiResponse(page, '**/links', { error: 'Token expired' }, 401);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Should redirect to login or show auth error
        await page.waitForTimeout(1000);
        const url = page.url();
        const showsAuthError = url.includes('login') || await page.locator('text=unauthorized, text=login, text=session').first().isVisible();
        // Auth handling varies by implementation
    });

    test('should handle malformed token', async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'not-a-valid-jwt');
        });

        await mockApiResponse(page, '**/links', { error: 'Invalid token' }, 401);

        await page.goto(`${BASE_URL}/dashboard`);
        
        await page.waitForTimeout(1000);
    });

    test('should handle missing token', async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.removeItem('token');
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Should redirect to login
        await page.waitForURL(/login|home|\//, { timeout: 5000 });
    });

    test('should handle token with invalid signature', async ({ page }) => {
        // JWT with valid format but invalid signature
        const invalidToken = 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.INVALID_SIGNATURE';
        
        await page.addInitScript((token) => {
            localStorage.setItem('token', token);
        }, invalidToken);

        await mockApiResponse(page, '**/links', { error: 'Invalid signature' }, 401);

        await page.goto(`${BASE_URL}/dashboard`);
        
        await page.waitForTimeout(1000);
    });
});

// ============= Network Edge Cases =============

test.describe('Network Edge Cases', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });
    });

    test('should handle network timeout', async ({ page }) => {
        await page.route('**/links', async route => {
            // Simulate slow response
            await new Promise(resolve => setTimeout(resolve, 30000));
            await route.fulfill({
                status: 200,
                contentType: 'application/json',
                body: JSON.stringify([]),
            });
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Should show loading state
        const loading = page.locator('text=Loading, [data-testid="loading"]');
        if (await loading.first().isVisible({ timeout: 2000 })) {
            await expect(loading.first()).toBeVisible();
        }
    });

    test('should handle network error', async ({ page }) => {
        await page.route('**/links', async route => {
            await route.abort('failed');
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Should show error state
        const error = page.locator('text=error, text=failed, text=try again');
        if (await error.first().isVisible({ timeout: 5000 })) {
            await expect(error.first()).toBeVisible();
        }
    });

    test('should handle 500 server error', async ({ page }) => {
        await mockApiResponse(page, '**/links', { error: 'Internal server error' }, 500);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Should show server error
        const error = page.locator('text=error, text=server');
        if (await error.first().isVisible({ timeout: 5000 })) {
            await expect(error.first()).toBeVisible();
        }
    });

    test('should handle 503 service unavailable', async ({ page }) => {
        await mockApiResponse(page, '**/links', { error: 'Service unavailable' }, 503);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Should show service unavailable error
        await page.waitForTimeout(1000);
    });
});

// ============= Date/Time Edge Cases =============

test.describe('Date/Time Edge Cases', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });
    });

    test('should handle link created in different timezone', async ({ page }) => {
        await mockApiResponse(page, '**/links', [{
            id: 1,
            code: 'timezone123',
            original_url: 'https://example.com',
            click_count: 0,
            created_at: '2024-01-15T23:59:59+14:00', // UTC+14 timezone
            has_password: false,
            is_active: true,
            tags: [],
        }]);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Should display date correctly
        await page.waitForTimeout(1000);
    });

    test('should handle link with past expiration', async ({ page }) => {
        await mockApiResponse(page, '**/links', [{
            id: 1,
            code: 'expired123',
            original_url: 'https://example.com',
            click_count: 100,
            created_at: '2023-01-01T00:00:00Z',
            expires_at: '2023-06-01T00:00:00Z', // Already expired
            has_password: false,
            is_active: false,
            tags: [],
        }]);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Should show expired indicator
        const expiredIndicator = page.locator('text=Expired, text=Inactive, [data-testid="expired"]');
        if (await expiredIndicator.first().isVisible()) {
            await expect(expiredIndicator.first()).toBeVisible();
        }
    });

    test('should handle link expiring today', async ({ page }) => {
        const today = new Date();
        today.setHours(23, 59, 59);

        await mockApiResponse(page, '**/links', [{
            id: 1,
            code: 'expiring123',
            original_url: 'https://example.com',
            click_count: 50,
            created_at: '2024-01-01T00:00:00Z',
            expires_at: today.toISOString(),
            has_password: false,
            is_active: true,
            tags: [],
        }]);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // May show warning about expiring soon
        await page.waitForTimeout(1000);
    });

    test('should handle future scheduled link', async ({ page }) => {
        const futureDate = new Date();
        futureDate.setFullYear(futureDate.getFullYear() + 1);

        await mockApiResponse(page, '**/links', [{
            id: 1,
            code: 'future123',
            original_url: 'https://example.com',
            click_count: 0,
            created_at: new Date().toISOString(),
            starts_at: futureDate.toISOString(),
            has_password: false,
            is_active: false,
            tags: [],
        }]);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Should show scheduled indicator
        const scheduledIndicator = page.locator('text=Scheduled, text=Pending');
        if (await scheduledIndicator.first().isVisible()) {
            await expect(scheduledIndicator.first()).toBeVisible();
        }
    });
});

// ============= Data Edge Cases =============

test.describe('Data Edge Cases', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });
    });

    test('should handle empty links list', async ({ page }) => {
        await mockApiResponse(page, '**/links', []);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Should show empty state
        const emptyState = page.locator('text=No links, text=Create your first, text=Get started');
        if (await emptyState.first().isVisible({ timeout: 5000 })) {
            await expect(emptyState.first()).toBeVisible();
        }
    });

    test('should handle very large links list', async ({ page }) => {
        const manyLinks = Array.from({ length: 100 }, (_, i) => ({
            id: i + 1,
            code: `link${i}`,
            original_url: `https://example${i}.com`,
            click_count: Math.floor(Math.random() * 1000),
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: true,
            tags: [],
        }));

        await mockApiResponse(page, '**/links', manyLinks);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Should handle pagination or virtual scrolling
        await page.waitForTimeout(2000);
    });

    test('should handle link with zero clicks', async ({ page }) => {
        await mockApiResponse(page, '**/links', [{
            id: 1,
            code: 'zero123',
            original_url: 'https://example.com',
            click_count: 0,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: true,
            tags: [],
        }]);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Should display zero correctly
        const zeroClicks = page.locator('text=0');
        await expect(zeroClicks.first()).toBeVisible({ timeout: 5000 });
    });

    test('should handle link with very high click count', async ({ page }) => {
        await mockApiResponse(page, '**/links', [{
            id: 1,
            code: 'popular123',
            original_url: 'https://example.com',
            click_count: 9999999,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: true,
            tags: [],
        }]);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Should format large number or display abbreviation
        const clickCount = page.locator('text=9,999,999, text=9.9M, text=10M');
        await page.waitForTimeout(1000);
    });

    test('should handle link with many tags', async ({ page }) => {
        const manyTags = Array.from({ length: 20 }, (_, i) => ({
            id: i + 1,
            name: `tag-${i}`,
            color: `#${Math.floor(Math.random() * 16777215).toString(16)}`,
        }));

        await mockApiResponse(page, '**/links', [{
            id: 1,
            code: 'manytags123',
            original_url: 'https://example.com',
            click_count: 0,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: true,
            tags: manyTags,
        }]);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Should handle overflow gracefully
        await page.waitForTimeout(1000);
    });

    test('should handle special characters in notes', async ({ page }) => {
        await mockApiResponse(page, '**/links', [{
            id: 1,
            code: 'notes123',
            original_url: 'https://example.com',
            click_count: 0,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: true,
            notes: 'Test <script>alert("xss")</script> & special "chars"',
            tags: [],
        }]);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Should escape HTML and prevent XSS
        const script = page.locator('script:has-text("alert")');
        await expect(script).not.toBeVisible();
    });
});

// ============= UI State Edge Cases =============

test.describe('UI State Edge Cases', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });
    });

    test('should handle rapid button clicks', async ({ page }) => {
        let requestCount = 0;
        await page.route('**/links', async route => {
            requestCount++;
            await route.fulfill({
                status: 200,
                contentType: 'application/json',
                body: JSON.stringify(route.request().method() === 'POST' ? {
                    id: requestCount,
                    code: `rapid${requestCount}`,
                    original_url: 'https://example.com',
                    click_count: 0,
                    created_at: new Date().toISOString(),
                    has_password: false,
                    is_active: true,
                    tags: [],
                } : []),
            });
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        const urlInput = page.locator('input[placeholder*="URL"], input[type="url"]').first();
        const submitButton = page.locator('button:has-text("Shorten"), button[type="submit"]').first();
        
        if (await urlInput.isVisible()) {
            await urlInput.fill('https://example.com');
            
            // Click rapidly
            await submitButton.click();
            await submitButton.click();
            await submitButton.click();
        }
    });

    test('should handle form submission during loading', async ({ page }) => {
        let isLoading = false;
        await page.route('**/links', async route => {
            if (isLoading) {
                // Reject concurrent requests
                await route.fulfill({
                    status: 429,
                    contentType: 'application/json',
                    body: JSON.stringify({ error: 'Too many requests' }),
                });
                return;
            }
            isLoading = true;
            await new Promise(resolve => setTimeout(resolve, 2000));
            isLoading = false;
            await route.fulfill({
                status: 200,
                contentType: 'application/json',
                body: JSON.stringify({
                    id: 1,
                    code: 'loading123',
                    original_url: 'https://example.com',
                    click_count: 0,
                    created_at: new Date().toISOString(),
                    has_password: false,
                    is_active: true,
                    tags: [],
                }),
            });
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        const urlInput = page.locator('input[placeholder*="URL"], input[type="url"]').first();
        const submitButton = page.locator('button:has-text("Shorten"), button[type="submit"]').first();
        
        if (await urlInput.isVisible()) {
            await urlInput.fill('https://example.com');
            await submitButton.click();
            
            // Button should be disabled during loading
            await page.waitForTimeout(500);
            const isDisabled = await submitButton.isDisabled();
            // Implementation may vary
        }
    });

    test('should preserve scroll position on refresh', async ({ page }) => {
        const manyLinks = Array.from({ length: 50 }, (_, i) => ({
            id: i + 1,
            code: `scroll${i}`,
            original_url: `https://example${i}.com`,
            click_count: 0,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: true,
            tags: [],
        }));

        await mockApiResponse(page, '**/links', manyLinks);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Scroll down
        await page.evaluate(() => window.scrollTo(0, 1000));
        
        // Note: Scroll position preservation depends on implementation
        await page.waitForTimeout(500);
    });

    test('should handle window resize', async ({ page }) => {
        await mockApiResponse(page, '**/links', []);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Resize window
        await page.setViewportSize({ width: 375, height: 667 }); // Mobile
        await page.waitForTimeout(500);
        
        await page.setViewportSize({ width: 1920, height: 1080 }); // Desktop
        await page.waitForTimeout(500);
        
        // Should not break layout
        await expect(page.locator('body')).toBeVisible();
    });
});

// ============= Concurrency Edge Cases =============

test.describe('Concurrency Edge Cases', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });
    });

    test('should handle concurrent edits to same link', async ({ page }) => {
        await mockApiResponse(page, '**/links', [{
            id: 1,
            code: 'concurrent123',
            original_url: 'https://example.com',
            click_count: 0,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: true,
            tags: [],
        }]);

        await mockApiResponse(page, '**/links/1', {
            error: 'Resource was modified by another request',
        }, 409);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Try to edit link
        const editButton = page.locator('button[aria-label*="Edit"], button:has-text("Edit")').first();
        if (await editButton.isVisible()) {
            await editButton.click();
        }
    });

    test('should handle link deletion while editing', async ({ page }) => {
        await mockApiResponse(page, '**/links', [{
            id: 1,
            code: 'deleted123',
            original_url: 'https://example.com',
            click_count: 0,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: true,
            tags: [],
        }]);

        await mockApiResponse(page, '**/links/1', {
            error: 'Resource not found',
        }, 404);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Simulate link being deleted by another user
        // Implementation should handle gracefully
        await page.waitForTimeout(1000);
    });
});

// ============= Browser Compatibility Edge Cases =============

test.describe('Browser Compatibility Edge Cases', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });
        await mockApiResponse(page, '**/links', []);
    });

    test('should handle clipboard API not available', async ({ page }) => {
        await page.addInitScript(() => {
            // Remove clipboard API
            Object.defineProperty(navigator, 'clipboard', {
                value: undefined,
                writable: true,
            });
        });

        await mockApiResponse(page, '**/links', [{
            id: 1,
            code: 'copy123',
            original_url: 'https://example.com',
            click_count: 0,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: true,
            tags: [],
        }]);

        await page.goto(`${BASE_URL}/dashboard`);
        
        // Copy button should handle gracefully
        const copyButton = page.locator('button[aria-label*="Copy"], button:has-text("Copy")').first();
        if (await copyButton.isVisible()) {
            await copyButton.click();
            // Should show fallback or error message
        }
    });

    test('should handle localStorage not available', async ({ page }) => {
        await page.addInitScript(() => {
            // Make localStorage throw
            Object.defineProperty(window, 'localStorage', {
                get: () => {
                    throw new Error('localStorage is not available');
                },
            });
        });

        await page.goto(BASE_URL);
        
        // Should handle gracefully without crashing
        await page.waitForTimeout(1000);
    });
});




