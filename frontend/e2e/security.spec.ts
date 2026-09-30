import { createHmac, randomUUID } from 'node:crypto';
import { test, expect, type APIRequestContext, type APIResponse } from '@playwright/test';
import {
    API_URL,
    WEB_URL,
    adminToken,
    api,
    clientIpHeader,
    createLink,
    createUser,
    signIn,
    uniqueEmail,
} from './support/api';

/**
 * Security properties of the real backend, mostly at the HTTP level, plus the
 * browser-side handling of hostile data (XSS) and of forged sessions.
 *
 * Response headers: e2e serves the SPA from the Vite dev server, not from
 * production nginx. The nginx security headers (CSP, X-Frame-Options, HSTS,
 * nosniff) are asserted by the CI "Frontend production headers" job; this file
 * only checks headers the backend itself sends.
 *
 * Every API request carries its own CF-Connecting-IP (helpers add one; direct
 * calls pass clientIpHeader()), so these tests neither spend nor depend on the
 * rate-limit buckets that browser traffic from 127.0.0.1 shares.
 */

/** A real, non-reserved destination. example.com/.org/.net are refused by the URL policy. */
const DESTINATION = 'https://www.iana.org/help/example-domains';

const suffix = () => randomUUID().slice(0, 8);

/** Claims of a JWT, decoded without verification. */
function claimsOf(jwt: string): Record<string, unknown> {
    return JSON.parse(Buffer.from(jwt.split('.')[1], 'base64url').toString('utf8'));
}

const segment = (value: unknown) => Buffer.from(JSON.stringify(value)).toString('base64url');

/** HS256 over `claims` with a key an attacker might guess. The server's JWT_SECRET is never exposed to the tests. */
function signWithGuessedKey(claims: Record<string, unknown>): string {
    const unsigned = `${segment({ alg: 'HS256', typ: 'JWT' })}.${segment(claims)}`;
    const signature = createHmac('sha256', 'guessed-secret-not-the-server-one-0123456789')
        .update(unsigned)
        .digest('base64url');
    return `${unsigned}.${signature}`;
}

async function listLinks(request: APIRequestContext, token: string) {
    const response = await api(request, 'get', '/links', token);
    expect(response.status()).toBe(200);
    return response.json();
}

async function login(request: APIRequestContext, email: string, password: string, client = clientIpHeader()) {
    return request.post(`${API_URL}/auth/login`, { data: { email, password }, headers: client });
}

test.describe('Security Tests', () => {
    test.describe('Authentication Security', () => {
        test('should not expose sensitive data in error messages', async ({ request }) => {
            const user = await createUser(request, { verified: false });

            // A wrong password for a real account and an unknown account must be
            // indistinguishable, or the login form enumerates registered emails.
            const [wrongPassword, unknownAccount] = await Promise.all([
                login(request, user.email, 'wrongpassword'),
                login(request, uniqueEmail('nobody'), 'wrongpassword'),
            ]);

            expect(wrongPassword.status()).toBe(401);
            expect(unknownAccount.status()).toBe(401);
            expect(await wrongPassword.json()).toEqual({ error: 'Invalid credentials' });
            expect(await unknownAccount.text()).toBe(await wrongPassword.text());
        });

        test('should require strong passwords', async ({ request }) => {
            const email = uniqueEmail('weak-password');

            const weak = await request.post(`${API_URL}/auth/register`, {
                data: { email, password: '1234567' },
                headers: clientIpHeader(),
            });

            expect(weak.status()).toBe(400);
            expect(await weak.json()).toEqual({
                error: 'password: Password must be at least 8 characters',
            });
            // The rejected attempt created nothing: the address is still free.
            const compliant = await request.post(`${API_URL}/auth/register`, {
                data: { email, password: '12345678' },
                headers: clientIpHeader(),
            });
            expect(compliant.status()).toBe(201);
        });

        test('should reject forged tokens', async ({ request }) => {
            // An expired-but-genuine token cannot be minted without the server's
            // JWT_SECRET; what an attacker can produce are these forgeries.
            const user = await createUser(request, { verified: false });
            const admin = claimsOf(adminToken());
            const [header, , signature] = user.token.split('.');
            const forgeries: Record<string, string> = {
                'own token, payload edited to the admin id': `${header}.${segment({
                    ...claimsOf(user.token),
                    user_id: admin.user_id,
                    sub: admin.sub,
                })}.${signature}`,
                'admin claims signed with a guessed key': signWithGuessedKey(admin),
                'admin claims, alg none, no signature': `${segment({ alg: 'none', typ: 'JWT' })}.${segment(admin)}.`,
            };

            for (const [name, token] of Object.entries(forgeries)) {
                const response = await api(request, 'get', '/admin/stats', token);
                expect(response.status(), name).toBe(401);
                expect(await response.json(), name).toEqual({ success: false, message: 'Invalid token' });
            }

            // Control: the genuine admin token with those same claims is accepted.
            const genuine = await api(request, 'get', '/admin/stats', adminToken());
            expect(genuine.status()).toBe(200);
        });

        test('should reject malformed tokens', async ({ request }) => {
            const user = await createUser(request, { verified: false });
            const basic = Buffer.from(`${user.email}:${user.password}`).toString('base64');
            const malformed: Record<string, string> = {
                'not a JWT': 'Bearer malformed-token',
                'empty bearer': 'Bearer ',
                'two segments': 'Bearer a.b',
                'valid token without a scheme': user.token,
                'valid token, wrong scheme': `Token ${user.token}`,
                'right password over Basic auth': `Basic ${basic}`,
            };

            for (const [name, authorization] of Object.entries(malformed)) {
                const response = await request.get(`${API_URL}/auth/me`, {
                    headers: { Authorization: authorization, ...clientIpHeader() },
                });
                expect(response.status(), name).toBe(401);
            }

            const control = await api(request, 'get', '/auth/me', user.token);
            expect(control.status()).toBe(200);
        });
    });

    test.describe('XSS Prevention', () => {
        test('should sanitize URL input', async ({ page, request }) => {
            const user = await createUser(request);
            await signIn(page, user);
            await page.goto('/');

            await page.getByRole('textbox', { name: 'URL to shorten' }).fill('javascript:alert(1)');
            const created = page.waitForResponse(
                (r) => r.url() === `${API_URL}/links` && r.request().method() === 'POST',
            );
            await page.getByRole('button', { name: 'Shorten' }).click();

            expect((await created).status()).toBe(400);
            await expect(page.getByRole('alert')).toHaveText('URL must use http or https protocol');
            expect(await listLinks(request, user.token)).toEqual([]);
        });

        test('should escape HTML in displayed URLs', async ({ page, request }) => {
            // Markup that passes the URL policy (no script/handler keywords) is
            // stored verbatim, then shown to every visitor of the public preview.
            const attacker = await createUser(request);
            const destination = `${DESTINATION}?q="><img src=x id=xss-probe>`;
            const link = await createLink(request, attacker.token, { original_url: destination });

            await page.goto(`/${link.code}+`);

            await expect(page.getByText(destination, { exact: true })).toBeVisible();
            await expect(page.locator('#xss-probe')).toHaveCount(0);
        });

        test('should not execute inline scripts from user input', async ({ page, request }) => {
            // Titles are free text (no URL policy), so the payload reaches the dashboard as-is.
            const user = await createUser(request);
            const payload = '<img src=x onerror="window.__xssFired = true">';
            await createLink(request, user.token, { title: payload });
            await signIn(page, user);

            await page.goto('/dashboard');

            // Rendered as text, so no element was created and its handler never ran.
            await expect(page.getByText(payload, { exact: true })).toBeVisible();
            await expect(page.locator('img[src="x"]')).toHaveCount(0);
            const fired = await page.evaluate(() => (window as unknown as { __xssFired?: boolean }).__xssFired);
            expect(fired).toBeUndefined();
        });
    });

    test.describe('CSRF Protection', () => {
        test('should reject requests without proper headers', async ({ request }) => {
            // Auth is a bearer token, never a cookie, so a cross-site request carries
            // no credential. The API must also not grant CORS to foreign origins.
            const user = await createUser(request);
            const link = await createLink(request, user.token);
            const preflight = (origin: string) =>
                request.fetch(`${API_URL}/links/${link.id}`, {
                    method: 'OPTIONS',
                    headers: {
                        Origin: origin,
                        'Access-Control-Request-Method': 'DELETE',
                        'Access-Control-Request-Headers': 'authorization',
                        ...clientIpHeader(),
                    },
                });

            const foreign = await preflight('https://evil.example');
            expect(foreign.headers()['access-control-allow-origin']).toBeUndefined();
            // Control: the app's own origin is granted, so CORS is active, not absent.
            const own = await preflight(new URL(WEB_URL).origin);
            expect(own.headers()['access-control-allow-origin']).toBe(new URL(WEB_URL).origin);

            const forged = await request.delete(`${API_URL}/links/${link.id}`, {
                headers: { Origin: 'https://evil.example', ...clientIpHeader() },
            });
            expect(forged.status()).toBe(401);
            expect(forged.headers()['access-control-allow-origin']).toBeUndefined();
            // The link is untouched.
            expect(await listLinks(request, user.token)).toEqual([expect.objectContaining({ id: link.id })]);
        });
    });

    test.describe('SQL Injection Prevention', () => {
        test('should handle SQL injection attempts in search', async ({ request }) => {
            // The dashboard filters in the browser; GET /links?search= is the
            // server-side search that reaches SQL (LIKE over url, code and notes).
            const user = await createUser(request);
            const mine = await createLink(request, user.token);
            const search = async (query: string) => {
                const response = await api(request, 'get', `/links?search=${encodeURIComponent(query)}`, user.token);
                expect(response.status(), query).toBe(200);
                return ((await response.json()) as Array<{ code: string }>).map((l) => l.code);
            };

            // Interpreted as SQL, a tautology would match the user's link; as the
            // literal text it is, it matches nothing...
            expect(await search("' OR '1'='1")).toEqual([]);
            expect(await search("') OR 1=1 --")).toEqual([]);
            // ...a stacked statement is not executed...
            expect(await search("'; DROP TABLE links; --")).toEqual([]);
            // ...and the table is intact and searchable.
            expect(await search(mine.code)).toEqual([mine.code]);
        });

        test('should handle SQL injection in API', async ({ request }) => {
            test.slow(); // every login attempt runs bcrypt, which is slow in debug builds
            const user = await createUser(request, { verified: false });
            const attempts = [
                { email: "admin'; DROP TABLE users; --", password: 'password' },
                { email: "x' OR '1'='1@users.opn.onl", password: "' OR '1'='1" },
                { email: `${user.email}' --`, password: 'anything' },
                { email: "test' OR '1'='1", password: 'test' },
            ];

            const responses = await Promise.all(attempts.map((a) => login(request, a.email, a.password)));

            for (const [i, response] of responses.entries()) {
                // The generic failure, never a 500 or a database error message.
                expect(response.status(), attempts[i].email).toBe(401);
                expect(await response.json(), attempts[i].email).toEqual({ error: 'Invalid credentials' });
            }
            // The users table survived and ordinary login still works.
            expect((await login(request, user.email, user.password)).status()).toBe(200);
        });
    });

    test.describe('Rate Limiting', () => {
        test('should rate limit login attempts', async ({ request }) => {
            test.slow(); // ten concurrent bcrypt verifications
            const victim = await createUser(request, { verified: false });
            // All guesses come from one address, as a brute force would.
            const attacker = clientIpHeader();

            const guesses = await Promise.all(
                Array.from({ length: 10 }, (_, i) => login(request, victim.email, `wrong-guess-${i}`, attacker)),
            );
            expect(guesses.map((r) => r.status())).toEqual(Array(10).fill(401));

            // The 11th attempt in the minute is refused even with the right password.
            const blocked = await login(request, victim.email, victim.password, attacker);
            expect(blocked.status()).toBe(429);
            expect(blocked.headers()['retry-after']).toMatch(/^\d+$/);
            expect(await blocked.json()).toMatchObject({ error: 'Too many requests' });

            // It is a per-client limit, not an account lockout.
            expect((await login(request, victim.email, victim.password)).status()).toBe(200);
        });

        test('should rate limit link creation', async ({ request }) => {
            const user = await createUser(request);
            const url = `${DESTINATION}?promo=${suffix()}`;

            for (let i = 1; i <= 10; i++) {
                const response = await api(request, 'post', '/links', user.token, { original_url: url });
                expect(response.status(), `link #${i}`).toBe(201);
            }
            const eleventh = await api(request, 'post', '/links', user.token, { original_url: url });

            expect(eleventh.status()).toBe(429);
            expect(await eleventh.json()).toEqual({
                error: 'You have shortened this URL too many times. Please wait a few minutes.',
            });
            // The limit is per destination: another URL still goes through.
            const other = await api(request, 'post', '/links', user.token, { original_url: `${url}-other` });
            expect(other.status()).toBe(201);
        });
    });

    test.describe('URL Safety', () => {
        test('should block dangerous URL schemes', async ({ request }) => {
            const user = await createUser(request);

            for (const url of [
                'javascript:alert(1)',
                'data:text/html,<script>alert(1)</script>',
                'vbscript:msgbox(1)',
                'file:///etc/passwd',
                'ftp://ftp.iana.org/pub/',
            ]) {
                const response = await api(request, 'post', '/links', user.token, { original_url: url });
                expect(response.status(), url).toBe(400);
                expect(await response.json(), url).toEqual({ error: 'URL must use http or https protocol' });
            }
            expect(await listLinks(request, user.token)).toEqual([]);
        });

        test('should block local/private IPs', async ({ request }) => {
            const user = await createUser(request);
            const internalHost = 'Links to local/internal hosts are not allowed';
            const rawIp = 'Links to raw IP addresses are not allowed';

            for (const [url, error] of [
                ['http://localhost/admin', internalHost],
                ['http://metadata.google.internal/computeMetadata/v1/', internalHost],
                ['http://printer.local/', internalHost],
                ['http://127.0.0.1:8080/', rawIp],
                ['http://169.254.169.254/latest/meta-data/', rawIp],
                ['http://10.0.0.1/', rawIp],
                ['http://[::1]/', rawIp],
            ]) {
                const response = await api(request, 'post', '/links', user.token, { original_url: url });
                expect(response.status(), url).toBe(400);
                expect(await response.json(), url).toEqual({ error });
            }
            expect(await listLinks(request, user.token)).toEqual([]);
        });

        test('should block links straight to executable files', async ({ request }) => {
            const user = await createUser(request);

            for (const [url, ext] of [
                ['https://www.iana.org/downloads/setup.exe', 'exe'],
                ['https://www.iana.org/news/update.hta?id=headline', 'hta'],
                ['https://www.iana.org/tools/install.ps1', 'ps1'],
                ['https://www.iana.org/app/latest.apk', 'apk'],
            ]) {
                const response = await api(request, 'post', '/links', user.token, { original_url: url });
                expect(response.status(), url).toBe(400);
                expect(await response.json(), url).toEqual({
                    error: `Links to .${ext} files are not allowed (potentially executable content)`,
                });
            }
            // Control: the same host with a document is fine.
            const document = await api(request, 'post', '/links', user.token, {
                original_url: 'https://www.iana.org/reports/annual.pdf',
            });
            expect(document.status()).toBe(201);
        });
    });

    test.describe('Session Security', () => {
        test('should invalidate session on password change', async ({ request }) => {
            const user = await createUser(request, { verified: false });
            const newPassword = 'Changed-Password-456!';

            const change = await api(request, 'post', '/auth/change-password', user.token, {
                current_password: user.password,
                new_password: newPassword,
            });

            expect(change.status()).toBe(200);
            const body = await change.json();
            expect(body.message).toBe('Password changed successfully');
            // Tokens issued before the change are revoked...
            expect((await api(request, 'get', '/auth/me', user.token)).status()).toBe(401);
            // ...the token returned with the change keeps this session going...
            expect((await api(request, 'get', '/auth/me', body.token)).status()).toBe(200);
            // ...and only the new password logs in.
            const [oldPassword, updated] = await Promise.all([
                login(request, user.email, user.password),
                login(request, user.email, newPassword),
            ]);
            expect(oldPassword.status()).toBe(401);
            expect(updated.status()).toBe(200);
        });

        test('should reject a forged admin session', async ({ page }) => {
            // localStorage is attacker-writable (XSS, shared machine). A token forged
            // for the admin plus is_admin=true must not open the admin panel.
            const forged = signWithGuessedKey(claimsOf(adminToken()));
            await page.addInitScript((token) => {
                // Plant once; the SPA must be free to clear it afterwards.
                if (sessionStorage.getItem('e2e-forged-session')) return;
                sessionStorage.setItem('e2e-forged-session', '1');
                localStorage.setItem('token', token);
                localStorage.setItem('is_admin', 'true');
            }, forged);
            const adminCall = page.waitForResponse(
                (r) => r.url() === `${API_URL}/admin/stats` && r.request().method() === 'GET',
            );

            await page.goto('/admin');

            expect((await adminCall).status()).toBe(401);
            await expect(page).toHaveURL(/\/login$/);
            expect(
                await page.evaluate(() => [localStorage.getItem('token'), localStorage.getItem('is_admin')]),
            ).toEqual([null, null]);
        });
    });

    test.describe('Input Validation', () => {
        test('should validate email format', async ({ request }) => {
            for (const email of ['not-an-email', 'missing-domain@', '@users.opn.onl', 'two@@users.opn.onl']) {
                const response = await request.post(`${API_URL}/auth/register`, {
                    data: { email, password: 'ValidPassword123!' },
                    headers: clientIpHeader(),
                });
                expect(response.status(), email).toBe(400);
                expect((await response.json()).error, email).toMatch(/^email: /);
            }
        });

        test('should limit URL length', async ({ request }) => {
            const user = await createUser(request);
            const base = `${DESTINATION}?pad=`;
            const atLimit = base + 'a'.repeat(2048 - base.length);

            const tooLong = await api(request, 'post', '/links', user.token, { original_url: `${atLimit}a` });

            expect(tooLong.status()).toBe(400);
            expect(await tooLong.json()).toEqual({ error: 'URL is too long (max 2048 characters)' });
            const longest = await api(request, 'post', '/links', user.token, { original_url: atLimit });
            expect(longest.status()).toBe(201);
            expect((await longest.json()).original_url).toHaveLength(2048);
        });

        test('should validate alias format', async ({ request }) => {
            const user = await createUser(request);
            const reserved = 'This alias is reserved and cannot be used';

            for (const [alias, error] of [
                ['invalid alias with spaces!', 'Alias can only contain letters, numbers, hyphens, and underscores'],
                ['abc', 'Alias must be at least 5 characters'],
                ['a'.repeat(51), 'Alias must be at most 50 characters'],
                ['-leading-hyphen', 'Alias cannot start or end with hyphen or underscore'],
                // Aliases must not shadow app routes (opn.onl/<alias> would open the page).
                ['dashboard', reserved],
                ['Settings', reserved],
            ]) {
                const response = await api(request, 'post', '/links', user.token, {
                    original_url: DESTINATION,
                    custom_alias: alias,
                });
                expect(response.status(), alias).toBe(400);
                expect(await response.json(), alias).toEqual({ error });
            }
            expect(await listLinks(request, user.token)).toEqual([]);
        });
    });

    test.describe('Security Headers', () => {
        test('should not leak the unlock token to the destination', async ({ request }) => {
            // Unlocking a password link puts a short-lived proof in the query string;
            // the final redirect must tell the browser not to send it on as a Referer.
            const user = await createUser(request);
            const destination = `${DESTINATION}?ref=${suffix()}`;
            const link = await createLink(request, user.token, {
                original_url: destination,
                password: 'link-secret-123',
            });

            const locked = await request.get(`${API_URL}/${link.code}`, {
                maxRedirects: 0,
                headers: clientIpHeader(),
            });
            expect(locked.status()).toBe(307);
            expect(locked.headers()['location']).toMatch(new RegExp(`/password/${link.code}$`));

            const verify = await request.post(`${API_URL}/${link.code}/verify`, {
                data: { password: 'link-secret-123' },
                headers: clientIpHeader(),
            });
            expect(verify.status()).toBe(200);
            const unlock = new URL((await verify.json()).redirect_url).searchParams.get('unlock');
            expect(unlock).toBeTruthy();

            const unlocked = await request.get(`${API_URL}/${link.code}?unlock=${encodeURIComponent(unlock!)}`, {
                maxRedirects: 0,
                headers: clientIpHeader(),
            });
            expect(unlocked.status()).toBe(307);
            expect(unlocked.headers()['location']).toBe(destination);
            expect(unlocked.headers()['referrer-policy']).toBe('no-referrer');
        });
    });

    test.describe('API Key Security', () => {
        test('should not expose API keys in responses', async ({ request }) => {
            const user = await createUser(request);

            const created = await api(request, 'post', '/auth/api-keys', user.token, { name: 'CI key' });
            expect(created.status()).toBe(201);
            const key = await created.json();
            expect(key.key).toMatch(/^opn_[A-Za-z0-9]{40}$/);
            expect(key.key_prefix).toBe(key.key.slice(0, 12));

            // The secret is shown once; listing returns the prefix only.
            const listed = await api(request, 'get', '/auth/api-keys', user.token);
            expect(listed.status()).toBe(200);
            expect(await listed.text()).not.toContain(key.key);
            expect(await listed.json()).toEqual([
                { id: key.id, name: 'CI key', key_prefix: key.key_prefix, last_used_at: null, created_at: key.created_at },
            ]);

            // The key works for the API it was issued for...
            expect((await api(request, 'get', '/links', key.key)).status()).toBe(200);
            // ...but cannot read or mint keys: that needs the account's own session...
            expect((await api(request, 'get', '/auth/api-keys', key.key)).status()).toBe(401);
            expect((await api(request, 'post', '/auth/api-keys', key.key, { name: 'escalated' })).status()).toBe(401);
            // ...and stops working once revoked.
            expect((await api(request, 'delete', `/auth/api-keys/${key.id}`, user.token)).status()).toBe(200);
            expect((await api(request, 'get', '/links', key.key)).status()).toBe(401);
        });
    });

    test.describe('Error Handling', () => {
        test('should not expose stack traces', async ({ request }) => {
            const user = await createUser(request, { verified: false });
            const auth = { Authorization: `Bearer ${user.token}` };
            const cases: Array<[string, APIResponse, number]> = [
                [
                    'wrong JSON type',
                    await request.post(`${API_URL}/links`, {
                        headers: { ...auth, ...clientIpHeader() },
                        data: { original_url: 123 },
                    }),
                    422,
                ],
                [
                    'wrong content type',
                    await request.post(`${API_URL}/links`, {
                        headers: { ...auth, 'Content-Type': 'text/plain', ...clientIpHeader() },
                        data: Buffer.from(DESTINATION),
                    }),
                    415,
                ],
                [
                    'out-of-range id',
                    await request.get(`${API_URL}/links/99999999999/qr`, { headers: { ...auth, ...clientIpHeader() } }),
                    400,
                ],
                [
                    'invalid query',
                    await request.get(`${API_URL}/links?limit=-1`, { headers: { ...auth, ...clientIpHeader() } }),
                    400,
                ],
            ];

            for (const [name, response, status] of cases) {
                // A client mistake is a 4xx with a short message, never a 500...
                expect(response.status(), name).toBe(status);
                // ...and never panics, backtraces, source paths or SQL.
                expect(await response.text(), name).not.toMatch(
                    /panicked|backtrace|\.rs:\d|src\/|sqlx|sea_orm|SELECT |INSERT |postgres/i,
                );
            }
        });
    });
});
