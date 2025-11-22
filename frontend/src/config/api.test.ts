import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { 
    API_BASE_URL, 
    API_ENDPOINTS, 
    getAuthHeaders, 
    apiCall,
} from './api';

describe('API Configuration', () => {
    describe('API_BASE_URL', () => {
        it('should have a default value', () => {
            expect(API_BASE_URL).toBeDefined();
            expect(typeof API_BASE_URL).toBe('string');
        });

        it('should start with http', () => {
            expect(API_BASE_URL.startsWith('http')).toBe(true);
        });
    });
});

describe('API_ENDPOINTS', () => {
    describe('Auth endpoints', () => {
        it('should have register endpoint', () => {
            expect(API_ENDPOINTS.register).toContain('/auth/register');
        });

        it('should have login endpoint', () => {
            expect(API_ENDPOINTS.login).toContain('/auth/login');
        });

        it('should have passkey endpoints', () => {
            expect(API_ENDPOINTS.passkeyRegisterStart).toContain('/auth/passkey/register/start');
            expect(API_ENDPOINTS.passkeyRegisterFinish).toContain('/auth/passkey/register/finish');
            expect(API_ENDPOINTS.passkeyLoginStart).toContain('/auth/passkey/login/start');
            expect(API_ENDPOINTS.passkeyLoginFinish).toContain('/auth/passkey/login/finish');
        });
    });

    describe('Link endpoints', () => {
        it('should have links endpoint', () => {
            expect(API_ENDPOINTS.links).toContain('/links');
        });

        it('should have bulk endpoints', () => {
            expect(API_ENDPOINTS.bulkLinks).toContain('/links/bulk');
            expect(API_ENDPOINTS.bulkDeleteLinks).toContain('/links/bulk/delete');
            expect(API_ENDPOINTS.bulkUpdateLinks).toContain('/links/bulk/update');
        });

        it('should have dynamic link endpoints', () => {
            expect(API_ENDPOINTS.linkStats(1)).toContain('/links/1/stats');
            expect(API_ENDPOINTS.linkQr(5)).toContain('/links/5/qr');
            expect(API_ENDPOINTS.linkDelete(10)).toContain('/links/10');
            expect(API_ENDPOINTS.linkUpdate(20)).toContain('/links/20');
            expect(API_ENDPOINTS.linkTags(3)).toContain('/links/3/tags');
        });

        it('should have export endpoint', () => {
            expect(API_ENDPOINTS.exportLinks).toContain('/links/export');
        });
    });

    describe('Analytics endpoints', () => {
        it('should have dashboard stats endpoint', () => {
            expect(API_ENDPOINTS.dashboardStats).toContain('/analytics/dashboard');
        });

        it('should have realtime clicks endpoint', () => {
            expect(API_ENDPOINTS.linkRealtimeClicks(1)).toContain('/links/1/clicks/realtime');
        });
    });

    describe('Organization endpoints', () => {
        it('should have orgs endpoint', () => {
            expect(API_ENDPOINTS.orgs).toContain('/orgs');
        });

        it('should have dynamic org endpoints', () => {
            expect(API_ENDPOINTS.org(1)).toContain('/orgs/1');
            expect(API_ENDPOINTS.orgMembers(2)).toContain('/orgs/2/members');
            expect(API_ENDPOINTS.orgMember(3, 4)).toContain('/orgs/3/members/4');
            expect(API_ENDPOINTS.orgAudit(5)).toContain('/orgs/5/audit');
        });
    });

    describe('Folder endpoints', () => {
        it('should have folders endpoint', () => {
            expect(API_ENDPOINTS.folders).toContain('/folders');
        });

        it('should have dynamic folder endpoints', () => {
            expect(API_ENDPOINTS.folder(1)).toContain('/folders/1');
            expect(API_ENDPOINTS.folderLinks(2)).toContain('/folders/2/links');
        });
    });

    describe('Tag endpoints', () => {
        it('should have tags endpoint', () => {
            expect(API_ENDPOINTS.tags).toContain('/tags');
        });

        it('should have dynamic tag endpoints', () => {
            expect(API_ENDPOINTS.tag(1)).toContain('/tags/1');
            expect(API_ENDPOINTS.tagLinks(2)).toContain('/tags/2/links');
        });
    });

    describe('Password verification endpoint', () => {
        it('should have verify password endpoint', () => {
            expect(API_ENDPOINTS.verifyPassword('abc123')).toContain('/abc123/verify');
        });
    });

    describe('Documentation endpoints', () => {
        it('should have swagger endpoint', () => {
            expect(API_ENDPOINTS.swagger).toContain('/swagger-ui');
        });

        it('should have openapi endpoint', () => {
            expect(API_ENDPOINTS.openapi).toContain('/api-docs/openapi.json');
        });
    });
});

describe('getAuthHeaders', () => {
    beforeEach(() => {
        localStorage.clear();
    });

    it('should return Content-Type header', () => {
        const headers = getAuthHeaders() as Record<string, string>;
        expect(headers['Content-Type']).toBe('application/json');
    });

    it('should not include Authorization when no token', () => {
        const headers = getAuthHeaders();
        expect(headers).not.toHaveProperty('Authorization');
    });

    it('should include Authorization when token exists', () => {
        localStorage.setItem('token', 'test-jwt-token');
        const headers = getAuthHeaders() as Record<string, string>;
        expect(headers['Authorization']).toBe('Bearer test-jwt-token');
    });
});

describe('apiCall', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        localStorage.clear();
    });

    afterEach(() => {
        vi.restoreAllMocks();
    });

    it('should make successful GET request', async () => {
        const mockData = { id: 1, name: 'Test' };
        global.fetch = vi.fn().mockResolvedValue({
            ok: true,
            json: () => Promise.resolve(mockData),
        });

        const result = await apiCall('https://api.test.com/data');

        expect(result.data).toEqual(mockData);
        expect(result.error).toBeUndefined();
    });

    it('should handle error response', async () => {
        global.fetch = vi.fn().mockResolvedValue({
            ok: false,
            status: 400,
            json: () => Promise.resolve({ error: 'Bad request' }),
        });

        const result = await apiCall('https://api.test.com/data');

        expect(result.data).toBeUndefined();
        expect(result.error).toBe('Bad request');
    });

    it('should handle rate limiting (429)', async () => {
        const mockHeaders = new Headers();
        mockHeaders.set('Retry-After', '30');
        
        global.fetch = vi.fn().mockResolvedValue({
            ok: false,
            status: 429,
            headers: mockHeaders,
            json: () => Promise.resolve({}),
        });

        const result = await apiCall('https://api.test.com/data');

        expect(result.error).toContain('Too many requests');
    });

    it('should handle network error', async () => {
        global.fetch = vi.fn().mockRejectedValue(new Error('Network error'));

        const result = await apiCall('https://api.test.com/data');

        expect(result.data).toBeUndefined();
        expect(result.error).toBe('Network error');
    });

    it('should include auth headers', async () => {
        localStorage.setItem('token', 'test-token');
        const mockFetch = vi.fn().mockResolvedValue({
            ok: true,
            json: () => Promise.resolve({}),
            headers: new Headers(),
        });
        global.fetch = mockFetch;

        await apiCall('https://api.test.com/data');

        expect(mockFetch).toHaveBeenCalled();
        const callArgs = mockFetch.mock.calls[0];
        expect(callArgs[0]).toBe('https://api.test.com/data');
        const headers = callArgs[1].headers;
        expect(headers).toHaveProperty('Authorization');
    });

    it('should handle POST request with body', async () => {
        const mockFetch = vi.fn().mockResolvedValue({
            ok: true,
            json: () => Promise.resolve({ success: true }),
            headers: new Headers(),
        });
        global.fetch = mockFetch;

        await apiCall('https://api.test.com/data', {
            method: 'POST',
            body: JSON.stringify({ test: 'data' }),
        });

        expect(mockFetch).toHaveBeenCalled();
        const callArgs = mockFetch.mock.calls[0];
        expect(callArgs[1].method).toBe('POST');
        expect(callArgs[1].body).toBe(JSON.stringify({ test: 'data' }));
    });
});
