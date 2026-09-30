import { request } from '@playwright/test';
import { API_URL, clientIpHeader, TEST_PASSWORD } from './api';

/**
 * Bootstrap the admin every real-backend spec leans on.
 *
 * The backend makes the first account registered on an empty database an
 * admin, so this needs a fresh database (CI's service container is one). On a
 * database that already holds that account it signs in instead, so re-running
 * against the same local database works too.
 *
 * The token reaches the workers through the environment: Playwright starts
 * them after global setup and they inherit process.env.
 */
const ADMIN_EMAIL = 'e2e-admin@users.opn.onl';

export default async function globalSetup(): Promise<void> {
    const ctx = await request.newContext();
    try {
        const health = await ctx.get(`${API_URL}/health`).catch((e) => {
            throw new Error(`backend not reachable at ${API_URL}: ${e}`);
        });
        if (!health.ok()) {
            throw new Error(`backend at ${API_URL} is unhealthy: ${health.status()}`);
        }

        let res = await ctx.post(`${API_URL}/auth/register`, {
            data: { email: ADMIN_EMAIL, password: TEST_PASSWORD },
            headers: clientIpHeader(),
        });
        if (res.status() === 409) {
            res = await ctx.post(`${API_URL}/auth/login`, {
                data: { email: ADMIN_EMAIL, password: TEST_PASSWORD },
                headers: clientIpHeader(),
            });
        }
        if (!res.ok()) {
            throw new Error(`admin bootstrap failed: ${res.status()} ${await res.text()}`);
        }
        const body = await res.json();
        if (!body.is_admin) {
            throw new Error(
                `${ADMIN_EMAIL} is not an admin: the database was not empty when it was ` +
                    'registered. Point the backend at a fresh database.',
            );
        }

        if (!body.email_verified) {
            const verify = await ctx.post(`${API_URL}/admin/users/${body.user_id}/verify-email`, {
                headers: { Authorization: `Bearer ${body.token}`, ...clientIpHeader() },
            });
            if (!verify.ok()) {
                throw new Error(`admin self-verify failed: ${verify.status()} ${await verify.text()}`);
            }
        }

        process.env.E2E_ADMIN_TOKEN = body.token;
        process.env.E2E_ADMIN_ID = String(body.user_id);
        process.env.E2E_ADMIN_EMAIL = ADMIN_EMAIL;
    } finally {
        await ctx.dispose();
    }
}
