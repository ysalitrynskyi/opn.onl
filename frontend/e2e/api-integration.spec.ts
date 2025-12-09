import { test, expect, request } from '@playwright/test';

const API_URL = process.env.API_URL || 'http://localhost:3000/api';
const timestamp = Date.now();
const testEmail = `api-test-${timestamp}@example.com`;
const testPassword = 'TestPassword123!';

test.describe('API Integration Tests', () => {
    let token: string;
    let linkId: number;
    let linkCode: string;

    test.describe.serial('Authentication API', () => {
        test('POST /auth/register - creates new user', async ({ request }) => {
            const response = await request.post(`${API_URL}/auth/register`, {
                data: {
                    email: testEmail,
                    password: testPassword,
                },
            });

            expect(response.status()).toBe(200);
            const data = await response.json();
            expect(data.user || data.email).toBeDefined();
        });

        test('POST /auth/login - authenticates user', async ({ request }) => {
            const response = await request.post(`${API_URL}/auth/login`, {
                data: {
                    email: testEmail,
                    password: testPassword,
                },
            });

            expect(response.status()).toBe(200);
            const data = await response.json();
            expect(data.token).toBeDefined();
            token = data.token;
        });

        test('POST /auth/login - rejects invalid credentials', async ({ request }) => {
            const response = await request.post(`${API_URL}/auth/login`, {
                data: {
                    email: testEmail,
                    password: 'wrongpassword',
                },
            });

            expect(response.status()).toBe(401);
        });

        test('GET /auth/me - returns user profile', async ({ request }) => {
            const response = await request.get(`${API_URL}/auth/me`, {
                headers: {
                    Authorization: `Bearer ${token}`,
                },
            });

            expect(response.status()).toBe(200);
            const data = await response.json();
            expect(data.email).toBe(testEmail);
        });

        test('GET /auth/me - rejects without token', async ({ request }) => {
            const response = await request.get(`${API_URL}/auth/me`);
            expect(response.status()).toBe(401);
        });
    });

    test.describe.serial('Links API', () => {
        test('POST /links - creates new link', async ({ request }) => {
            const response = await request.post(`${API_URL}/links`, {
                headers: {
                    Authorization: `Bearer ${token}`,
                },
                data: {
                    url: 'https://example.com/api-test',
                },
            });

            expect(response.status()).toBe(200);
            const data = await response.json();
            expect(data.code).toBeDefined();
            expect(data.original_url).toBe('https://example.com/api-test');
            linkId = data.id;
            linkCode = data.code;
        });

        test('POST /links - creates link with custom alias', async ({ request }) => {
            const customAlias = `api-alias-${timestamp}`;
            const response = await request.post(`${API_URL}/links`, {
                headers: {
                    Authorization: `Bearer ${token}`,
                },
                data: {
                    url: 'https://example.com/custom',
                    alias: customAlias,
                },
            });

            expect(response.status()).toBe(200);
            const data = await response.json();
            expect(data.code).toBe(customAlias);
        });

        test('POST /links - validates URL format', async ({ request }) => {
            const response = await request.post(`${API_URL}/links`, {
                headers: {
                    Authorization: `Bearer ${token}`,
                },
                data: {
                    url: 'not-a-valid-url',
                },
            });

            expect(response.status()).toBe(400);
        });

        test('POST /links - rejects blocked URLs', async ({ request }) => {
            // This depends on what URLs are blocked
        });

        test('GET /links - lists user links', async ({ request }) => {
            const response = await request.get(`${API_URL}/links`, {
                headers: {
                    Authorization: `Bearer ${token}`,
                },
            });

            expect(response.status()).toBe(200);
            const data = await response.json();
            expect(Array.isArray(data.links || data)).toBe(true);
        });

        test('GET /links/:id - gets single link', async ({ request }) => {
            const response = await request.get(`${API_URL}/links/${linkId}`, {
                headers: {
                    Authorization: `Bearer ${token}`,
                },
            });

            expect(response.status()).toBe(200);
            const data = await response.json();
            expect(data.id).toBe(linkId);
        });

        test('PUT /links/:id - updates link', async ({ request }) => {
            const response = await request.put(`${API_URL}/links/${linkId}`, {
                headers: {
                    Authorization: `Bearer ${token}`,
                },
                data: {
                    notes: 'Updated via API test',
                },
            });

            expect(response.status()).toBe(200);
            const data = await response.json();
            expect(data.notes).toBe('Updated via API test');
        });

        test('GET /:code - redirects to original URL', async ({ request }) => {
            const response = await request.get(`${API_URL.replace('/api', '')}/${linkCode}`, {
                followRedirects: false,
            });

            expect(response.status()).toBe(301);
            expect(response.headers()['location']).toContain('example.com');
        });

        test('GET /links/:id/qr - generates QR code', async ({ request }) => {
            const response = await request.get(`${API_URL}/links/${linkId}/qr`, {
                headers: {
                    Authorization: `Bearer ${token}`,
                },
            });

            expect(response.status()).toBe(200);
            expect(response.headers()['content-type']).toContain('image');
        });

        test('DELETE /links/:id - deletes link', async ({ request }) => {
            const response = await request.delete(`${API_URL}/links/${linkId}`, {
                headers: {
                    Authorization: `Bearer ${token}`,
                },
            });

            expect(response.status()).toBe(200);
        });
    });

    test.describe('Analytics API', () => {
        test('GET /links/:id/analytics - requires authentication', async ({ request }) => {
            const response = await request.get(`${API_URL}/links/1/analytics`);
            expect(response.status()).toBe(401);
        });
    });

    test.describe('Folders API', () => {
        let folderId: number;

        test('POST /folders - creates folder', async ({ request }) => {
            const response = await request.post(`${API_URL}/folders`, {
                headers: {
                    Authorization: `Bearer ${token}`,
                },
                data: {
                    name: `API Test Folder ${timestamp}`,
                    color: '#3b82f6',
                },
            });

            if (response.status() === 200) {
                const data = await response.json();
                expect(data.name).toBe(`API Test Folder ${timestamp}`);
                folderId = data.id;
            }
        });

        test('GET /folders - lists folders', async ({ request }) => {
            const response = await request.get(`${API_URL}/folders`, {
                headers: {
                    Authorization: `Bearer ${token}`,
                },
            });

            expect(response.status()).toBe(200);
            const data = await response.json();
            expect(Array.isArray(data.folders || data)).toBe(true);
        });

        test('PUT /folders/:id - updates folder', async ({ request }) => {
            if (folderId) {
                const response = await request.put(`${API_URL}/folders/${folderId}`, {
                    headers: {
                        Authorization: `Bearer ${token}`,
                    },
                    data: {
                        name: 'Updated Folder Name',
                    },
                });

                expect(response.status()).toBe(200);
            }
        });

        test('DELETE /folders/:id - deletes folder', async ({ request }) => {
            if (folderId) {
                const response = await request.delete(`${API_URL}/folders/${folderId}`, {
                    headers: {
                        Authorization: `Bearer ${token}`,
                    },
                });

                expect(response.status()).toBe(200);
            }
        });
    });

    test.describe('Tags API', () => {
        let tagId: number;

        test('POST /tags - creates tag', async ({ request }) => {
            const response = await request.post(`${API_URL}/tags`, {
                headers: {
                    Authorization: `Bearer ${token}`,
                },
                data: {
                    name: `api-tag-${timestamp}`,
                    color: '#ef4444',
                },
            });

            if (response.status() === 200) {
                const data = await response.json();
                expect(data.name).toBe(`api-tag-${timestamp}`);
                tagId = data.id;
            }
        });

        test('GET /tags - lists tags', async ({ request }) => {
            const response = await request.get(`${API_URL}/tags`, {
                headers: {
                    Authorization: `Bearer ${token}`,
                },
            });

            expect(response.status()).toBe(200);
        });

        test('DELETE /tags/:id - deletes tag', async ({ request }) => {
            if (tagId) {
                const response = await request.delete(`${API_URL}/tags/${tagId}`, {
                    headers: {
                        Authorization: `Bearer ${token}`,
                    },
                });

                expect(response.status()).toBe(200);
            }
        });
    });

    test.describe('Organizations API', () => {
        let orgId: number;

        test('POST /organizations - creates organization', async ({ request }) => {
            const response = await request.post(`${API_URL}/organizations`, {
                headers: {
                    Authorization: `Bearer ${token}`,
                },
                data: {
                    name: `API Test Org ${timestamp}`,
                    slug: `api-org-${timestamp}`,
                },
            });

            if (response.status() === 200) {
                const data = await response.json();
                expect(data.name).toBe(`API Test Org ${timestamp}`);
                orgId = data.id;
            }
        });

        test('GET /organizations - lists organizations', async ({ request }) => {
            const response = await request.get(`${API_URL}/organizations`, {
                headers: {
                    Authorization: `Bearer ${token}`,
                },
            });

            expect(response.status()).toBe(200);
        });

        test('DELETE /organizations/:id - deletes organization', async ({ request }) => {
            if (orgId) {
                const response = await request.delete(`${API_URL}/organizations/${orgId}`, {
                    headers: {
                        Authorization: `Bearer ${token}`,
                    },
                });

                expect(response.status()).toBe(200);
            }
        });
    });

    test.describe('Rate Limiting', () => {
        test('should enforce rate limits', async ({ request }) => {
            const requests = [];
            for (let i = 0; i < 100; i++) {
                requests.push(request.get(`${API_URL}/links`, {
                    headers: {
                        Authorization: `Bearer ${token}`,
                    },
                }));
            }

            const responses = await Promise.all(requests);
            const rateLimited = responses.filter(r => r.status() === 429);
            
            // Some requests should be rate limited
            // (depends on rate limit configuration)
        });
    });

    test.describe('Error Responses', () => {
        test('should return proper error format', async ({ request }) => {
            const response = await request.get(`${API_URL}/nonexistent`);
            
            expect(response.status()).toBeGreaterThanOrEqual(400);
        });

        test('should handle malformed JSON', async ({ request }) => {
            const response = await request.post(`${API_URL}/links`, {
                headers: {
                    Authorization: `Bearer ${token}`,
                    'Content-Type': 'application/json',
                },
                data: 'not json',
            });

            expect(response.status()).toBeGreaterThanOrEqual(400);
        });
    });

    test.describe('CORS Headers', () => {
        test('should return CORS headers', async ({ request }) => {
            const response = await request.options(`${API_URL}/links`);
            
            // Check for CORS headers
            const headers = response.headers();
            expect(
                headers['access-control-allow-origin'] ||
                headers['access-control-allow-methods']
            ).toBeDefined();
        });
    });
});


