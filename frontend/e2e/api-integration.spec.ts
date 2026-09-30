import { randomUUID } from 'node:crypto';
import { test, expect, type APIResponse } from '@playwright/test';
import {
    API_URL,
    WEB_URL,
    TEST_PASSWORD,
    adminToken,
    api,
    clientIpHeader,
    createLink,
    createUser,
    uniqueEmail,
} from './support/api';

/**
 * HTTP-level contract tests against the real backend (no /api prefix: routes
 * are /auth/*, /links/*, /folders, /tags, /orgs/*, /admin/*, /:code).
 *
 * Every test builds its own users and data through the helpers, so the tests
 * run in any order, in parallel, and again on the same database. Every request
 * carries its own CF-Connecting-IP (the helpers add one; direct calls below
 * pass clientIpHeader()), so nothing here spends the per-IP rate-limit buckets
 * that browser traffic from 127.0.0.1 shares across the suite.
 */

/** A real, non-reserved destination. example.com/.org/.net are refused by the URL policy. */
const DESTINATION = 'https://www.iana.org/help/example-domains';

const suffix = () => randomUUID().slice(0, 8);

test.describe('API Integration Tests', () => {
    test.describe('Authentication API', () => {
        test('POST /auth/register - creates new user', async ({ request }) => {
            const email = uniqueEmail('api-register');
            const response = await request.post(`${API_URL}/auth/register`, {
                data: { email, password: TEST_PASSWORD },
                headers: clientIpHeader(),
            });

            expect(response.status()).toBe(201);
            const data = await response.json();
            expect(data).toMatchObject({ email, email_verified: false, is_admin: false });
            expect(data.user_id).toEqual(expect.any(Number));

            // The returned token is a working session for the new account.
            const me = await api(request, 'get', '/auth/me', data.token);
            expect(me.status()).toBe(200);
            expect(await me.json()).toMatchObject({ id: data.user_id, email });
        });

        test('POST /auth/login - authenticates user', async ({ request }) => {
            const user = await createUser(request);

            const response = await request.post(`${API_URL}/auth/login`, {
                data: { email: user.email, password: user.password },
                headers: clientIpHeader(),
            });

            expect(response.status()).toBe(200);
            const data = await response.json();
            expect(data).toMatchObject({
                user_id: user.userId,
                email: user.email,
                email_verified: true,
                is_admin: false,
            });
            const me = await api(request, 'get', '/auth/me', data.token);
            expect(me.status()).toBe(200);
            expect((await me.json()).id).toBe(user.userId);
        });

        test('POST /auth/login - rejects invalid credentials', async ({ request }) => {
            const user = await createUser(request, { verified: false });

            const response = await request.post(`${API_URL}/auth/login`, {
                data: { email: user.email, password: 'wrongpassword' },
                headers: clientIpHeader(),
            });

            expect(response.status()).toBe(401);
            expect(await response.json()).toEqual({ error: 'Invalid credentials' });
        });

        test('GET /auth/me - returns user profile', async ({ request }) => {
            const user = await createUser(request);

            const response = await api(request, 'get', '/auth/me', user.token);

            expect(response.status()).toBe(200);
            expect(await response.json()).toMatchObject({
                id: user.userId,
                email: user.email,
                email_verified: true,
                is_admin: false,
                link_count: 0,
                total_clicks: 0,
            });

            // The profile is live account state, not a snapshot of the token.
            await createLink(request, user.token);
            const after = await api(request, 'get', '/auth/me', user.token);
            expect((await after.json()).link_count).toBe(1);
        });

        test('GET /auth/me - rejects without token', async ({ request }) => {
            const response = await api(request, 'get', '/auth/me');

            expect(response.status()).toBe(401);
            expect(await response.json()).toEqual({ error: 'Unauthorized' });
        });
    });

    test.describe('Links API', () => {
        test('POST /links - creates new link', async ({ request }) => {
            const user = await createUser(request);

            const response = await api(request, 'post', '/links', user.token, {
                original_url: DESTINATION,
            });

            expect(response.status()).toBe(201);
            const data = await response.json();
            expect(data).toMatchObject({
                original_url: DESTINATION,
                click_count: 0,
                has_password: false,
                is_active: true,
                is_pinned: false,
                tags: [],
            });
            expect(data.id).toEqual(expect.any(Number));
            // Generated codes are six alphanumerics.
            expect(data.code).toMatch(/^[A-Za-z0-9]{6}$/);
            expect(data.short_url.endsWith(`/${data.code}`)).toBe(true);
            expect(data.api_url.endsWith(`/${data.code}`)).toBe(true);
        });

        test('POST /links - creates link with custom alias', async ({ request }) => {
            const user = await createUser(request);
            const customAlias = `api-alias-${suffix()}`;

            const response = await api(request, 'post', '/links', user.token, {
                original_url: DESTINATION,
                custom_alias: customAlias,
            });

            expect(response.status()).toBe(201);
            expect((await response.json()).code).toBe(customAlias);

            // The alias is now taken.
            const again = await api(request, 'post', '/links', user.token, {
                original_url: `${DESTINATION}?again=1`,
                custom_alias: customAlias,
            });
            expect(again.status()).toBe(409);
            expect(await again.json()).toEqual({ error: 'Alias already taken' });
        });

        test('POST /links - validates URL format', async ({ request }) => {
            const user = await createUser(request);

            const response = await api(request, 'post', '/links', user.token, {
                original_url: 'not-a-valid-url',
            });

            expect(response.status()).toBe(400);
            expect(await response.json()).toEqual({ error: 'Invalid URL format' });
            const list = await api(request, 'get', '/links', user.token);
            expect(await list.json()).toEqual([]);
        });

        test('POST /links - rejects blocked URLs', async ({ request }) => {
            const user = await createUser(request);
            const domain = `blocked-${suffix()}.e2e-blocklist.net`;
            const block = await api(request, 'post', '/admin/blocked/domains', adminToken(), {
                domain,
                reason: 'e2e blocklist check',
            });
            expect(block.status()).toBe(201);
            const { id: blockId } = await block.json();

            // The block covers the domain and its subdomains.
            for (const url of [`https://${domain}/offer`, `https://www.${domain}/offer`]) {
                const response = await api(request, 'post', '/links', user.token, { original_url: url });
                expect(response.status(), url).toBe(403);
                expect(await response.json(), url).toEqual({
                    error: 'This domain is blocked: e2e blocklist check',
                });
            }

            // Lifting the block makes the same destination acceptable again.
            const unblock = await api(request, 'delete', `/admin/blocked/domains/${blockId}`, adminToken());
            expect(unblock.status()).toBe(200);
            const allowed = await api(request, 'post', '/links', user.token, {
                original_url: `https://${domain}/offer`,
            });
            expect(allowed.status()).toBe(201);
        });

        test('POST /links - requires a verified email address', async ({ request }) => {
            const user = await createUser(request, { verified: false });

            const response = await api(request, 'post', '/links', user.token, {
                original_url: DESTINATION,
            });

            expect(response.status()).toBe(403);
            expect(await response.json()).toEqual({
                error: 'Please verify your email address before creating links',
            });
        });

        test('GET /links - lists user links', async ({ request }) => {
            const [owner, stranger] = await Promise.all([createUser(request), createUser(request)]);
            const first = await createLink(request, owner.token, { original_url: `${DESTINATION}?n=1` });
            const second = await createLink(request, owner.token, { original_url: `${DESTINATION}?n=2` });
            await createLink(request, stranger.token);

            const response = await api(request, 'get', '/links', owner.token);

            expect(response.status()).toBe(200);
            const data: Array<{ code: string; original_url: string }> = await response.json();
            // Only the caller's own links, newest first.
            expect(data.map((l) => [l.code, l.original_url])).toEqual([
                [second.code, second.original_url],
                [first.code, first.original_url],
            ]);
        });

        test('PUT /links/:id - updates link', async ({ request }) => {
            const [owner, stranger] = await Promise.all([createUser(request), createUser(request)]);
            const link = await createLink(request, owner.token);

            const denied = await api(request, 'put', `/links/${link.id}`, stranger.token, {
                notes: 'not yours',
            });
            expect(denied.status()).toBe(403);

            const response = await api(request, 'put', `/links/${link.id}`, owner.token, {
                notes: 'Updated via API test',
                title: 'API test title',
            });

            expect(response.status()).toBe(200);
            expect(await response.json()).toMatchObject({
                id: link.id,
                code: link.code,
                notes: 'Updated via API test',
                title: 'API test title',
            });
            const list = await api(request, 'get', '/links', owner.token);
            expect(await list.json()).toEqual([
                expect.objectContaining({ id: link.id, notes: 'Updated via API test', title: 'API test title' }),
            ]);
        });

        test('GET /:code - redirects to original URL', async ({ request }) => {
            const user = await createUser(request);
            const destination = `${DESTINATION}?campaign=${suffix()}`;
            const link = await createLink(request, user.token, { original_url: destination });

            const response = await request.get(`${API_URL}/${link.code}`, {
                maxRedirects: 0,
                headers: clientIpHeader(),
            });

            // 307, not a cacheable 301: every visit must reach the backend so it is
            // counted and later edits, blocks and expiry apply.
            expect(response.status()).toBe(307);
            expect(response.headers()['location']).toBe(destination);
        });

        test('GET /links/:id/qr - generates QR code', async ({ request }) => {
            const user = await createUser(request);
            const link = await createLink(request, user.token);

            const anonymous = await api(request, 'get', `/links/${link.id}/qr`);
            expect(anonymous.status()).toBe(401);

            const response = await api(request, 'get', `/links/${link.id}/qr`, user.token);

            expect(response.status()).toBe(200);
            expect(response.headers()['content-type']).toBe('image/png');
            const png = await response.body();
            expect([...png.subarray(0, 8)]).toEqual([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
        });

        test('DELETE /links/:id - deletes link', async ({ request }) => {
            const [owner, stranger] = await Promise.all([createUser(request), createUser(request)]);
            const link = await createLink(request, owner.token);

            const denied = await api(request, 'delete', `/links/${link.id}`, stranger.token);
            expect(denied.status()).toBe(403);

            const response = await api(request, 'delete', `/links/${link.id}`, owner.token);

            expect(response.status()).toBe(200);
            expect(await response.json()).toEqual({ message: 'Link deleted successfully' });
            const list = await api(request, 'get', '/links', owner.token);
            expect(await list.json()).toEqual([]);
            const redirect = await request.get(`${API_URL}/${link.code}`, {
                maxRedirects: 0,
                headers: clientIpHeader(),
            });
            expect(redirect.status()).toBe(404);
        });
    });

    test.describe('Analytics API', () => {
        test('GET /links/:id/stats - requires the link owner', async ({ request }) => {
            const [owner, stranger] = await Promise.all([createUser(request), createUser(request)]);
            const link = await createLink(request, owner.token);

            const anonymous = await api(request, 'get', `/links/${link.id}/stats`);
            expect(anonymous.status()).toBe(401);
            const foreign = await api(request, 'get', `/links/${link.id}/stats`, stranger.token);
            expect(foreign.status()).toBe(403);

            const own = await api(request, 'get', `/links/${link.id}/stats`, owner.token);
            expect(own.status()).toBe(200);
            expect(await own.json()).toMatchObject({
                link_id: link.id,
                code: link.code,
                original_url: link.original_url,
                total_clicks: 0,
                recent_clicks: [],
            });
        });
    });

    test.describe('Folders API', () => {
        test('POST /folders - creates folder', async ({ request }) => {
            const user = await createUser(request);
            const name = `API Test Folder ${suffix()}`;

            const response = await api(request, 'post', '/folders', user.token, { name, color: '#3b82f6' });

            expect(response.status()).toBe(201);
            expect(await response.json()).toMatchObject({
                name,
                color: '#3b82f6',
                user_id: user.userId,
                org_id: null,
                link_count: 0,
            });
        });

        test('GET /folders - lists folders', async ({ request }) => {
            const user = await createUser(request);
            for (const name of ['Zeta folder', 'Alpha folder']) {
                const created = await api(request, 'post', '/folders', user.token, { name });
                expect(created.status()).toBe(201);
            }

            const response = await api(request, 'get', '/folders', user.token);

            expect(response.status()).toBe(200);
            const data: Array<{ name: string }> = await response.json();
            expect(data.map((f) => f.name)).toEqual(['Alpha folder', 'Zeta folder']);
        });

        test('PUT /folders/:id - updates folder', async ({ request }) => {
            const user = await createUser(request);
            const created = await api(request, 'post', '/folders', user.token, { name: 'Before rename' });
            const folder = await created.json();

            const response = await api(request, 'put', `/folders/${folder.id}`, user.token, {
                name: 'Updated Folder Name',
                color: '#ef4444',
            });

            expect(response.status()).toBe(200);
            expect(await response.json()).toMatchObject({
                id: folder.id,
                name: 'Updated Folder Name',
                color: '#ef4444',
            });
            const reread = await api(request, 'get', `/folders/${folder.id}`, user.token);
            expect(await reread.json()).toMatchObject({ name: 'Updated Folder Name', color: '#ef4444' });
        });

        test('DELETE /folders/:id - deletes folder', async ({ request }) => {
            const user = await createUser(request);
            const folder = await (await api(request, 'post', '/folders', user.token, { name: 'Doomed' })).json();
            const link = await createLink(request, user.token, { folder_id: folder.id });
            expect(link.folder_id).toBe(folder.id);

            const response = await api(request, 'delete', `/folders/${folder.id}`, user.token);

            expect(response.status()).toBe(204);
            const reread = await api(request, 'get', `/folders/${folder.id}`, user.token);
            expect(reread.status()).toBe(404);
            // Deleting a folder unfiles its links; it does not delete them.
            const list = await api(request, 'get', '/links', user.token);
            expect(await list.json()).toEqual([expect.objectContaining({ id: link.id, folder_id: null })]);
        });
    });

    test.describe('Tags API', () => {
        test('POST /tags - creates tag', async ({ request }) => {
            const user = await createUser(request);
            const name = `api-tag-${suffix()}`;

            const response = await api(request, 'post', '/tags', user.token, { name, color: '#ef4444' });

            expect(response.status()).toBe(201);
            expect(await response.json()).toMatchObject({
                name,
                color: '#ef4444',
                user_id: user.userId,
                org_id: null,
                link_count: 0,
            });
        });

        test('GET /tags - lists tags', async ({ request }) => {
            const user = await createUser(request);
            const tags = [];
            for (const name of ['urgent', 'archive']) {
                const created = await api(request, 'post', '/tags', user.token, { name });
                expect(created.status()).toBe(201);
                tags.push(await created.json());
            }
            await createLink(request, user.token, { tag_ids: [tags[0].id] });

            const response = await api(request, 'get', '/tags', user.token);

            expect(response.status()).toBe(200);
            const data: Array<{ name: string; link_count: number }> = await response.json();
            expect(data.map((t) => [t.name, t.link_count])).toEqual([
                ['archive', 0],
                ['urgent', 1],
            ]);
        });

        test('DELETE /tags/:id - deletes tag', async ({ request }) => {
            const user = await createUser(request);
            const tag = await (await api(request, 'post', '/tags', user.token, { name: 'short-lived' })).json();
            const link = await createLink(request, user.token, { tag_ids: [tag.id] });
            expect(link.tags).toEqual([expect.objectContaining({ id: tag.id })]);

            const response = await api(request, 'delete', `/tags/${tag.id}`, user.token);

            expect(response.status()).toBe(204);
            expect(await (await api(request, 'get', '/tags', user.token)).json()).toEqual([]);
            // The link survives without the tag.
            const list = await api(request, 'get', '/links', user.token);
            expect(await list.json()).toEqual([expect.objectContaining({ id: link.id, tags: [] })]);
        });
    });

    test.describe('Organizations API', () => {
        test('POST /orgs - creates organization', async ({ request }) => {
            const user = await createUser(request);
            const name = `API Test Org ${suffix()}`;
            const slug = `api-org-${suffix()}`;

            const response = await api(request, 'post', '/orgs', user.token, { name, slug });

            expect(response.status()).toBe(201);
            expect(await response.json()).toMatchObject({
                name,
                slug,
                owner_id: user.userId,
                member_count: 1,
                link_count: 0,
            });

            // Slugs are unique across the instance.
            const taken = await api(request, 'post', '/orgs', user.token, { name: 'Copycat', slug });
            expect(taken.status()).toBe(409);
            expect(await taken.json()).toEqual({ error: 'Slug already exists' });
        });

        test('GET /orgs - lists organizations', async ({ request }) => {
            const user = await createUser(request);
            const slug = `api-org-${suffix()}`;
            const created = await api(request, 'post', '/orgs', user.token, { name: 'Listed Org', slug });
            expect(created.status()).toBe(201);

            const response = await api(request, 'get', '/orgs', user.token);

            expect(response.status()).toBe(200);
            expect(await response.json()).toEqual([
                expect.objectContaining({ name: 'Listed Org', slug, owner_id: user.userId, member_count: 1 }),
            ]);
        });

        test('DELETE /orgs/:id - deletes organization', async ({ request }) => {
            const user = await createUser(request);
            const slug = `api-org-${suffix()}`;
            const org = await (await api(request, 'post', '/orgs', user.token, { name: 'Doomed Org', slug })).json();

            const response = await api(request, 'delete', `/orgs/${org.id}`, user.token);

            expect(response.status()).toBe(204);
            expect(await (await api(request, 'get', '/orgs', user.token)).json()).toEqual([]);
            // The organization is gone for good, so its slug is free again.
            const reuse = await api(request, 'post', '/orgs', user.token, { name: 'Phoenix Org', slug });
            expect(reuse.status()).toBe(201);
        });
    });

    test.describe('Rate Limiting', () => {
        test('should enforce rate limits', async ({ request }) => {
            const user = await createUser(request);
            // One client address for the whole burst: the test spends only its own bucket.
            const client = clientIpHeader();

            const responses = await Promise.all(
                Array.from({ length: 110 }, () =>
                    request.get(`${API_URL}/links`, {
                        headers: { Authorization: `Bearer ${user.token}`, ...client },
                    }),
                ),
            );

            const statuses = responses.map((r) => r.status());
            expect(statuses.filter((s) => s !== 200 && s !== 429)).toEqual([]);
            expect(statuses).toContain(200);
            // A client gets at most 100 API calls a minute (the 10-per-second gate
            // can only turn away more), so at least 10 of 110 must be refused.
            const limited = responses.filter((r) => r.status() === 429);
            expect(limited.length).toBeGreaterThanOrEqual(10);
            expect(limited[0].headers()['retry-after']).toMatch(/^\d+$/);
            expect(await limited[0].json()).toMatchObject({ error: 'Too many requests' });

            // The limit is per client, not global: another address is still served.
            const other = await api(request, 'get', '/links', user.token);
            expect(other.status()).toBe(200);
        });
    });

    test.describe('Error Responses', () => {
        test('should return proper error format', async ({ request }) => {
            const user = await createUser(request);

            // API errors are JSON objects with a single human-readable `error`.
            const cases: Array<[APIResponse, number, string]> = [
                [await api(request, 'get', '/links'), 401, 'Unauthorized'],
                [
                    await api(request, 'post', '/links', user.token, { original_url: 'nope' }),
                    400,
                    'Invalid URL format',
                ],
                [await api(request, 'delete', '/links/2147483647', user.token), 404, 'Link not found'],
            ];
            for (const [response, status, error] of cases) {
                expect(response.status(), error).toBe(status);
                expect(response.headers()['content-type'], error).toBe('application/json');
                expect(await response.json(), error).toEqual({ error });
            }

            // An unknown short code is a plain 404, not a redirect somewhere.
            const unknown = await request.get(`${API_URL}/no-such-code-${suffix()}`, {
                maxRedirects: 0,
                headers: clientIpHeader(),
            });
            expect(unknown.status()).toBe(404);
            expect(await unknown.text()).toBe('Link not found');
        });

        test('should handle malformed JSON', async ({ request }) => {
            const user = await createUser(request);

            const response = await request.post(`${API_URL}/links`, {
                headers: {
                    Authorization: `Bearer ${user.token}`,
                    'Content-Type': 'application/json',
                    ...clientIpHeader(),
                },
                // A Buffer is sent verbatim; a string with a JSON content type
                // would be JSON-encoded by Playwright into the valid `"not json"`.
                data: Buffer.from('not json'),
            });

            expect(response.status()).toBe(400);
            expect(await response.text()).toContain('Failed to parse the request body as JSON');
            const list = await api(request, 'get', '/links', user.token);
            expect(await list.json()).toEqual([]);
        });
    });

    test.describe('CORS Headers', () => {
        test('should return CORS headers', async ({ request }) => {
            // The SPA calls the API cross-origin (VITE_API_URL), so its origin
            // (FRONTEND_URL on the backend) must be granted.
            const origin = new URL(WEB_URL).origin;

            const preflight = await request.fetch(`${API_URL}/links`, {
                method: 'OPTIONS',
                headers: {
                    Origin: origin,
                    'Access-Control-Request-Method': 'POST',
                    'Access-Control-Request-Headers': 'authorization,content-type',
                    ...clientIpHeader(),
                },
            });
            expect(preflight.status()).toBe(200);
            expect(preflight.headers()['access-control-allow-origin']).toBe(origin);
            expect(preflight.headers()['access-control-allow-methods']).toBeTruthy();

            const response = await request.get(`${API_URL}/auth/settings`, {
                headers: { Origin: origin, ...clientIpHeader() },
            });
            expect(response.status()).toBe(200);
            expect(response.headers()['access-control-allow-origin']).toBe(origin);
        });
    });
});
