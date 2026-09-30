import { randomUUID } from 'node:crypto';
import { test, expect, type APIRequestContext, type APIResponse, type Page } from '@playwright/test';
import { API_URL, WEB_URL, TEST_PASSWORD, createLink, createUser, signIn, uniqueEmail } from './support/api';

/**
 * Rate limiting against the real backend (backend/src/utils/rate_limiter.rs).
 *
 * Buckets are keyed by client IP, which the backend reads from
 * CF-Connecting-IP (TRUST_PROXY_HEADERS=true in e2e, as behind Cloudflare in
 * production). Every test owns one address, fixed for the whole test and
 * random per run, for its API calls and — via page.setExtraHTTPHeaders — its
 * browser traffic. So each test drains only its own bucket, a re-run against
 * the same backend starts fresh, and nothing else in the suite is affected.
 *
 * Limits under test (RateLimiters::default):
 *   per-second  10 req / 1 s    every non-redirect route, checked first
 *   auth        10 req / 60 s   /auth and /auth/*
 *   verify       5 req / 60 s   POST /:code/verify, per client and link
 *   redirect   100 req / 1 s    /:code, exempt from the per-second gate
 * plus the create-link handler's own cap: the same URL at most 10 times in
 * 10 minutes per user.
 */

/** A random address in 198.18.0.0/15, a range the shared helpers (10.x) never hand out. */
function freshClientIp(): string {
    const b = randomUUID().replace(/-/g, '');
    const octet = (i: number) => parseInt(b.slice(i * 2, i * 2 + 2), 16);
    return `198.${18 + (octet(0) & 1)}.${octet(1)}.${octet(2) || 1}`;
}

const asClient = (ip: string) => ({ 'CF-Connecting-IP': ip });

/** The message of a 429 from the per-minute buckets (auth, verify, ...). */
const WINDOW_LIMIT_MESSAGE = /^Rate limit exceeded\. Please try again in \d+ seconds\.$/;

/**
 * The dashboard's GET /links, which every dashboard mount loads. Waiting for it
 * (no timeout but the test's) rather than for the first element keeps a slow
 * first compile of the lazy dashboard chunk from failing an assertion.
 */
const linkListLoaded = (page: Page) =>
    page.waitForResponse((r) => r.request().method() === 'GET' && r.url() === `${API_URL}/links`);

/** Eleven simultaneous GET /health from one client: one more than the per-second gate allows. */
async function burst(request: APIRequestContext, ip: string, headers: Record<string, string> = {}) {
    const responses = await Promise.all(
        Array.from({ length: 11 }, () =>
            request.get(`${API_URL}/health`, { headers: { ...asClient(ip), ...headers } }),
        ),
    );
    return {
        allowed: responses.filter((r) => r.status() === 200),
        refused: responses.filter((r) => r.status() === 429),
    };
}

test.describe('sign-in budget', () => {
    test('ten failed sign-ins lock that client, and only that client, out of sign-in for the rest of the minute', async ({
        request,
    }) => {
        test.slow(); // eleven bcrypt verifications on a debug backend
        const user = await createUser(request);
        const attacker = freshClientIp();
        const signInFrom = (ip: string, password: string) =>
            request.post(`${API_URL}/auth/login`, { headers: asClient(ip), data: { email: user.email, password } });

        await test.step('ten wrong passwords are answered normally while the budget counts down', async () => {
            const responses = await Promise.all(
                Array.from({ length: 10 }, () => signInFrom(attacker, 'wrong-password-123')),
            );
            for (const res of responses) {
                expect(res.status()).toBe(401);
                expect(await res.json()).toEqual({ error: 'Invalid credentials' });
                expect(res.headers()['x-ratelimit-limit']).toBe('10');
            }
            const remaining = responses.map((r) => Number(r.headers()['x-ratelimit-remaining']));
            expect(remaining.sort((a, b) => a - b)).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
        });

        await test.step('the right password is refused from that client too', async () => {
            // The ten requests above may also have filled the 10-per-second gate,
            // whose 429 has a different message; poll until that second has passed.
            let res!: APIResponse;
            await expect
                .poll(
                    async () => {
                        res = await signInFrom(attacker, user.password);
                        return (await res.json()).message;
                    },
                    { intervals: [1_000], timeout: 10_000 },
                )
                .toMatch(WINDOW_LIMIT_MESSAGE);

            expect(res.status()).toBe(429);
            const retryAfter = Number(res.headers()['retry-after']);
            expect(retryAfter).toBeGreaterThanOrEqual(1);
            expect(retryAfter).toBeLessThanOrEqual(60);
            expect(await res.json()).toEqual({
                error: 'Too many requests',
                retry_after: retryAfter,
                message: `Rate limit exceeded. Please try again in ${retryAfter} seconds.`,
            });
            expect(res.headers()['x-ratelimit-limit']).toBe('10');
            expect(res.headers()['x-ratelimit-remaining']).toBe('0');
        });

        await test.step('another client can still sign in to the same account', async () => {
            const res = await signInFrom(freshClientIp(), user.password);
            expect(res.status()).toBe(200);
            expect((await res.json()).user_id).toBe(user.userId);
        });
    });

    test('a locked-out client is refused by both the sign-in and the sign-up form', async ({ page, request }) => {
        const user = await createUser(request);
        const lockedOut = freshClientIp();

        // Every request to /auth/* spends the budget, whatever its outcome.
        // Malformed ones skip password hashing, so the forms below are tried
        // well inside the minute even on a slow machine.
        const spent = await Promise.all(
            Array.from({ length: 10 }, () =>
                request.post(`${API_URL}/auth/login`, { headers: asClient(lockedOut), data: { email: user.email } }),
            ),
        );
        for (const res of spent) expect(res.status()).toBe(422);

        await page.setExtraHTTPHeaders(asClient(lockedOut));
        const postTo = (path: string) =>
            page.waitForResponse((r) => r.request().method() === 'POST' && r.url() === `${API_URL}${path}`);

        await test.step('sign-in shows the refusal and leaves the visitor signed out', async () => {
            await page.goto('/login');
            await page.getByLabel('Email address').fill(user.email);
            await page.getByLabel('Password').fill(user.password);
            const response = postTo('/auth/login');
            await page.getByRole('button', { name: 'Sign in', exact: true }).click();

            expect((await response).status()).toBe(429);
            await expect(page.getByRole('alert')).toHaveText('Too many requests');
            await expect(page).toHaveURL(/\/login$/);
            expect(await page.evaluate(() => localStorage.getItem('token'))).toBeNull();
        });

        await test.step('sign-up shares the budget and is refused as well', async () => {
            await page.goto('/register');
            await page.getByLabel('Email address').fill(uniqueEmail('locked-out'));
            await page.getByLabel('Password').fill(TEST_PASSWORD);
            const response = postTo('/auth/register');
            await page.getByRole('button', { name: 'Create account' }).click();

            expect((await response).status()).toBe(429);
            await expect(page.getByRole('alert')).toHaveText('Too many requests');
            await expect(page.getByRole('heading', { name: 'Check your email' })).toBeHidden();
        });
    });

    test('ordinary page loads do not use up the sign-in budget', async ({ request }) => {
        test.fail(
            true,
            'BUG: GET /auth/settings and /auth/me count against the 10/min sign-in bucket, so ten dashboard loads (or three Settings visits) lock the user out of sign-in',
        );
        const user = await createUser(request);
        const ip = freshClientIp();

        // Dashboard.tsx fetches GET /auth/settings on every mount; Settings.tsx
        // fetches /auth/me, /auth/settings, /auth/passkeys and /auth/api-keys.
        for (let i = 0; i < 10; i++) {
            const res = await request.get(`${API_URL}/auth/settings`, { headers: asClient(ip) });
            expect(res.status()).toBe(200);
        }

        // Past the 10-per-second gate the reads just filled, a sign-in should work.
        await expect
            .poll(
                async () => {
                    const res = await request.post(`${API_URL}/auth/login`, {
                        headers: asClient(ip),
                        data: { email: user.email, password: user.password },
                    });
                    return { status: res.status(), body: await res.json() };
                },
                {
                    message: 'sign-in from a client that has only loaded pages',
                    intervals: [1_100],
                    timeout: 5_000,
                },
            )
            .toMatchObject({ status: 200 });
    });
});

test.describe('per-second gate', () => {
    test('an eleventh API request within one second is refused', async ({ request }) => {
        const { allowed, refused } = await burst(request, freshClientIp());
        expect(allowed).toHaveLength(10);
        expect(refused).toHaveLength(1);

        const [res] = refused;
        const body = await res.json();
        expect(body).toMatchObject({
            error: 'Too many requests',
            message: 'Rate limit: maximum 10 requests per second',
        });
        expect(res.headers()['retry-after']).toBe(String(body.retry_after));
        expect(res.headers()['x-ratelimit-limit']).toBe('10');
        expect(res.headers()['x-ratelimit-remaining']).toBe('0');
    });

    test('a refused burst tells the client to wait before retrying', async ({ request }) => {
        test.fail(
            true,
            'BUG: Retry-After is the remaining window truncated to whole seconds, so every per-second 429 says "Retry-After: 0"',
        );
        const { refused } = await burst(request, freshClientIp());
        expect(refused).toHaveLength(1);
        expect(Number(refused[0].headers()['retry-after'])).toBeGreaterThanOrEqual(1);
    });

    test('the web app can read Retry-After on a cross-origin 429', async ({ request }) => {
        test.fail(
            true,
            'BUG: CORS sends no Access-Control-Expose-Headers, so the browser hides Retry-After and PasswordPrompt always says "60 seconds"',
        );
        // PasswordPrompt.tsx (and apiCall in config/api.ts) read Retry-After from
        // a fetch to the API origin, which is a different origin from the app.
        const { refused } = await burst(request, freshClientIp(), { Origin: WEB_URL });
        expect(refused).toHaveLength(1);
        const headers = refused[0].headers();
        expect(headers['access-control-allow-origin']).toBe(WEB_URL);
        expect(headers['access-control-expose-headers'] ?? '').toMatch(/retry-after/i);
    });

    test('short-link visits are exempt, even for a code that starts with "auth"', async ({ request }) => {
        // The middleware once classified routes by a path heuristic and filed
        // codes like this under the API or sign-in buckets. Twelve rapid visits
        // would trip either the 10-per-second gate or the 10-per-minute
        // sign-in budget; redirects get 100 per second instead.
        const owner = await createUser(request);
        const alias = `auth-sale-${randomUUID().slice(0, 8)}`;
        const link = await createLink(request, owner.token, { custom_alias: alias });
        const visitor = freshClientIp();

        const responses = await Promise.all(
            Array.from({ length: 12 }, () =>
                request.get(`${API_URL}/${alias}`, { headers: asClient(visitor), maxRedirects: 0 }),
            ),
        );
        for (const res of responses) {
            expect(res.status()).toBe(307);
            expect(res.headers()['location']).toBe(link.original_url);
        }
    });
});

test.describe('link password guesses', () => {
    test('five wrong guesses lock that client out of the link, and the prompt says so', async ({
        page,
        request,
    }) => {
        test.slow(); // bcrypt for the link password and every guess
        const owner = await createUser(request);
        const password = 'correct-horse-battery';
        const link = await createLink(request, owner.token, { password });
        const guesser = freshClientIp();
        const guess = (ip: string, attempt: string) =>
            request.post(`${API_URL}/${link.code}/verify`, { headers: asClient(ip), data: { password: attempt } });

        await test.step('five wrong guesses are answered normally while the budget counts down', async () => {
            const responses = await Promise.all(Array.from({ length: 5 }, () => guess(guesser, 'wrong-guess')));
            for (const res of responses) {
                expect(res.status()).toBe(401);
                expect(await res.json()).toEqual({ error: 'Invalid password' });
                expect(res.headers()['x-ratelimit-limit']).toBe('5');
            }
            const remaining = responses.map((r) => Number(r.headers()['x-ratelimit-remaining']));
            expect(remaining.sort((a, b) => a - b)).toEqual([0, 1, 2, 3, 4]);
        });

        await test.step('the prompt refuses even the right password from that client', async () => {
            await page.setExtraHTTPHeaders(asClient(guesser));
            await page.goto(`/password/${link.code}`);
            await page.getByLabel('Password').fill(password);
            const response = page.waitForResponse(
                (r) => r.request().method() === 'POST' && r.url() === `${API_URL}/${link.code}/verify`,
            );
            await page.getByRole('button', { name: 'Continue' }).click();

            const res = await response;
            expect(res.status()).toBe(429);
            const retryAfter = Number(res.headers()['retry-after']);
            expect(retryAfter).toBeGreaterThanOrEqual(1);
            expect(retryAfter).toBeLessThanOrEqual(60);
            expect(await res.json()).toEqual({
                error: 'Too many requests',
                retry_after: retryAfter,
                message: `Rate limit exceeded. Please try again in ${retryAfter} seconds.`,
            });

            await expect(page.getByText(/^Too many attempts\. Please try again in \d+ seconds\.$/)).toBeVisible();
            await expect(page).toHaveURL(new RegExp(`/password/${link.code}$`));
        });

        await test.step('another client with the right password gets through', async () => {
            const res = await guess(freshClientIp(), password);
            expect(res.status()).toBe(200);
            expect((await res.json()).redirect_url).toContain(`/${link.code}?unlock=`);
        });
    });
});

test.describe('link creation', () => {
    test('the same URL is refused an eleventh time in ten minutes; the form keeps it and other URLs still work', async ({
        page,
        request,
    }) => {
        const user = await createUser(request);
        const repeated = `https://www.iana.org/help/example-domains?campaign=${randomUUID()}`;
        // Sequential: the handler counts existing rows before inserting, so
        // parallel creates could race past the cap.
        for (let i = 0; i < 10; i++) {
            await createLink(request, user.token, { original_url: repeated });
        }

        await signIn(page, user);
        await page.setExtraHTTPHeaders(asClient(freshClientIp()));
        const listed = linkListLoaded(page);
        await page.goto('/dashboard');
        expect((await listed).status()).toBe(200);
        await expect(page.getByText(/^10 links?$/)).toBeVisible();

        const urlInput = page.getByPlaceholder('https://example.com/long-url');
        const createButton = page.getByRole('button', { name: 'Create', exact: true });
        const createResponse = () =>
            page.waitForResponse((r) => r.request().method() === 'POST' && r.url() === `${API_URL}/links`);

        await urlInput.fill(repeated);
        let response = createResponse();
        await createButton.click();
        let res = await response;
        expect(res.status()).toBe(429);
        expect(await res.json()).toEqual({
            error: 'You have shortened this URL too many times. Please wait a few minutes.',
        });
        await expect(page.getByRole('alert')).toHaveText(
            'You have shortened this URL too many times. Please wait a few minutes.',
        );
        await expect(urlInput).toHaveValue(repeated);
        await expect(page.getByText(/^10 links?$/)).toBeVisible();

        // The cap is per URL, not a lockout.
        await urlInput.fill(`https://www.iana.org/help/example-domains?campaign=${randomUUID()}`);
        response = createResponse();
        await createButton.click();
        res = await response;
        expect(res.status()).toBe(201);
        await expect(page.getByRole('alert')).toBeHidden();
        await expect(page.getByText(/^11 links?$/)).toBeVisible();
        await expect(urlInput).toHaveValue('');
    });

    test('a rate-limited link list shows the error, not an empty dashboard, until a reload after the limit', async ({
        page,
        request,
    }) => {
        const user = await createUser(request);
        const link = await createLink(request, user.token);
        await signIn(page, user);
        await page.setExtraHTTPHeaders(asClient(freshClientIp()));

        // Mocked: tripping the general bucket (100 requests a minute) for real
        // would take a hundred requests. Only GET /links is replaced, with the
        // middleware's exact 429; the rest of the page talks to the real backend
        // as this real user.
        const listUrl = `${API_URL}/links`;
        await page.route(listUrl, (route) =>
            route.request().method() === 'GET'
                ? route.fulfill({
                      status: 429,
                      headers: { 'Retry-After': '42', 'X-RateLimit-Limit': '100', 'X-RateLimit-Remaining': '0' },
                      json: {
                          error: 'Too many requests',
                          retry_after: 42,
                          message: 'Rate limit exceeded. Please try again in 42 seconds.',
                      },
                  })
                : route.fallback(),
        );
        const refusedList = linkListLoaded(page);
        await page.goto('/dashboard');
        expect((await refusedList).status()).toBe(429);
        await expect(page.getByRole('alert')).toHaveText('Too many requests');
        await expect(page.getByText('No links yet')).toBeHidden();

        await page.unroute(listUrl);
        const listed = linkListLoaded(page);
        await page.reload();
        expect((await listed).status()).toBe(200);
        const origin = new URL(page.url()).origin;
        await expect(page.getByRole('link', { name: `${new URL(origin).host}/${link.code}`, exact: true })).toBeVisible();
        await expect(page.getByRole('alert')).toBeHidden();
    });
});
