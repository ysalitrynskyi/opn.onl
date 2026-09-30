import { randomUUID } from 'node:crypto';
import { expect, type APIRequestContext, type Page } from '@playwright/test';

/**
 * Shared helpers for specs that run against the real backend.
 *
 * The backend must be started with TRUST_PROXY_HEADERS=true (CI does this).
 * Every helper request then carries its own CF-Connecting-IP, so fixture setup
 * never spends the per-IP auth rate-limit budget that the specs under test
 * rely on. Browser traffic from `page` is not affected.
 */

/** Backend origin. No path prefix: routes are /auth/*, /links/*, /admin/*. */
export const API_URL = (process.env.E2E_API_URL || 'http://localhost:3000').replace(/\/$/, '');

/** Frontend origin the specs navigate to. */
export const WEB_URL = (
    process.env.PLAYWRIGHT_BASE_URL || `http://localhost:${process.env.E2E_WEB_PORT || '5173'}`
).replace(/\/$/, '');

/** Meets the backend's only password rule (length >= 8). */
export const TEST_PASSWORD = 'E2e-Password-123!';

/** A fresh, never-used client address for one request. */
export function clientIpHeader(): Record<string, string> {
    const b = randomUUID().replace(/-/g, '');
    const octet = (i: number) => parseInt(b.slice(i * 2, i * 2 + 2), 16);
    return { 'CF-Connecting-IP': `10.${octet(0)}.${octet(1)}.${octet(2) || 1}` };
}

/** A unique address that passes the backend's email validation and domain policy. */
export function uniqueEmail(prefix = 'e2e'): string {
    return `${prefix}-${randomUUID()}@users.opn.onl`;
}

export interface TestUser {
    email: string;
    password: string;
    token: string;
    userId: number;
    isAdmin: boolean;
}

/** The admin created by global-setup (first user on the fresh database). */
export function adminToken(): string {
    const token = process.env.E2E_ADMIN_TOKEN;
    if (!token) throw new Error('E2E_ADMIN_TOKEN is not set; is globalSetup configured?');
    return token;
}

/**
 * Register a new user through the real API. Verified by default, because the
 * backend refuses to create links for an unverified account and there is no
 * SMTP in e2e: the admin verifies the address through
 * POST /admin/users/:id/verify-email instead.
 */
export async function createUser(
    request: APIRequestContext,
    opts: { verified?: boolean; email?: string; password?: string } = {},
): Promise<TestUser> {
    const email = opts.email ?? uniqueEmail();
    const password = opts.password ?? TEST_PASSWORD;
    const res = await request.post(`${API_URL}/auth/register`, {
        data: { email, password },
        headers: clientIpHeader(),
    });
    expect(res.status(), `register ${email}: ${await res.text()}`).toBe(201);
    const body = await res.json();
    const user: TestUser = {
        email,
        password,
        token: body.token,
        userId: body.user_id,
        isAdmin: body.is_admin,
    };
    if (opts.verified ?? true) {
        await verifyEmail(request, user.userId);
    }
    return user;
}

export async function verifyEmail(request: APIRequestContext, userId: number): Promise<void> {
    const res = await request.post(`${API_URL}/admin/users/${userId}/verify-email`, {
        headers: { Authorization: `Bearer ${adminToken()}`, ...clientIpHeader() },
    });
    expect(res.status(), `verify user ${userId}: ${await res.text()}`).toBe(200);
}

/** Authenticated JSON call as `token`. Returns the raw response for status assertions. */
export async function api(
    request: APIRequestContext,
    method: 'get' | 'post' | 'put' | 'patch' | 'delete',
    path: string,
    token?: string,
    data?: unknown,
) {
    return request[method](`${API_URL}${path}`, {
        headers: {
            ...(token ? { Authorization: `Bearer ${token}` } : {}),
            ...clientIpHeader(),
        },
        ...(data === undefined ? {} : { data }),
    });
}

/** Create a link as `token` and return the created row. */
export async function createLink(
    request: APIRequestContext,
    token: string,
    data: Record<string, unknown> = {},
) {
    const res = await api(request, 'post', '/links', token, {
        original_url: 'https://www.iana.org/help/example-domains',
        ...data,
    });
    expect(res.status(), `create link: ${await res.text()}`).toBe(201);
    return res.json();
}

/**
 * Start `page` signed in as `user`, the same way Login.tsx leaves the browser:
 * the JWT and the admin flag in localStorage. Call before the first goto.
 */
export async function signIn(page: Page, user: Pick<TestUser, 'token' | 'isAdmin'>): Promise<void> {
    await page.addInitScript(
        ([token, isAdmin]) => {
            localStorage.setItem('token', token);
            localStorage.setItem('is_admin', isAdmin);
        },
        [user.token, user.isAdmin ? 'true' : 'false'] as const,
    );
}
