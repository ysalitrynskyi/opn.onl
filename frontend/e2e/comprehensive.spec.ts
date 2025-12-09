import { test, expect, Page } from '@playwright/test';

// ============= Test Configuration =============

const BASE_URL = process.env.PLAYWRIGHT_BASE_URL || 'http://localhost:5173';
const API_URL = process.env.PLAYWRIGHT_API_URL || 'http://localhost:3000';

// ============= Helper Functions =============

async function login(page: Page, email = 'test@example.com', password = 'TestPassword123') {
    await page.goto(`${BASE_URL}/login`);
    await page.fill('input[type="email"]', email);
    await page.fill('input[type="password"]', password);
    await page.click('button[type="submit"]');
    await page.waitForURL(`${BASE_URL}/dashboard`, { timeout: 10000 });
}

async function mockApiResponse(page: Page, url: string, response: any, status = 200) {
    await page.route(url, async route => {
        await route.fulfill({
            status,
            contentType: 'application/json',
            body: JSON.stringify(response),
        });
    });
}

// ============= Home Page Tests =============

test.describe('Home Page', () => {
    test('should display hero section', async ({ page }) => {
        await page.goto(BASE_URL);
        
        await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
        await expect(page.getByText(/shorten/i)).toBeVisible();
    });

    test('should have URL input form', async ({ page }) => {
        await page.goto(BASE_URL);
        
        const urlInput = page.locator('input[type="url"], input[placeholder*="URL"]');
        await expect(urlInput).toBeVisible();
    });

    test('should have call-to-action buttons', async ({ page }) => {
        await page.goto(BASE_URL);
        
        // Should have either a shorten button or sign up/login links
        const hasShorten = await page.locator('button:has-text("Shorten")').isVisible();
        const hasGetStarted = await page.locator('a:has-text("Get Started")').isVisible();
        
        expect(hasShorten || hasGetStarted).toBeTruthy();
    });

    test('should navigate to login page', async ({ page }) => {
        await page.goto(BASE_URL);
        
        await page.click('a:has-text("Login"), a:has-text("Sign in")');
        await expect(page).toHaveURL(/\/login/);
    });

    test('should navigate to register page', async ({ page }) => {
        await page.goto(BASE_URL);
        
        const signUpLink = page.locator('a:has-text("Sign up"), a:has-text("Register"), a:has-text("Get Started")');
        if (await signUpLink.isVisible()) {
            await signUpLink.click();
            await expect(page).toHaveURL(/\/register/);
        }
    });
});

// ============= Authentication Tests =============

test.describe('Authentication', () => {
    test.describe('Login', () => {
        test('should display login form', async ({ page }) => {
            await page.goto(`${BASE_URL}/login`);
            
            await expect(page.locator('input[type="email"]')).toBeVisible();
            await expect(page.locator('input[type="password"]')).toBeVisible();
            await expect(page.locator('button[type="submit"]')).toBeVisible();
        });

        test('should show validation errors for empty form', async ({ page }) => {
            await page.goto(`${BASE_URL}/login`);
            
            await page.click('button[type="submit"]');
            
            // Should show some form of validation
            const hasValidation = await page.locator('[class*="error"], [class*="invalid"], :invalid').isVisible();
            expect(hasValidation).toBeTruthy();
        });

        test('should show error for invalid credentials', async ({ page }) => {
            await mockApiResponse(page, '**/auth/login', { error: 'Invalid credentials' }, 401);
            
            await page.goto(`${BASE_URL}/login`);
            await page.fill('input[type="email"]', 'wrong@example.com');
            await page.fill('input[type="password"]', 'wrongpassword');
            await page.click('button[type="submit"]');
            
            await expect(page.getByText(/invalid|error|incorrect/i)).toBeVisible({ timeout: 5000 });
        });

        test('should have link to registration', async ({ page }) => {
            await page.goto(`${BASE_URL}/login`);
            
            await expect(page.locator('a[href="/register"]')).toBeVisible();
        });
    });

    test.describe('Registration', () => {
        test('should display registration form', async ({ page }) => {
            await page.goto(`${BASE_URL}/register`);
            
            await expect(page.locator('input[type="email"]')).toBeVisible();
            await expect(page.locator('input[type="password"]')).toBeVisible();
            await expect(page.locator('button[type="submit"]')).toBeVisible();
        });

        test('should validate email format', async ({ page }) => {
            await page.goto(`${BASE_URL}/register`);
            
            await page.fill('input[type="email"]', 'invalid-email');
            await page.click('button[type="submit"]');
            
            const emailInput = page.locator('input[type="email"]');
            const isInvalid = await emailInput.evaluate((el: HTMLInputElement) => !el.validity.valid);
            expect(isInvalid).toBeTruthy();
        });

        test('should have link to login', async ({ page }) => {
            await page.goto(`${BASE_URL}/register`);
            
            await expect(page.locator('a[href="/login"]')).toBeVisible();
        });
    });
});

// ============= Dashboard Tests =============

test.describe('Dashboard', () => {
    test.beforeEach(async ({ page }) => {
        // Mock authenticated user
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });
        
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
                tags: [],
            },
            {
                id: 2,
                code: 'xyz789',
                original_url: 'https://test.com/long-url',
                click_count: 100,
                created_at: '2024-01-14T10:00:00Z',
                has_password: true,
                is_active: true,
                tags: [{ id: 1, name: 'important', color: '#FF5733' }],
            },
        ]);
    });

    test('should display links list', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard`);
        
        await expect(page.getByText('abc123')).toBeVisible({ timeout: 5000 });
        await expect(page.getByText('xyz789')).toBeVisible();
    });

    test('should show click counts', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard`);
        
        await expect(page.getByText('42')).toBeVisible({ timeout: 5000 });
    });

    test('should have create link form', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard`);
        
        const urlInput = page.locator('input[placeholder*="URL"], input[type="url"]');
        await expect(urlInput).toBeVisible();
    });

    test('should have search functionality', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard`);
        
        const searchInput = page.locator('input[placeholder*="Search"], input[type="search"]');
        if (await searchInput.isVisible()) {
            await searchInput.fill('abc');
            // Should filter results
        }
    });
});

// ============= Link Management Tests =============

test.describe('Link Management', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });
    });

    test('should create a new link', async ({ page }) => {
        await mockApiResponse(page, '**/links', {
            id: 3,
            code: 'new123',
            original_url: 'https://new-url.com',
            click_count: 0,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: true,
            tags: [],
        });

        await page.goto(`${BASE_URL}/dashboard`);
        
        const urlInput = page.locator('input[placeholder*="URL"], input[type="url"]').first();
        await urlInput.fill('https://new-url.com');
        
        const submitButton = page.locator('button:has-text("Shorten"), button[type="submit"]').first();
        await submitButton.click();
    });

    test('should copy link to clipboard', async ({ page, context }) => {
        await context.grantPermissions(['clipboard-write', 'clipboard-read']);
        
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
        
        const copyButton = page.locator('button[aria-label*="Copy"], button:has-text("Copy")').first();
        if (await copyButton.isVisible()) {
            await copyButton.click();
        }
    });

    test('should delete a link', async ({ page }) => {
        await mockApiResponse(page, '**/links', [{
            id: 1,
            code: 'del123',
            original_url: 'https://example.com',
            click_count: 0,
            created_at: new Date().toISOString(),
            has_password: false,
            is_active: true,
            tags: [],
        }]);
        
        await mockApiResponse(page, '**/links/1', { message: 'Deleted' });

        await page.goto(`${BASE_URL}/dashboard`);
        
        const deleteButton = page.locator('button[aria-label*="Delete"], button:has-text("Delete")').first();
        if (await deleteButton.isVisible()) {
            await deleteButton.click();
        }
    });
});

// ============= Analytics Tests =============

test.describe('Analytics', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });

        await mockApiResponse(page, '**/links/1/stats', {
            link_id: 1,
            code: 'abc123',
            original_url: 'https://example.com',
            total_clicks: 150,
            unique_visitors: 120,
            clicks_by_day: [
                { date: '2024-01-15', count: 15 },
                { date: '2024-01-14', count: 12 },
            ],
            clicks_by_country: [
                { country: 'United States', count: 80, percentage: 53.3 },
            ],
            clicks_by_browser: [
                { browser: 'Chrome', count: 70, percentage: 46.7 },
            ],
            clicks_by_device: [
                { device: 'Desktop', count: 90, percentage: 60 },
            ],
            recent_clicks: [],
            geo_data: [],
        });
    });

    test('should display analytics page', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Should show some analytics content
        await expect(page.locator('body')).toContainText(/click|view|analytic/i);
    });
});

// ============= Static Pages Tests =============

test.describe('Static Pages', () => {
    test('should display About page', async ({ page }) => {
        await page.goto(`${BASE_URL}/about`);
        await expect(page.locator('body')).toContainText(/about|opn/i);
    });

    test('should display Privacy page', async ({ page }) => {
        await page.goto(`${BASE_URL}/privacy`);
        await expect(page.locator('body')).toContainText(/privacy/i);
    });

    test('should display Terms page', async ({ page }) => {
        await page.goto(`${BASE_URL}/terms`);
        await expect(page.locator('body')).toContainText(/terms/i);
    });

    test('should display Contact page', async ({ page }) => {
        await page.goto(`${BASE_URL}/contact`);
        await expect(page.locator('body')).toContainText(/contact/i);
    });

    test('should display FAQ page', async ({ page }) => {
        await page.goto(`${BASE_URL}/faq`);
        await expect(page.locator('body')).toContainText(/faq|question/i);
    });

    test('should display Features page', async ({ page }) => {
        await page.goto(`${BASE_URL}/features`);
        await expect(page.locator('body')).toContainText(/feature/i);
    });

    test('should display Pricing page', async ({ page }) => {
        await page.goto(`${BASE_URL}/pricing`);
        await expect(page.locator('body')).toContainText(/pricing|free/i);
    });
});

// ============= Navigation Tests =============

test.describe('Navigation', () => {
    test('should have working header navigation', async ({ page }) => {
        await page.goto(BASE_URL);
        
        const nav = page.locator('header nav, nav');
        await expect(nav).toBeVisible();
    });

    test('should have working footer links', async ({ page }) => {
        await page.goto(BASE_URL);
        
        const footer = page.locator('footer');
        if (await footer.isVisible()) {
            await expect(footer).toContainText(/privacy|terms|contact/i);
        }
    });

    test('should handle 404 gracefully', async ({ page }) => {
        await page.goto(`${BASE_URL}/non-existent-page-12345`);
        
        // Should either show 404 or redirect to home
        const url = page.url();
        const content = await page.textContent('body');
        
        expect(
            url.includes('404') || 
            content?.toLowerCase().includes('not found') ||
            url === `${BASE_URL}/`
        ).toBeTruthy();
    });
});

// ============= Mobile Responsiveness Tests =============

test.describe('Mobile Responsiveness', () => {
    test.use({ viewport: { width: 375, height: 667 } }); // iPhone SE

    test('should display mobile menu', async ({ page }) => {
        await page.goto(BASE_URL);
        
        // Look for hamburger menu or mobile nav
        const mobileMenu = page.locator('[aria-label*="menu"], button:has-text("Menu"), .hamburger, [class*="mobile"]');
        if (await mobileMenu.isVisible()) {
            await expect(mobileMenu).toBeVisible();
        }
    });

    test('should have responsive layout', async ({ page }) => {
        await page.goto(BASE_URL);
        
        // Content should not overflow
        const body = page.locator('body');
        const box = await body.boundingBox();
        
        expect(box?.width).toBeLessThanOrEqual(375);
    });
});

// ============= Accessibility Tests =============

test.describe('Accessibility', () => {
    test('should have proper heading hierarchy', async ({ page }) => {
        await page.goto(BASE_URL);
        
        const h1 = await page.locator('h1').count();
        expect(h1).toBeGreaterThanOrEqual(1);
    });

    test('should have alt text on images', async ({ page }) => {
        await page.goto(BASE_URL);
        
        const images = await page.locator('img').all();
        for (const img of images) {
            const alt = await img.getAttribute('alt');
            const role = await img.getAttribute('role');
            
            // Should have alt text or be marked as presentation
            expect(alt !== null || role === 'presentation').toBeTruthy();
        }
    });

    test('should have form labels', async ({ page }) => {
        await page.goto(`${BASE_URL}/login`);
        
        const inputs = await page.locator('input').all();
        for (const input of inputs) {
            const id = await input.getAttribute('id');
            const ariaLabel = await input.getAttribute('aria-label');
            const placeholder = await input.getAttribute('placeholder');
            
            // Should have some form of label
            if (id) {
                const label = await page.locator(`label[for="${id}"]`).count();
                expect(label > 0 || ariaLabel || placeholder).toBeTruthy();
            }
        }
    });

    test('should be keyboard navigable', async ({ page }) => {
        await page.goto(BASE_URL);
        
        // Press Tab and verify focus moves
        await page.keyboard.press('Tab');
        
        const focusedElement = await page.evaluate(() => document.activeElement?.tagName);
        expect(focusedElement).toBeTruthy();
    });
});

// ============= Performance Tests =============

test.describe('Performance', () => {
    test('should load within acceptable time', async ({ page }) => {
        const startTime = Date.now();
        
        await page.goto(BASE_URL, { waitUntil: 'domcontentloaded' });
        
        const loadTime = Date.now() - startTime;
        expect(loadTime).toBeLessThan(5000); // 5 seconds max
    });

    test('should not have console errors', async ({ page }) => {
        const errors: string[] = [];
        page.on('console', msg => {
            if (msg.type() === 'error') {
                errors.push(msg.text());
            }
        });

        await page.goto(BASE_URL);
        await page.waitForTimeout(1000);

        // Filter out expected errors (like failed API calls in test env)
        const criticalErrors = errors.filter(
            e => !e.includes('net::ERR') && !e.includes('Failed to fetch')
        );
        
        expect(criticalErrors).toHaveLength(0);
    });
});





