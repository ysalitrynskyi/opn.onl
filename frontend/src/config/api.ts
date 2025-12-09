// API Configuration
// In development, this uses localhost. In production, set VITE_API_URL environment variable.

export const API_BASE_URL = import.meta.env.VITE_API_URL || 'http://localhost:3000';

export const API_ENDPOINTS = {
    // Base URL for constructing custom endpoints
    base: API_BASE_URL,
    
    // Auth
    register: `${API_BASE_URL}/auth/register`,
    login: `${API_BASE_URL}/auth/login`,
    verifyEmail: `${API_BASE_URL}/auth/verify-email`,
    resendVerification: `${API_BASE_URL}/auth/resend-verification`,
    forgotPassword: `${API_BASE_URL}/auth/forgot-password`,
    resetPassword: `${API_BASE_URL}/auth/reset-password`,
    
    // User
    appSettings: `${API_BASE_URL}/auth/settings`,
    userProfile: `${API_BASE_URL}/auth/me`,
    updateProfile: `${API_BASE_URL}/auth/profile`,
    
    // Passkeys
    passkeys: `${API_BASE_URL}/auth/passkeys`,
    passkeyRegisterStart: `${API_BASE_URL}/auth/passkey/register/start`,
    passkeyRegisterFinish: `${API_BASE_URL}/auth/passkey/register/finish`,
    passkeyLoginStart: `${API_BASE_URL}/auth/passkey/login/start`,
    passkeyLoginFinish: `${API_BASE_URL}/auth/passkey/login/finish`,
    passkeyDelete: `${API_BASE_URL}/auth/passkey/delete`,
    passkeyRename: `${API_BASE_URL}/auth/passkey/rename`,
    
    // Links
    links: `${API_BASE_URL}/links`,
    bulkLinks: `${API_BASE_URL}/links/bulk`,
    bulkDeleteLinks: `${API_BASE_URL}/links/bulk/delete`,
    bulkUpdateLinks: `${API_BASE_URL}/links/bulk/update`,
    exportLinks: `${API_BASE_URL}/links/export`,
    checkCode: `${API_BASE_URL}/links/check-code`,
    healthCheck: `${API_BASE_URL}/links/health-check`,
    buildUtm: `${API_BASE_URL}/links/build-utm`,
    sparklines: `${API_BASE_URL}/links/sparklines`,
    previewMetadata: `${API_BASE_URL}/links/preview-metadata`,
    linkStats: (id: number) => `${API_BASE_URL}/links/${id}/stats`,
    linkQr: (id: number) => `${API_BASE_URL}/links/${id}/qr`,
    linkDelete: (id: number) => `${API_BASE_URL}/links/${id}`,
    linkUpdate: (id: number) => `${API_BASE_URL}/links/${id}`,
    linkClone: (id: number) => `${API_BASE_URL}/links/${id}/clone`,
    linkPin: (id: number) => `${API_BASE_URL}/links/${id}/pin`,
    linkTags: (id: number) => `${API_BASE_URL}/links/${id}/tags`,
    linkRealtimeClicks: (id: number) => `${API_BASE_URL}/links/${id}/clicks/realtime`,
    
    // Analytics
    dashboardStats: `${API_BASE_URL}/analytics/dashboard`,
    
    // Organizations
    orgs: `${API_BASE_URL}/orgs`,
    org: (id: number) => `${API_BASE_URL}/orgs/${id}`,
    orgMembers: (orgId: number) => `${API_BASE_URL}/orgs/${orgId}/members`,
    orgMember: (orgId: number, memberId: number) => `${API_BASE_URL}/orgs/${orgId}/members/${memberId}`,
    orgAudit: (orgId: number) => `${API_BASE_URL}/orgs/${orgId}/audit`,
    
    // Folders
    folders: `${API_BASE_URL}/folders`,
    folder: (id: number) => `${API_BASE_URL}/folders/${id}`,
    folderLinks: (id: number) => `${API_BASE_URL}/folders/${id}/links`,
    
    // Tags
    tags: `${API_BASE_URL}/tags`,
    tag: (id: number) => `${API_BASE_URL}/tags/${id}`,
    tagLinks: (id: number) => `${API_BASE_URL}/tags/${id}/links`,
    
    // Redirect (for password verification)
    verifyPassword: (code: string) => `${API_BASE_URL}/${code}/verify`,
    
    // Link preview (for checking if link exists)
    preview: (code: string) => `${API_BASE_URL}/${code}/preview`,
    
    // Change password
    changePassword: `${API_BASE_URL}/auth/change-password`,
    
    // Delete account
    deleteAccount: `${API_BASE_URL}/auth/delete-account`,
    
    // Contact
    contact: `${API_BASE_URL}/contact`,
    
    // Health
    health: `${API_BASE_URL}/health`,
    
    // API Docs
    swagger: `${API_BASE_URL}/swagger-ui`,
    openapi: `${API_BASE_URL}/api-docs/openapi.json`,
    
    // Admin
    adminStats: `${API_BASE_URL}/admin/stats`,
    adminUsers: `${API_BASE_URL}/admin/users`,
    adminUser: (id: number) => `${API_BASE_URL}/admin/users/${id}`,
    adminUserHardDelete: (id: number) => `${API_BASE_URL}/admin/users/${id}/hard`,
    adminUserRestore: (id: number) => `${API_BASE_URL}/admin/users/${id}/restore`,
    adminUserMakeAdmin: (id: number) => `${API_BASE_URL}/admin/users/${id}/make-admin`,
    adminUserRemoveAdmin: (id: number) => `${API_BASE_URL}/admin/users/${id}/remove-admin`,
    adminBlockedLinks: `${API_BASE_URL}/admin/blocked/links`,
    adminBlockedLink: (id: number) => `${API_BASE_URL}/admin/blocked/links/${id}`,
    adminBlockedDomains: `${API_BASE_URL}/admin/blocked/domains`,
    adminBlockedDomain: (id: number) => `${API_BASE_URL}/admin/blocked/domains/${id}`,
    adminBackup: `${API_BASE_URL}/admin/backup`,
    adminBackupCleanup: (keep: number) => `${API_BASE_URL}/admin/backup/cleanup/${keep}`,
};

// Helper to get auth headers
export const getAuthHeaders = (): HeadersInit => {
    const token = localStorage.getItem('token');
    return {
        'Content-Type': 'application/json',
        ...(token ? { 'Authorization': `Bearer ${token}` } : {}),
    };
};

// Handle unauthorized - clear token and redirect to login
const handleUnauthorized = () => {
    localStorage.removeItem('token');
    localStorage.removeItem('is_admin');
    // Redirect to login page
    window.location.href = '/login';
};

// Authenticated fetch wrapper - handles 401 automatically
export async function authFetch(
    url: string,
    options: RequestInit = {}
): Promise<Response> {
    const response = await fetch(url, {
        ...options,
        headers: {
            ...getAuthHeaders(),
            ...options.headers,
        },
    });

    // Handle unauthorized - logout user
    if (response.status === 401) {
        handleUnauthorized();
        throw new Error('Session expired. Please log in again.');
    }

    return response;
}

// Helper for API calls with error handling
export async function apiCall<T>(
    url: string,
    options: RequestInit = {}
): Promise<{ data?: T; error?: string }> {
    try {
        const response = await fetch(url, {
            ...options,
            headers: {
                ...getAuthHeaders(),
                ...options.headers,
            },
        });

        // Handle unauthorized - logout user
        if (response.status === 401) {
            handleUnauthorized();
            return { error: 'Session expired. Please log in again.' };
        }

        // Handle rate limiting
        if (response.status === 429) {
            const retryAfter = response.headers.get('Retry-After');
            return { 
                error: `Too many requests. Please try again in ${retryAfter || '60'} seconds.` 
            };
        }

        const data = await response.json().catch(() => null);

        if (!response.ok) {
            return { error: data?.error || `Request failed with status ${response.status}` };
        }

        return { data };
    } catch (error) {
        return { error: error instanceof Error ? error.message : 'Network error' };
    }
}

// Types for API responses
export interface Link {
    id: number;
    code: string;
    original_url: string;
    click_count: number;
    created_at: string;
    expires_at?: string;
    has_password: boolean;
    notes?: string;
    folder_id?: number;
    org_id?: number;
    starts_at?: string;
    max_clicks?: number;
    is_active: boolean;
    is_pinned: boolean;
    tags: Tag[];
}

export interface Tag {
    id: number;
    name: string;
    color?: string;
}

export interface Folder {
    id: number;
    name: string;
    color?: string;
    user_id?: number;
    org_id?: number;
    created_at: string;
    link_count: number;
}

export interface Organization {
    id: number;
    name: string;
    slug: string;
    owner_id: number;
    created_at: string;
    member_count: number;
    link_count: number;
}

export interface OrgMember {
    id: number;
    user_id: number;
    email: string;
    role: string;
    joined_at: string;
}

export interface DashboardStats {
    total_links: number;
    total_clicks: number;
    active_links: number;
    clicks_today: number;
    clicks_this_week: number;
    clicks_this_month: number;
    top_links: TopLink[];
    clicks_by_day: DayStats[];
    top_countries: CountryStats[];
    top_browsers: BrowserStats[];
}

export interface TopLink {
    id: number;
    code: string;
    original_url: string;
    click_count: number;
}

export interface DayStats {
    date: string;
    count: number;
}

export interface CountryStats {
    country: string;
    count: number;
    percentage: number;
}

export interface BrowserStats {
    browser: string;
    count: number;
    percentage: number;
}

export interface LinkStats {
    link_id: number;
    code: string;
    original_url: string;
    total_clicks: number;
    unique_visitors: number;
    clicks_by_day: DayStats[];
    clicks_by_country: CountryStats[];
    clicks_by_city: CityStats[];
    clicks_by_device: DeviceStats[];
    clicks_by_browser: BrowserStats[];
    clicks_by_os: OsStats[];
    clicks_by_referer: RefererStats[];
    recent_clicks: RecentClick[];
    geo_data: GeoPoint[];
}

export interface CityStats {
    city: string;
    country?: string;
    count: number;
    percentage: number;
}

export interface DeviceStats {
    device: string;
    count: number;
    percentage: number;
}

export interface OsStats {
    os: string;
    count: number;
    percentage: number;
}

export interface RefererStats {
    referer: string;
    count: number;
    percentage: number;
}

export interface RecentClick {
    id: number;
    timestamp: string;
    country?: string;
    city?: string;
    device?: string;
    browser?: string;
    os?: string;
    referer?: string;
}

export interface GeoPoint {
    latitude: number;
    longitude: number;
    city?: string;
    country?: string;
    count: number;
}

export interface AuditLog {
    id: number;
    user_id?: number;
    user_email?: string;
    action: string;
    resource_type: string;
    resource_id?: number;
    details?: Record<string, any>;
    ip_address?: string;
    created_at: string;
}
