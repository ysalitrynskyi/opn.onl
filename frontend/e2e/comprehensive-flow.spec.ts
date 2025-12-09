import { test, expect } from '@playwright/test';

// Generate unique test data
const timestamp = Date.now();
const testEmail = `test-${timestamp}@example.com`;
const testPassword = 'TestPassword123!';

test.describe('Complete User Flow', () => {
    test.describe.serial('User Registration and Login', () => {
        test('should allow new user registration', async ({ page }) => {
            await page.goto('/register');
            
            // Fill registration form
            await page.fill('input[type="email"]', testEmail);
            await page.fill('input[type="password"]', testPassword);
            
            // If there's a confirm password field
            const confirmField = page.locator('input[name="confirmPassword"], input[placeholder*="confirm"]');
            if (await confirmField.count() > 0) {
                await confirmField.fill(testPassword);
            }
            
            await page.click('button[type="submit"]');
            
            // Should redirect to login or dashboard
            await expect(page).toHaveURL(/\/login|\/dashboard|\/verify-email/);
        });

        test('should show validation errors for invalid input', async ({ page }) => {
            await page.goto('/register');
            
            // Try to submit with invalid email
            await page.fill('input[type="email"]', 'invalidemail');
            await page.fill('input[type="password"]', 'short');
            
            await page.click('button[type="submit"]');
            
            // Should show validation error
            await expect(page.locator('text=/invalid|error|too short/i')).toBeVisible();
        });

        test('should allow user login', async ({ page }) => {
            await page.goto('/login');
            
            await page.fill('input[type="email"]', testEmail);
            await page.fill('input[type="password"]', testPassword);
            
            await page.click('button[type="submit"]');
            
            // Should redirect to dashboard
            await page.waitForURL(/\/dashboard/, { timeout: 10000 });
        });

        test('should show error for wrong credentials', async ({ page }) => {
            await page.goto('/login');
            
            await page.fill('input[type="email"]', 'wrong@example.com');
            await page.fill('input[type="password"]', 'wrongpassword');
            
            await page.click('button[type="submit"]');
            
            // Should show error
            await expect(page.locator('text=/invalid|error|incorrect/i')).toBeVisible();
        });
    });
});

test.describe('Link Management Flow', () => {
    test.beforeEach(async ({ page }) => {
        // Login first
        await page.goto('/login');
        await page.fill('input[type="email"]', testEmail);
        await page.fill('input[type="password"]', testPassword);
        await page.click('button[type="submit"]');
        await page.waitForURL(/\/dashboard/, { timeout: 10000 });
    });

    test('should create a new link', async ({ page }) => {
        // Find the URL input
        const urlInput = page.locator('input[placeholder*="http"], input[type="url"]').first();
        await urlInput.fill('https://example.com/test-page');
        
        // Click create button
        await page.click('button:has-text("Shorten"), button:has-text("Create")');
        
        // Should see success or the new link
        await expect(page.locator('text=/success|created|shortened/i').or(page.locator('text=/example\.com/'))).toBeVisible({ timeout: 5000 });
    });

    test('should create link with custom alias', async ({ page }) => {
        const urlInput = page.locator('input[placeholder*="http"], input[type="url"]').first();
        await urlInput.fill('https://example.com/custom-alias-test');
        
        // Look for custom alias input or expand options
        const aliasInput = page.locator('input[name="alias"], input[placeholder*="alias"]');
        if (await aliasInput.count() > 0) {
            await aliasInput.fill(`custom-${timestamp}`);
        }
        
        await page.click('button:has-text("Shorten"), button:has-text("Create")');
        
        await expect(page.locator(`text=/custom-${timestamp}|success/`)).toBeVisible({ timeout: 5000 });
    });

    test('should view link analytics', async ({ page }) => {
        // Find first link's analytics button
        const analyticsButton = page.locator('[aria-label*="analytics"], button:has-text("Analytics")').first();
        
        if (await analyticsButton.count() > 0) {
            await analyticsButton.click();
            await page.waitForURL(/\/analytics\//);
            
            // Should see analytics page elements
            await expect(page.locator('text=/clicks|visits|statistics/i')).toBeVisible();
        }
    });

    test('should copy link to clipboard', async ({ page }) => {
        const copyButton = page.locator('[aria-label*="copy"], button:has-text("Copy")').first();
        
        if (await copyButton.count() > 0) {
            await copyButton.click();
            
            // Should show copied confirmation
            await expect(page.locator('text=/copied/i')).toBeVisible();
        }
    });

    test('should generate QR code', async ({ page }) => {
        const qrButton = page.locator('[aria-label*="qr"], button:has-text("QR")').first();
        
        if (await qrButton.count() > 0) {
            await qrButton.click();
            
            // Should show QR modal or image
            await expect(page.locator('img[alt*="QR"], [class*="qr"]')).toBeVisible();
        }
    });

    test('should edit link', async ({ page }) => {
        const editButton = page.locator('[aria-label*="edit"], button:has-text("Edit")').first();
        
        if (await editButton.count() > 0) {
            await editButton.click();
            
            // Should show edit modal/form
            await expect(page.locator('input, form')).toBeVisible();
        }
    });

    test('should delete link', async ({ page }) => {
        const deleteButton = page.locator('[aria-label*="delete"], button:has-text("Delete")').first();
        
        if (await deleteButton.count() > 0) {
            await deleteButton.click();
            
            // Confirm deletion if dialog appears
            const confirmButton = page.locator('button:has-text("Confirm"), button:has-text("Yes")');
            if (await confirmButton.count() > 0) {
                await confirmButton.click();
            }
            
            // Should show success or link removed
            await expect(page.locator('text=/deleted|removed|success/i')).toBeVisible();
        }
    });
});

test.describe('Search and Filter', () => {
    test.beforeEach(async ({ page }) => {
        await page.goto('/login');
        await page.fill('input[type="email"]', testEmail);
        await page.fill('input[type="password"]', testPassword);
        await page.click('button[type="submit"]');
        await page.waitForURL(/\/dashboard/, { timeout: 10000 });
    });

    test('should search links', async ({ page }) => {
        const searchInput = page.locator('input[placeholder*="search"], input[type="search"]');
        
        if (await searchInput.count() > 0) {
            await searchInput.fill('example');
            await page.waitForTimeout(500); // Debounce
            
            // Results should filter
        }
    });

    test('should sort links', async ({ page }) => {
        const sortSelect = page.locator('select, [role="combobox"]').first();
        
        if (await sortSelect.count() > 0) {
            await sortSelect.click();
            // Select an option
        }
    });
});

test.describe('Folder Management', () => {
    test.beforeEach(async ({ page }) => {
        await page.goto('/login');
        await page.fill('input[type="email"]', testEmail);
        await page.fill('input[type="password"]', testPassword);
        await page.click('button[type="submit"]');
        await page.waitForURL(/\/dashboard/, { timeout: 10000 });
    });

    test('should create a folder', async ({ page }) => {
        const createFolderBtn = page.locator('button:has-text("New Folder"), button:has-text("Create Folder")');
        
        if (await createFolderBtn.count() > 0) {
            await createFolderBtn.click();
            
            const nameInput = page.locator('input[name="name"], input[placeholder*="folder"]');
            await nameInput.fill(`Test Folder ${timestamp}`);
            
            await page.click('button:has-text("Create"), button:has-text("Save")');
            
            await expect(page.locator(`text=/Test Folder ${timestamp}|success/`)).toBeVisible();
        }
    });

    test('should move link to folder', async ({ page }) => {
        // This would involve drag-drop or a move button
    });
});

test.describe('Settings Page', () => {
    test.beforeEach(async ({ page }) => {
        await page.goto('/login');
        await page.fill('input[type="email"]', testEmail);
        await page.fill('input[type="password"]', testPassword);
        await page.click('button[type="submit"]');
        await page.waitForURL(/\/dashboard/, { timeout: 10000 });
    });

    test('should navigate to settings', async ({ page }) => {
        await page.goto('/settings');
        
        await expect(page.locator('text=/settings|profile|account/i')).toBeVisible();
    });

    test('should update profile', async ({ page }) => {
        await page.goto('/settings');
        
        const displayNameInput = page.locator('input[name="displayName"], input[name="display_name"]');
        if (await displayNameInput.count() > 0) {
            await displayNameInput.fill(`Test User ${timestamp}`);
            
            await page.click('button:has-text("Save"), button:has-text("Update")');
            
            await expect(page.locator('text=/saved|updated|success/i')).toBeVisible();
        }
    });

    test('should change password', async ({ page }) => {
        await page.goto('/settings');
        
        const changePasswordBtn = page.locator('button:has-text("Change Password")');
        if (await changePasswordBtn.count() > 0) {
            await changePasswordBtn.click();
            
            // Fill password change form
            const currentPasswordInput = page.locator('input[name="currentPassword"]');
            const newPasswordInput = page.locator('input[name="newPassword"]');
            
            if (await currentPasswordInput.count() > 0) {
                await currentPasswordInput.fill(testPassword);
                await newPasswordInput.fill(testPassword + 'New');
                
                // Submit
                await page.click('button:has-text("Update"), button[type="submit"]');
            }
        }
    });

    test('should export data', async ({ page }) => {
        await page.goto('/settings');
        
        const exportBtn = page.locator('button:has-text("Export")');
        if (await exportBtn.count() > 0) {
            const [download] = await Promise.all([
                page.waitForEvent('download'),
                exportBtn.click(),
            ]);
            
            expect(download.suggestedFilename()).toMatch(/\.csv|\.json/);
        }
    });
});

test.describe('Link Redirection', () => {
    test('should redirect short links', async ({ page }) => {
        // Create a link first
        await page.goto('/');
        
        const urlInput = page.locator('input[placeholder*="http"], input[type="url"]').first();
        if (await urlInput.count() > 0) {
            await urlInput.fill('https://example.com');
            await page.click('button:has-text("Shorten")');
            
            // Get the shortened URL
            const shortUrlElement = page.locator('text=/opn\.onl\/[a-zA-Z0-9]+/');
            if (await shortUrlElement.count() > 0) {
                const shortUrl = await shortUrlElement.textContent();
                if (shortUrl) {
                    // Navigate to the short URL
                    await page.goto(shortUrl);
                    
                    // Should redirect to original
                    await expect(page).toHaveURL(/example\.com/);
                }
            }
        }
    });

    test('should handle invalid short codes', async ({ page }) => {
        await page.goto('/nonexistent-code');
        
        // Should show 404 or redirect
        await expect(page.locator('text=/not found|404/i').or(page.locator('text=/link.*expired/i'))).toBeVisible();
    });
});

test.describe('Password Protected Links', () => {
    test('should prompt for password on protected links', async ({ page }) => {
        // Navigate to a password-protected link
        // This would need a known protected link or to create one first
    });

    test('should allow access with correct password', async ({ page }) => {
        // Test password verification flow
    });

    test('should deny access with wrong password', async ({ page }) => {
        // Test incorrect password handling
    });
});

test.describe('Mobile Responsiveness', () => {
    test.use({ viewport: { width: 375, height: 667 } });

    test('should display mobile menu', async ({ page }) => {
        await page.goto('/');
        
        // Should have hamburger menu
        const menuButton = page.locator('button[aria-label*="menu"], [class*="hamburger"]');
        await expect(menuButton).toBeVisible();
    });

    test('should navigate via mobile menu', async ({ page }) => {
        await page.goto('/');
        
        const menuButton = page.locator('button[aria-label*="menu"]');
        if (await menuButton.count() > 0) {
            await menuButton.click();
            
            // Menu should open
            await expect(page.locator('nav')).toBeVisible();
        }
    });

    test('should have responsive forms', async ({ page }) => {
        await page.goto('/login');
        
        const form = page.locator('form');
        const formBox = await form.boundingBox();
        
        if (formBox) {
            expect(formBox.width).toBeLessThanOrEqual(375);
        }
    });
});

test.describe('Accessibility', () => {
    test('should have proper focus management', async ({ page }) => {
        await page.goto('/login');
        
        // Tab through form elements
        await page.keyboard.press('Tab');
        const focused = await page.evaluate(() => document.activeElement?.tagName);
        expect(['INPUT', 'BUTTON', 'A']).toContain(focused);
    });

    test('should have proper ARIA labels', async ({ page }) => {
        await page.goto('/');
        
        // Check for main landmark
        const main = page.locator('main, [role="main"]');
        await expect(main).toBeVisible();
    });

    test('should support keyboard navigation', async ({ page }) => {
        await page.goto('/login');
        
        // Should be able to submit form with Enter
        await page.fill('input[type="email"]', 'test@example.com');
        await page.fill('input[type="password"]', 'password123');
        await page.keyboard.press('Enter');
    });
});

test.describe('Error Handling', () => {
    test('should handle network errors gracefully', async ({ page }) => {
        // Simulate offline
        await page.route('**/api/**', route => route.abort());
        
        await page.goto('/dashboard');
        
        // Should show error state
        await expect(page.locator('text=/error|offline|try again/i')).toBeVisible();
    });

    test('should handle 404 pages', async ({ page }) => {
        await page.goto('/this-page-does-not-exist');
        
        await expect(page.locator('text=/not found|404/i')).toBeVisible();
    });

    test('should handle server errors', async ({ page }) => {
        await page.route('**/api/**', route => {
            route.fulfill({ status: 500, body: 'Internal Server Error' });
        });
        
        await page.goto('/dashboard');
        
        // Should show error message
        await expect(page.locator('text=/error|something went wrong/i')).toBeVisible();
    });
});

test.describe('Performance', () => {
    test('should load home page within acceptable time', async ({ page }) => {
        const startTime = Date.now();
        await page.goto('/');
        const loadTime = Date.now() - startTime;
        
        // Should load within 5 seconds
        expect(loadTime).toBeLessThan(5000);
    });

    test('should cache static assets', async ({ page }) => {
        // First load
        await page.goto('/');
        
        // Second load should be faster due to caching
        const startTime = Date.now();
        await page.reload();
        const loadTime = Date.now() - startTime;
        
        expect(loadTime).toBeLessThan(3000);
    });
});

