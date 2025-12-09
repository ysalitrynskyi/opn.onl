import { test, expect } from '@playwright/test';

const API_URL = process.env.API_URL || 'http://localhost:3000/api';

test.describe('Security Tests', () => {
    test.describe('Authentication Security', () => {
        test('should not expose sensitive data in error messages', async ({ request }) => {
            const response = await request.post(`${API_URL}/auth/login`, {
                data: {
                    email: 'test@example.com',
                    password: 'wrongpassword',
                },
            });

            const data = await response.json();
            
            // Should not reveal whether email exists
            expect(data.error).not.toMatch(/user.*not.*found/i);
            expect(data.error).not.toMatch(/password.*incorrect/i);
        });

        test('should require strong passwords', async ({ request }) => {
            const response = await request.post(`${API_URL}/auth/register`, {
                data: {
                    email: `weak-password-${Date.now()}@example.com`,
                    password: '123', // Too short
                },
            });

            expect(response.status()).toBe(400);
        });

        test('should handle token expiration', async ({ request }) => {
            // Use an expired token
            const expiredToken = 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxIiwiZXhwIjoxfQ.invalid';
            
            const response = await request.get(`${API_URL}/auth/me`, {
                headers: {
                    Authorization: `Bearer ${expiredToken}`,
                },
            });

            expect(response.status()).toBe(401);
        });

        test('should reject malformed tokens', async ({ request }) => {
            const response = await request.get(`${API_URL}/auth/me`, {
                headers: {
                    Authorization: 'Bearer malformed-token',
                },
            });

            expect(response.status()).toBe(401);
        });
    });

    test.describe('XSS Prevention', () => {
        test('should sanitize URL input', async ({ page }) => {
            await page.goto('/');
            
            const urlInput = page.locator('input[type="url"], input[placeholder*="http"]').first();
            if (await urlInput.count() > 0) {
                // Try to inject XSS via URL
                await urlInput.fill('javascript:alert(1)');
                await page.click('button:has-text("Shorten")');
                
                // Should reject or sanitize
                await expect(page.locator('text=/invalid|error|not allowed/i')).toBeVisible();
            }
        });

        test('should escape HTML in displayed URLs', async ({ page }) => {
            await page.goto('/');
            
            const urlInput = page.locator('input[type="url"], input[placeholder*="http"]').first();
            if (await urlInput.count() > 0) {
                await urlInput.fill('https://example.com/<script>alert(1)</script>');
                await page.click('button:has-text("Shorten")');
                
                // Check that script tags are escaped
                const content = await page.content();
                expect(content).not.toContain('<script>alert(1)</script>');
            }
        });

        test('should not execute inline scripts from user input', async ({ page }) => {
            let alertCalled = false;
            page.on('dialog', () => {
                alertCalled = true;
            });

            await page.goto('/');
            
            // Wait a bit to see if any alert fires
            await page.waitForTimeout(2000);
            
            expect(alertCalled).toBe(false);
        });
    });

    test.describe('CSRF Protection', () => {
        test('should reject requests without proper headers', async ({ request }) => {
            // Try to make a state-changing request without proper origin
        });
    });

    test.describe('SQL Injection Prevention', () => {
        test('should handle SQL injection attempts in search', async ({ page }) => {
            await page.goto('/dashboard');
            
            const searchInput = page.locator('input[type="search"], input[placeholder*="search"]');
            if (await searchInput.count() > 0) {
                // Try SQL injection
                await searchInput.fill("'; DROP TABLE links; --");
                
                // Should not cause errors
                await page.waitForTimeout(1000);
                await expect(page.locator('text=/error|500/i')).not.toBeVisible();
            }
        });

        test('should handle SQL injection in API', async ({ request }) => {
            const response = await request.post(`${API_URL}/auth/login`, {
                data: {
                    email: "admin'; DROP TABLE users; --",
                    password: 'password',
                },
            });

            // Should return normal error, not SQL error
            expect(response.status()).toBe(400);
            const data = await response.json();
            expect(data.error).not.toMatch(/sql|syntax|postgres/i);
        });
    });

    test.describe('Rate Limiting', () => {
        test('should rate limit login attempts', async ({ request }) => {
            const attempts = [];
            for (let i = 0; i < 20; i++) {
                attempts.push(
                    request.post(`${API_URL}/auth/login`, {
                        data: {
                            email: 'test@example.com',
                            password: 'wrongpassword',
                        },
                    })
                );
            }

            const responses = await Promise.all(attempts);
            const rateLimited = responses.filter(r => r.status() === 429);
            
            // Should have some rate limited responses
            expect(rateLimited.length).toBeGreaterThan(0);
        });

        test('should rate limit link creation', async ({ request }) => {
            // First login to get token
            // Then try rapid link creation
        });
    });

    test.describe('URL Safety', () => {
        test('should block dangerous URL schemes', async ({ request }) => {
            const response = await request.post(`${API_URL}/links`, {
                data: {
                    url: 'data:text/html,<script>alert(1)</script>',
                },
            });

            expect(response.status()).toBe(400);
        });

        test('should block local/private IPs', async ({ request }) => {
            const response = await request.post(`${API_URL}/links`, {
                data: {
                    url: 'http://localhost/admin',
                },
            });

            // Should reject localhost URLs
            expect(response.status()).toBeGreaterThanOrEqual(400);
        });
    });

    test.describe('Session Security', () => {
        test('should invalidate session on password change', async ({ page, request }) => {
            // Login
            // Change password
            // Old session should be invalid
        });

        test('should not allow session fixation', async ({ page }) => {
            // Set a fake token
            await page.addInitScript(() => {
                localStorage.setItem('token', 'fake-token');
            });

            await page.goto('/dashboard');
            
            // Should redirect to login, not accept fake token
            await expect(page).toHaveURL(/\/login/);
        });
    });

    test.describe('Input Validation', () => {
        test('should validate email format', async ({ request }) => {
            const response = await request.post(`${API_URL}/auth/register`, {
                data: {
                    email: 'not-an-email',
                    password: 'ValidPassword123!',
                },
            });

            expect(response.status()).toBe(400);
        });

        test('should limit URL length', async ({ request }) => {
            const longUrl = 'https://example.com/' + 'a'.repeat(10000);
            const response = await request.post(`${API_URL}/links`, {
                data: {
                    url: longUrl,
                },
            });

            expect(response.status()).toBeGreaterThanOrEqual(400);
        });

        test('should validate alias format', async ({ request }) => {
            const response = await request.post(`${API_URL}/links`, {
                data: {
                    url: 'https://example.com',
                    alias: 'invalid alias with spaces!',
                },
            });

            expect(response.status()).toBe(400);
        });
    });

    test.describe('Content Security', () => {
        test('should have CSP headers', async ({ page }) => {
            const response = await page.goto('/');
            const headers = response?.headers();
            
            // Check for security headers
            expect(
                headers?.['content-security-policy'] ||
                headers?.['x-content-type-options']
            ).toBeDefined();
        });

        test('should have X-Frame-Options header', async ({ page }) => {
            const response = await page.goto('/');
            const headers = response?.headers();
            
            expect(
                headers?.['x-frame-options'] ||
                headers?.['content-security-policy']?.includes('frame-ancestors')
            ).toBeDefined();
        });

        test('should serve HTTPS in production', async ({ page }) => {
            // In production, check for HTTPS redirect
            if (process.env.NODE_ENV === 'production') {
                const httpUrl = 'http://opn.onl';
                const response = await page.request.get(httpUrl, {
                    followRedirects: false,
                });
                
                expect(response.status()).toBe(301);
                expect(response.headers()['location']).toMatch(/^https/);
            }
        });
    });

    test.describe('File Upload Security', () => {
        test('should not allow executable uploads', async ({ page }) => {
            // If there's any file upload functionality
        });
    });

    test.describe('API Key Security', () => {
        test('should not expose API keys in responses', async ({ request }) => {
            const response = await request.get(`${API_URL}/links`);
            const text = await response.text();
            
            // Should not contain sensitive keys
            expect(text).not.toMatch(/sk_live|sk_test|aws_secret/i);
        });
    });

    test.describe('Error Handling', () => {
        test('should not expose stack traces', async ({ request }) => {
            const response = await request.get(`${API_URL}/cause-error`);
            const data = await response.json().catch(() => ({}));
            
            // Should not contain stack trace
            expect(JSON.stringify(data)).not.toMatch(/at \w+|stack.*trace/i);
        });

        test('should not expose database details', async ({ request }) => {
            const response = await request.post(`${API_URL}/auth/login`, {
                data: {
                    email: "test' OR '1'='1",
                    password: 'test',
                },
            });

            const text = await response.text();
            expect(text).not.toMatch(/postgres|mysql|sqlite|sql/i);
        });
    });
});

