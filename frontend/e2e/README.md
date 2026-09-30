# Playwright E2E

337 tests in 16 files under `frontend/e2e/`. Default project is Chromium.
`playwright.config.ts` starts Vite on `http://localhost:5173` (`webServer`).
It does **not** start the backend. Specs that talk to a live API need Axum on
`:3000` and Postgres.

## Tiers

| Tier | When | What |
|---|---|---|
| **Gate** | every PR / push (`e2e` job) | Chromium, `password-link.spec.ts` + `edge-cases.spec.ts` (40 tests). These were the only two files that ran 100% green locally. |
| **Full** | `workflow_dispatch` or weekly Monday 06:17 UTC (`e2e-full` job) | Chromium, all 16 files. Expected red until the suite is rewritten against the current UI and `/auth`, `/links` routes (no `/api` prefix). |
| **All browsers** | local only | `E2E_FULL=1 npm run test:e2e` — firefox, webkit, Pixel 5, iPhone 12 as well. Not run in CI. |

A local run (2026-09-17, Chromium, retries 0, Vite `webServer` + backend on `:3000`):
**201 passed, 120 failed, 16 skipped** (serial describes aborted after the first fail). Failures are the tests, not the CI wiring: stale marketing copy, mock JWTs that `authFetch` 401s, `API_URL=http://localhost:3000/api` against routes that are `/auth/*` and `/links/*`, register asserted as 200 when the handler returns 201.

## Local

```bash
cd frontend
npx playwright install chromium
npm run test:e2e          # all files, Chromium; starts Vite
npm run test:e2e:gate     # the PR subset
npm run test:e2e:full     # all files, Chromium (same as test:e2e today)
E2E_FULL=1 npm run test:e2e   # five browser projects
```

For live-API specs (`api-integration`, `security`, `comprehensive-flow`):

```bash
# throwaway DB, then from backend/
DATABASE_URL=postgres://postgres:postgres@localhost:5432/opn_onl_test \
JWT_SECRET=ci-test-jwt-secret-please-change-in-production-0123456789 \
FRONTEND_URL=http://localhost:5173 \
BASE_URL=http://localhost:3000 \
cargo run
```

Vite 8 needs Node 20.19+ / 22+. CI uses Node 22.
