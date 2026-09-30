# Playwright E2E

233 tests in 16 files under `frontend/e2e/`, run against the real stack: Postgres, the backend
binary, and the Vite dev server that `playwright.config.ts` starts (`webServer`). Playwright does
not start the backend or the database.

## In CI

The `e2e` job runs the whole Chromium suite on every PR and push (`npm run test:e2e:ci`), with two
workers and up to two retries. It boots Postgres as a service, builds and starts the backend with
`TRUST_PROXY_HEADERS=true`, and gives each run a fresh database.

## Running locally

```bash
# 1. A fresh, empty database. globalSetup registers the first account, which the
#    backend makes an admin, and uses it to verify every test account.
createdb opn_e2e

# 2. The backend, from backend/
DATABASE_URL=postgres://localhost/opn_e2e \
JWT_SECRET=local-e2e-jwt-secret-at-least-32-characters \
FRONTEND_URL=http://localhost:5173 BASE_URL=http://localhost:3000 \
TRUST_PROXY_HEADERS=true \
cargo run

# 3. The suite, from frontend/ (starts Vite on :5173)
npx playwright install chromium
npm run test:e2e:ci
```

Re-running against the same database works: globalSetup signs the admin back in. To run a second
stack side by side, point the suite at it with `E2E_API_URL` (backend origin) and `E2E_WEB_PORT` (the
Vite port it starts); the backend's `FRONTEND_URL` must match that port for CORS.
`E2E_FULL=1 npm run test:e2e` adds Firefox, WebKit and two mobile projects; CI does not run those.

## How the specs are written

- **Real accounts, not fake tokens.** `e2e/support/api.ts` registers users through the API and
  verifies them through the admin endpoint (there is no SMTP), then `signIn(page, user)` seeds
  `localStorage` exactly as the login page does. Specs that only read share one account per worker.
- **Separate client addresses.** Helper requests, and each page via
  `page.setExtraHTTPHeaders(clientIpHeader())`, send their own `CF-Connecting-IP`, so no spec spends
  the per-address rate-limit budgets another spec asserts on. The rate-limit specs pin one address per
  test on purpose.
- **Mocks only where the real thing cannot be produced on demand**: a 500, a network failure, a slow
  response, GeoIP data (the e2e backend has no MaxMind database). Each mock says why in a comment.
- **No vacuous assertions.** `if (await el.isVisible()) { … }` passes when the element never renders;
  assert it is visible, then act.
