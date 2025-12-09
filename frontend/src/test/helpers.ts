import { vi } from 'vitest';

// ============= Mock Data =============

export const mockUser = {
    id: 1,
    email: 'test@example.com',
    token: 'mock-jwt-token',
};

export const mockLink = {
    id: 1,
    code: 'abc123',
    original_url: 'https://example.com/long-url',
    click_count: 42,
    created_at: '2024-01-15T10:30:00Z',
    expires_at: null,
    has_password: false,
    notes: null,
    folder_id: null,
    org_id: null,
    starts_at: null,
    max_clicks: null,
    is_active: true,
    tags: [],
};

export const mockLinkWithTags = {
    ...mockLink,
    id: 2,
    code: 'xyz789',
    tags: [
        { id: 1, name: 'important', color: '#FF5733' },
        { id: 2, name: 'work', color: '#3498DB' },
    ],
};

export const mockLinkWithPassword = {
    ...mockLink,
    id: 3,
    code: 'pwd456',
    has_password: true,
};

export const mockLinkScheduled = {
    ...mockLink,
    id: 4,
    code: 'sch789',
    starts_at: new Date(Date.now() + 86400000).toISOString(), // Tomorrow
    is_active: false,
};

export const mockLinkExpired = {
    ...mockLink,
    id: 5,
    code: 'exp123',
    expires_at: new Date(Date.now() - 86400000).toISOString(), // Yesterday
    is_active: false,
};

export const mockLinkClickLimited = {
    ...mockLink,
    id: 6,
    code: 'lim456',
    max_clicks: 100,
    click_count: 100,
    is_active: false,
};

export const mockFolder = {
    id: 1,
    name: 'My Links',
    color: '#3498DB',
    user_id: 1,
    org_id: null,
    created_at: '2024-01-01T00:00:00Z',
    link_count: 5,
};

export const mockTag = {
    id: 1,
    name: 'important',
    color: '#FF5733',
    user_id: 1,
    org_id: null,
    created_at: '2024-01-01T00:00:00Z',
    link_count: 10,
};

export const mockOrganization = {
    id: 1,
    name: 'My Team',
    slug: 'my-team',
    owner_id: 1,
    created_at: '2024-01-01T00:00:00Z',
    member_count: 5,
    link_count: 100,
};

export const mockOrgMember = {
    id: 1,
    user_id: 2,
    email: 'member@example.com',
    role: 'editor',
    joined_at: '2024-01-15T00:00:00Z',
};

export const mockDashboardStats = {
    total_links: 50,
    total_clicks: 1250,
    active_links: 45,
    clicks_today: 42,
    clicks_this_week: 280,
    clicks_this_month: 850,
    top_links: [
        { id: 1, code: 'abc123', original_url: 'https://example.com', click_count: 150 },
        { id: 2, code: 'xyz789', original_url: 'https://test.com', click_count: 100 },
    ],
    clicks_by_day: [
        { date: '2024-01-15', count: 42 },
        { date: '2024-01-14', count: 38 },
        { date: '2024-01-13', count: 45 },
    ],
    top_countries: [
        { country: 'United States', count: 500, percentage: 40 },
        { country: 'United Kingdom', count: 250, percentage: 20 },
    ],
    top_browsers: [
        { browser: 'Chrome', count: 600, percentage: 48 },
        { browser: 'Firefox', count: 300, percentage: 24 },
    ],
};

export const mockLinkStats = {
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
        { country: 'Germany', count: 30, percentage: 20 },
    ],
    clicks_by_city: [
        { city: 'New York', country: 'United States', count: 25, percentage: 16.7 },
    ],
    clicks_by_device: [
        { device: 'Desktop', count: 90, percentage: 60 },
        { device: 'Mobile', count: 50, percentage: 33.3 },
    ],
    clicks_by_browser: [
        { browser: 'Chrome', count: 70, percentage: 46.7 },
        { browser: 'Firefox', count: 40, percentage: 26.7 },
    ],
    clicks_by_os: [
        { os: 'Windows', count: 60, percentage: 40 },
        { os: 'macOS', count: 45, percentage: 30 },
    ],
    clicks_by_referer: [
        { referer: 'google.com', count: 50, percentage: 33.3 },
        { referer: 'Direct', count: 40, percentage: 26.7 },
    ],
    recent_clicks: [
        {
            id: 1,
            timestamp: '2024-01-15T10:30:00Z',
            country: 'United States',
            city: 'New York',
            device: 'Desktop',
            browser: 'Chrome',
            os: 'Windows',
            referer: 'google.com',
        },
    ],
    geo_data: [
        { latitude: 40.7128, longitude: -74.006, city: 'New York', country: 'United States', count: 25 },
    ],
};

// ============= Mock API Responses =============

export const mockApiSuccess = <T>(data: T) => ({
    ok: true,
    status: 200,
    json: () => Promise.resolve(data),
    headers: new Headers(),
});

export const mockApiError = (error: string, status = 400) => ({
    ok: false,
    status,
    json: () => Promise.resolve({ error }),
    headers: new Headers(),
});

export const mockApi429 = (retryAfter = '60') => ({
    ok: false,
    status: 429,
    json: () => Promise.resolve({ error: 'Too many requests' }),
    headers: new Headers({ 'Retry-After': retryAfter }),
});

// ============= Mock Fetch Setup =============

export const setupMockFetch = (responses: Record<string, any>) => {
    global.fetch = vi.fn().mockImplementation((url: string) => {
        for (const [pattern, response] of Object.entries(responses)) {
            if (url.includes(pattern)) {
                return Promise.resolve(response);
            }
        }
        return Promise.resolve(mockApiError('Not found', 404));
    });
};

// ============= LocalStorage Helpers =============

export const setLoggedIn = (token = mockUser.token) => {
    localStorage.setItem('token', token);
};

export const setLoggedOut = () => {
    localStorage.removeItem('token');
};

// ============= Wait Helpers =============

export const waitFor = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

export const waitForElement = async (selector: string, timeout = 5000) => {
    const start = Date.now();
    while (Date.now() - start < timeout) {
        const element = document.querySelector(selector);
        if (element) return element;
        await waitFor(50);
    }
    throw new Error(`Element ${selector} not found within ${timeout}ms`);
};

// ============= Date Helpers =============

export const createFutureDate = (days: number) => {
    const date = new Date();
    date.setDate(date.getDate() + days);
    return date.toISOString();
};

export const createPastDate = (days: number) => {
    const date = new Date();
    date.setDate(date.getDate() - days);
    return date.toISOString();
};

// ============= Form Helpers =============

export const fillInput = (element: HTMLInputElement, value: string) => {
    element.value = value;
    element.dispatchEvent(new Event('input', { bubbles: true }));
    element.dispatchEvent(new Event('change', { bubbles: true }));
};

export const clickButton = (element: HTMLButtonElement) => {
    element.click();
    element.dispatchEvent(new Event('click', { bubbles: true }));
};





