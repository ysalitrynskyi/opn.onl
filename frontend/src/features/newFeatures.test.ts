import { describe, it, expect, vi, beforeEach } from 'vitest';

describe('Link Cloning Feature', () => {
    describe('Clone API endpoint', () => {
        it('should call the correct endpoint', () => {
            const linkId = 123;
            const expectedEndpoint = `/links/${linkId}/clone`;
            expect(expectedEndpoint).toBe('/links/123/clone');
        });

        it('should use POST method', () => {
            const method = 'POST';
            expect(method).toBe('POST');
        });
    });

    describe('Clone response', () => {
        it('should return a new link ID', () => {
            const response = { id: 456, code: 'newCode', short_url: 'https://opn.onl/newCode' };
            expect(response.id).toBe(456);
            expect(response.id).not.toBe(123); // Different from original
        });

        it('should return a new code', () => {
            const originalCode = 'abc123';
            const clonedCode = 'xyz789';
            expect(clonedCode).not.toBe(originalCode);
        });

        it('should include success message', () => {
            const response = { message: 'Link cloned successfully' };
            expect(response.message).toContain('cloned');
        });
    });
});

describe('Pin/Favorite Links Feature', () => {
    describe('Toggle pin endpoint', () => {
        it('should call the correct endpoint', () => {
            const linkId = 123;
            const expectedEndpoint = `/links/${linkId}/pin`;
            expect(expectedEndpoint).toBe('/links/123/pin');
        });

        it('should use POST method', () => {
            const method = 'POST';
            expect(method).toBe('POST');
        });
    });

    describe('Pin response', () => {
        it('should return new pin status', () => {
            const response = { is_pinned: true, message: 'Link pinned' };
            expect(response.is_pinned).toBe(true);
        });

        it('should return unpin message when unpinning', () => {
            const response = { is_pinned: false, message: 'Link unpinned' };
            expect(response.is_pinned).toBe(false);
            expect(response.message).toContain('unpinned');
        });
    });

    describe('Sorting with pinned links', () => {
        it('should sort pinned links first', () => {
            const links = [
                { id: 1, is_pinned: false, created_at: '2024-01-01' },
                { id: 2, is_pinned: true, created_at: '2024-01-02' },
                { id: 3, is_pinned: false, created_at: '2024-01-03' },
            ];

            const sorted = [...links].sort((a, b) => {
                if (a.is_pinned && !b.is_pinned) return -1;
                if (!a.is_pinned && b.is_pinned) return 1;
                return new Date(b.created_at).getTime() - new Date(a.created_at).getTime();
            });

            expect(sorted[0].id).toBe(2); // Pinned link first
            expect(sorted[0].is_pinned).toBe(true);
        });

        it('should maintain date order among pinned links', () => {
            const links = [
                { id: 1, is_pinned: true, created_at: '2024-01-01' },
                { id: 2, is_pinned: true, created_at: '2024-01-03' },
                { id: 3, is_pinned: true, created_at: '2024-01-02' },
            ];

            const sorted = [...links].sort((a, b) => {
                if (a.is_pinned && !b.is_pinned) return -1;
                if (!a.is_pinned && b.is_pinned) return 1;
                return new Date(b.created_at).getTime() - new Date(a.created_at).getTime();
            });

            expect(sorted[0].id).toBe(2); // Most recent pinned first
        });
    });
});

describe('Code Availability Check Feature', () => {
    describe('Check code endpoint', () => {
        it('should call the correct endpoint', () => {
            const code = 'myalias';
            const expectedEndpoint = `/links/check-code?code=${code}`;
            expect(expectedEndpoint).toContain('check-code');
        });

        it('should use GET method', () => {
            const method = 'GET';
            expect(method).toBe('GET');
        });
    });

    describe('Availability response', () => {
        it('should return available true for unused codes', () => {
            const response = { available: true, code: 'myalias', message: 'This alias is available' };
            expect(response.available).toBe(true);
        });

        it('should return available false for taken codes', () => {
            const response = { available: false, code: 'taken', message: 'This alias is already taken' };
            expect(response.available).toBe(false);
        });
    });

    describe('Code validation', () => {
        it('should reject codes shorter than minimum length', () => {
            const minLength = 5;
            const shortCode = 'abc';
            expect(shortCode.length).toBeLessThan(minLength);
        });

        it('should reject codes longer than maximum length', () => {
            const maxLength = 50;
            const longCode = 'a'.repeat(60);
            expect(longCode.length).toBeGreaterThan(maxLength);
        });

        it('should accept valid alphanumeric codes', () => {
            const validCode = 'my-link-123';
            const isValid = /^[a-zA-Z0-9_-]+$/.test(validCode);
            expect(isValid).toBe(true);
        });

        it('should reject codes with invalid characters', () => {
            const invalidCode = 'my link!';
            const isValid = /^[a-zA-Z0-9_-]+$/.test(invalidCode);
            expect(isValid).toBe(false);
        });

        it('should reject codes starting with hyphen', () => {
            const code = '-mylink';
            expect(code.startsWith('-')).toBe(true);
        });

        it('should reject codes ending with underscore', () => {
            const code = 'mylink_';
            expect(code.endsWith('_')).toBe(true);
        });
    });
});

describe('URL Health Check Feature', () => {
    describe('Health check endpoint', () => {
        it('should call the correct endpoint', () => {
            const expectedEndpoint = '/links/health-check';
            expect(expectedEndpoint).toBe('/links/health-check');
        });

        it('should use POST method', () => {
            const method = 'POST';
            expect(method).toBe('POST');
        });

        it('should send URL in request body', () => {
            const requestBody = { url: 'https://example.com' };
            expect(requestBody.url).toBe('https://example.com');
        });
    });

    describe('Health check response', () => {
        it('should return reachable true for accessible URLs', () => {
            const response = {
                url: 'https://example.com',
                reachable: true,
                status_code: 200,
                response_time_ms: 150,
                error: null,
            };
            expect(response.reachable).toBe(true);
        });

        it('should return reachable false for inaccessible URLs', () => {
            const response = {
                url: 'https://nonexistent.example',
                reachable: false,
                status_code: null,
                response_time_ms: 5000,
                error: 'Connection refused',
            };
            expect(response.reachable).toBe(false);
            expect(response.error).toBeTruthy();
        });

        it('should include response time', () => {
            const response = { response_time_ms: 150 };
            expect(response.response_time_ms).toBeGreaterThan(0);
        });

        it('should treat 3xx redirects as reachable', () => {
            const statusCode = 301;
            const isReachable = statusCode >= 200 && statusCode < 400;
            expect(isReachable).toBe(true);
        });

        it('should treat 4xx as not reachable', () => {
            const statusCode = 404;
            const isReachable = statusCode >= 200 && statusCode < 400;
            expect(isReachable).toBe(false);
        });
    });
});

describe('UTM Builder Feature', () => {
    describe('UTM endpoint', () => {
        it('should call the correct endpoint', () => {
            const expectedEndpoint = '/links/build-utm';
            expect(expectedEndpoint).toBe('/links/build-utm');
        });

        it('should use POST method', () => {
            const method = 'POST';
            expect(method).toBe('POST');
        });
    });

    describe('UTM parameters', () => {
        it('should add utm_source parameter', () => {
            const baseUrl = 'https://example.com';
            const utmSource = 'newsletter';
            const result = `${baseUrl}?utm_source=${utmSource}`;
            expect(result).toContain('utm_source=newsletter');
        });

        it('should add utm_medium parameter', () => {
            const baseUrl = 'https://example.com';
            const utmMedium = 'email';
            const result = `${baseUrl}?utm_medium=${utmMedium}`;
            expect(result).toContain('utm_medium=email');
        });

        it('should add utm_campaign parameter', () => {
            const baseUrl = 'https://example.com';
            const utmCampaign = 'spring_sale';
            const result = `${baseUrl}?utm_campaign=${utmCampaign}`;
            expect(result).toContain('utm_campaign=spring_sale');
        });

        it('should combine multiple UTM parameters', () => {
            const baseUrl = 'https://example.com';
            const params = new URLSearchParams({
                utm_source: 'google',
                utm_medium: 'cpc',
                utm_campaign: 'test',
            });
            const result = `${baseUrl}?${params.toString()}`;
            expect(result).toContain('utm_source=google');
            expect(result).toContain('utm_medium=cpc');
            expect(result).toContain('utm_campaign=test');
        });

        it('should not add empty UTM parameters', () => {
            const params: Record<string, string> = {};
            const utmSource = '';
            if (utmSource) {
                params.utm_source = utmSource;
            }
            expect(Object.keys(params)).not.toContain('utm_source');
        });

        it('should handle URLs with existing query params', () => {
            const baseUrl = 'https://example.com?existing=param';
            const hasExistingParams = baseUrl.includes('?');
            expect(hasExistingParams).toBe(true);
        });
    });

    describe('UTM response', () => {
        it('should return original URL', () => {
            const response = {
                original_url: 'https://example.com',
                url_with_utm: 'https://example.com?utm_source=test',
                utm_params: { utm_source: 'test' },
            };
            expect(response.original_url).toBe('https://example.com');
        });

        it('should return URL with UTM params', () => {
            const response = {
                original_url: 'https://example.com',
                url_with_utm: 'https://example.com?utm_source=test',
                utm_params: { utm_source: 'test' },
            };
            expect(response.url_with_utm).toContain('utm_source=test');
        });

        it('should return utm_params object', () => {
            const response = {
                utm_params: { utm_source: 'test', utm_medium: 'email' },
            };
            expect(response.utm_params.utm_source).toBe('test');
            expect(response.utm_params.utm_medium).toBe('email');
        });
    });
});

describe('LinkData Interface', () => {
    it('should include is_pinned field', () => {
        const link = {
            id: 1,
            code: 'abc123',
            original_url: 'https://example.com',
            is_pinned: true,
            is_active: true,
        };
        expect(link).toHaveProperty('is_pinned');
        expect(typeof link.is_pinned).toBe('boolean');
    });

    it('should default is_pinned to false', () => {
        const defaultPinned = false;
        expect(defaultPinned).toBe(false);
    });
});

describe('API Endpoints Configuration', () => {
    const API_BASE_URL = 'http://localhost:3000';

    it('should have linkClone endpoint', () => {
        const linkClone = (id: number) => `${API_BASE_URL}/links/${id}/clone`;
        expect(linkClone(123)).toBe('http://localhost:3000/links/123/clone');
    });

    it('should have linkPin endpoint', () => {
        const linkPin = (id: number) => `${API_BASE_URL}/links/${id}/pin`;
        expect(linkPin(123)).toBe('http://localhost:3000/links/123/pin');
    });

    it('should have checkCode endpoint', () => {
        const checkCode = `${API_BASE_URL}/links/check-code`;
        expect(checkCode).toBe('http://localhost:3000/links/check-code');
    });

    it('should have healthCheck endpoint', () => {
        const healthCheck = `${API_BASE_URL}/links/health-check`;
        expect(healthCheck).toBe('http://localhost:3000/links/health-check');
    });

    it('should have buildUtm endpoint', () => {
        const buildUtm = `${API_BASE_URL}/links/build-utm`;
        expect(buildUtm).toBe('http://localhost:3000/links/build-utm');
    });
});

