# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Machine context (this host)

Cross-repo / MCP / disk / Azure / efficiency unlocks:
`~/work/AGENTS.md` (walk-up discovery; **this file does not replace it**).

- Registry: `~/.config/agent-coordination/AGENT-REGISTRY.md`
- Runbooks (on demand): `~/work/_runbooks/` - azure-disabled, ai-providers, disk-cleanup, deploy-verify, ...
- **SSH:** never connect without operator OK naming exact host — `~/work/_runbooks/ssh-and-hosts.md` (many servers; do not guess).
- **Efficiency unlocks:** fast env help -> DIY if safe, or ask operator once (what / why / ~sec). Not invent-scope.
- **Azure:** 🛑 **HARD OFF** (2026-07-16) — credits exhausted; any call now bills a real card. Make zero Azure calls; `azure_image`/`bulk_text` were removed from every agent config. Re-enable only via `_runbooks/azure-disabled.md`. Who-uses-what: `_runbooks/ai-providers.md`.

## What this is

opn.onl — privacy-focused, self-hostable URL shortener. Rust/Axum/SeaORM/PostgreSQL backend (`backend/`), React 19/TypeScript/Vite/Tailwind frontend (`frontend/`). AGPL-3.0.

## Commands

### Backend (`backend/`)

```bash
docker-compose -f docker-compose.dev.yml up -d   # Postgres + Redis for local dev (run from repo root)
cargo run                                        # dev server on :3000 (migrations run automatically on startup)
cargo clippy                                     # lint
cargo build --release

# Tests are real integration tests against Postgres — DATABASE_URL must point
# at a throwaway database (migrations run automatically, once per process):
createdb opn_test
DATABASE_URL=postgres://localhost/opn_test cargo test
DATABASE_URL=postgres://localhost/opn_test cargo test --test admin_tests   # single test file
```

### Frontend (`frontend/`)

```bash
npm run dev        # Vite dev server on :5173 (honors PORT env var)
npm run build      # tsc -b && vite build (production build also prerenders static pages via Puppeteer)
npm run lint       # ESLint
npm run test       # Vitest unit tests (watch mode; `vitest run` for one-shot)
npx vitest run src/pages/Admin.test.tsx   # single test file
npm run test:e2e:ci   # Playwright, whole Chromium suite (what CI runs on every PR)
```

The Playwright suite runs against the real backend: start it on `:3000` with a **fresh database** and
`TRUST_PROXY_HEADERS=true` first. `frontend/e2e/README.md` has the exact commands.

## Architecture

### Backend

- **`src/lib.rs` is the single source of truth for the app**: `AppState` + `build_router()`. The binary (`src/main.rs`) is a thin wrapper: env, logging, DB connect, migrations, serve. Integration tests import the real router via the lib target (`opn_onl_backend`). `build_router` must stay byte-for-byte what the binary serves; middleware order (with_state → https_redirect → rate limit → CORS → tracing → security headers) must not be reordered.
- **`src/handlers/`** — one module per domain (auth, links, analytics, admin, organizations, folders, tags, passkeys, api_keys, bio, websocket, contact). Handlers do their own auth: they parse the `Authorization: Bearer` header themselves (JWT via `utils::decode_jwt`, or `opn_…` API keys); there is no auth middleware/extractor layer. Admin handlers gate on `require_admin` in `handlers/admin.rs` (checks `is_admin` and excludes soft-deleted users).
- **`src/entity/`** — SeaORM models. **Soft delete is the norm**: `users` and `links` have `deleted_at`; most list queries must filter `DeletedAt.is_null()`. Soft delete is an UPDATE, so FK `ON DELETE CASCADE` does not fire — related cleanup (e.g. passkeys on user delete) must be done explicitly.
- **`migration/`** — SeaORM migration crate; migrations run automatically on startup and on first test-suite connect.
- **`utils/`** — `ClickBuffer` (batches click events before DB flush), `RedisCache` (optional redirect cache — handlers that change link state must invalidate it or blocks/edits take up to the TTL to apply; use `links::invalidate_cached_codes` / `active_link_codes_for_user`), `EmailService` (optional; unset SMTP = verification/reset/password-changed emails are skipped and `POST /contact` answers 503, since the message has nowhere to go; every email is an `EmailBody` with a text part and an HTML part), `BackupService` (S3; optional), rate limiters, JWT, GeoIP, privacy sweep (IP truncation at collection, retention anonymization; referer stored host-only; `purge_click_pii_for_user` on account delete). `RateLimiters` lives on `AppState` (shared by the rate-limit middleware and handlers, e.g. the redirect password path enforces the `password_verify` limiter in-handler). Middleware classifies redirect vs API by route prefix, not path length.
- **Timestamps leave the API as RFC 3339 UTC** (`2026-11-15T04:59:00Z`). Every `*_at` column is a naive `TIMESTAMP` holding UTC; format it with `utils::time::utc_rfc3339`, never `NaiveDateTime::to_string()`. Browsers read an offset-less time as local time, which shifted every displayed time by the viewer's UTC offset until 2026-09.
- **Errors are JSON `{"error": "..."}`**: build them with `handlers::json_error` (or the file's existing `ErrorResponse` / `serde_json::json!` pattern), never a bare `(StatusCode, "text")`, which sends `text/plain` and makes clients guess per endpoint. The one exception is `GET /{code}`: a missing or dead link goes through `links::dead_link`, which answers a browser with a small HTML page and anything else with the plain reason. Validation messages from `validator` go through `utils::validation::validation_error_message`, whose output never includes the submitted value.
- **bcrypt only through `utils::jwt::{hash_password, verify_password}`**: both are async and run on the blocking pool. Calling `bcrypt::hash`/`verify` inline in a handler stalls the Tokio worker and every redirect scheduled on it. New account and link passwords are capped at `MAX_PASSWORD_BYTES` (72, bcrypt's input limit); login and link unlock accept any length so older passwords keep working.
- **Rate limits**: only `POST /auth/*` spends the 10-per-minute sign-in budget; `GET /auth/me`, `/auth/settings` and the other reads are on the general bucket (every page load makes them). 429s carry `Retry-After` (rounded up) and `X-RateLimit-*`. `build_cors()` echoes the requested headers (`Access-Control-Allow-Headers: *` does not cover `Authorization` under the Fetch spec) and exposes the rate-limit headers, because production is split-origin.
- **Auth/roles**: single `is_admin` flag on users (no role table). First registered user becomes admin (`ensure_admin_exists`). `token_version` on users invalidates old JWTs on credential change. `JWT_SECRET` is validated at boot (rejects short / known-placeholder values).
- **Route order matters**: `/:code` redirect routes are registered last so they don't shadow API routes.
- API docs generated via utoipa; new handlers should carry `#[utoipa::path]` annotations and be registered in `src/openapi.rs`. The served document is built by `openapi::api_doc()`, which stamps `CARGO_PKG_VERSION` — **do not put a `version` literal back into `#[openapi(info(...))]`** (utoipa only accepts a literal there, which is exactly how the published spec ended up advertising 1.2.1 after 1.3.0 shipped).

### Frontend

- SPA with react-router (`src/App.tsx`). Marketing pages are **eagerly imported** so the production build prerenders them to static HTML (SEO); app pages (Dashboard, Admin, Settings…) are lazy-loaded and not prerendered.
- **Adding a page route requires touching up to three places**: `src/App.tsx` (route), `vite.config.ts` `PRERENDER_ROUTES` (only if it should be prerendered), and `frontend/nginx.conf` SPA-route allowlist regex (production only — anything not on that allowlist matching `/[a-zA-Z0-9]{4,50}` is proxied to the backend as a short-link code and will 404 as a page).
- API access goes through `src/config/api.ts` (`API_ENDPOINTS`, `authFetch` — reads JWT from localStorage). No state-management library; pages fetch directly.
- **Never touch `localStorage`/`sessionStorage` directly**; use `src/utils/storage.ts`. When a visitor blocks site data, storage access throws, and one unguarded read during render used to replace every page, the public ones included, with the error screen.
- Show short links with `shortLinkBase()` / `shortLinkHost()` from `src/config/api.ts`, never a literal `opn.onl` (self-hosted instances have their own host). Count labels go through `src/utils/plural.ts`.
- Prerender in Docker must use Playwright/Puppeteer's bundled Chromium — Debian's apt chromium SIGTRAPs in the builder container (see vite.config.ts sandbox flags).
- **Runtime config is injected at container start, never at build time** (one image serves every deployment): `docker-entrypoint.sh` substitutes `%%GA_ID%%` / `%%GA_CONSENT_MODE%%` in **every `*.html` under the web root**, not just `index.html` — each prerendered route ships its own copy of the placeholders, so substituting only the SPA shell leaves the config dead on 11 of 12 pages. `scripts/test-entrypoint.sh` pins this and runs in CI. Anything that reads these values (`ConsentBanner`, the analytics disclosure on `Privacy.tsx`) must treat an unsubstituted `%%…%%` placeholder as "unknown", because that is what the prerenderer and `npm run dev` see.

### Deployment

- Production: docker-compose + Cloudflare Tunnel; frontend nginx proxies `/{code}` redirects and `/{code}/verify|preview` to the backend, serves prerendered HTML for static routes, and falls back to the SPA shell.
- **HTML is served `Cache-Control: no-cache`; only content-hashed assets are cached (1 year, immutable).** HTML carries both the hashed asset names and the entrypoint-injected runtime config, so a cached shell pins a returning visitor to the previous deploy — that is why the old consent banner survived the 1.3.0 rollout. The header comes from a `map` on content type, deliberately not from per-location `add_header` (which would drop every inherited security header). The CI header smoke test asserts it.
- Images are built by GitHub Actions on push to the `release` branch (`ghcr.io/ysalitrynskyi/opn-{backend,frontend}` multi-arch amd64+arm64); Portainer compose files consume `:latest` or a pinned version tag.

## Releasing

Backend and frontend share one version number. Bumping it means editing exactly these places — everything else derives:

| Place | Note |
|-------|------|
| `backend/Cargo.toml` (`version`) | Source of truth for the backend. |
| `backend/Cargo.lock` | Run `cargo update -p opn_onl_backend --offline` (or any cargo command) so the lock records the new version. |
| `frontend/package.json` (`version`) | Metadata only — nothing reads it at runtime. Keep it in lockstep so an image tag means one thing. |
| `README.md` | The "pin a release tag such as `:X.Y.Z`" example, so copy-pasters don't pin a stale tag. |

Deliberately **not** a place to edit: the OpenAPI `info.version` (reads `CARGO_PKG_VERSION`, see the Backend notes), image tags and release-notes versions (CI derives both from the git tag).

**What PR CI does not see.** The backend toolchain is pinned to 1.98.1 in
`backend/rust-toolchain.toml`, in every CI job that installs Rust, and in the release image
(`rust:1.98.1-alpine3.21`, same Alpine release as the runtime stage). PR CI builds
`backend/Dockerfile` for linux/amd64 only, does not push it, and skips that build when the diff
does not touch `backend/` or `.github/workflows/`. It does not build linux/arm64, so a musl
failure that exists only on that architecture still waits for the release workflow. The frontend
job runs `npm run build` (typecheck, Vite, prerender, and `verify-prerender: OK`). It does not
build the frontend image; the headers job builds only the nginx config stage. Migrations run at
backend start, before it serves anything, so a migration that rewrites a large table (a column
type change on `click_events`, say) is downtime. Check the table size and say so in the release
notes.

Release flow, in order:

1. Feature branch → PR → all CI checks green → merge to `main`. **Branch from `main`** — cutting a branch while still checked out on `release` drags release-only history into the next PR (that is how the deploy host's postgres port leaked into `main` before 1.3.1).
2. `git checkout release && git merge main -m "chore(release): sync main for vX.Y.Z"`. `release` carries no deliberate deviation from `main` any more — host-specific settings are env vars (e.g. `POSTGRES_HOST_PORT`), so `git diff main release` should come back empty. A conflict here means someone re-introduced a branch-local difference; fix that rather than resolving it every release.
3. Push `release` — this builds and publishes multi-arch `:latest`. **Let that `docker-build.yml` run start before pushing the tag**: the workflow's concurrency group keeps one run queued at a time, so pushing `release`, the tag and the release within seconds cancels the `release` run, and that is the only run that publishes `:latest` (it happened on 1.4.0). If it was cancelled, `gh run rerun <its id>` once the tag run is going.
4. Tag `vX.Y.Z` on the release merge commit, push the tag, then `gh release create` (notes follow the shape of the previous releases: Summary / themed sections / Images / Upgrade).
5. Both the tag push and the release publication trigger `docker-build.yml`, which serializes on a single concurrency group, so **the earlier queued run reports `cancelled`** — expected, not a failure. The surviving run publishes the `X.Y.Z` tags.
6. Verify before declaring done: `docker manifest inspect ghcr.io/ysalitrynskyi/opn-backend:X.Y.Z` (and `opn-frontend`) must list both `linux/amd64` and `linux/arm64`, and the same for `:latest`. Check that `:latest` is really the new build, not only multi-arch: `docker buildx imagetools inspect ghcr.io/ysalitrynskyi/opn-backend:latest --format '{{ with (index .Image "linux/arm64") }}{{ index .Config.Labels "org.opencontainers.image.revision" }}{{ end }}'` must print the release commit.

Deployment to production is a manual Portainer redeploy by the operator; a green release workflow only means the images exist.

## Testing conventions

- Backend integration tests live in `backend/tests/*.rs` and use `common::spawn_real_app()` (real router + real Postgres via `axum_test::TestServer`). Write new tests this way — do not stub the router or hit a running server with shell scripts.
- For WebSocket/SSE tests use `common::spawn_real_app_ws()`, which builds the router over an HTTP transport (mock transport can't upgrade) with a real `WsState` so you can broadcast and observe what a `/ws` subscriber receives (see `tests/websocket_comprehensive_tests.rs`). Requires the `axum-test` `ws` feature.
- Tests run in parallel against one shared database: generate unique emails/codes via `common::unique_email()` / `unique_code()`, and don't assert on global counts.
- Frontend unit tests colocate as `*.test.tsx` next to the component (Vitest + Testing Library, jsdom).
- Playwright specs in `frontend/e2e/` run against the real stack. Use `e2e/support/api.ts`:
  `createUser` (registers through the API and verifies through the admin endpoint; there is no SMTP),
  `signIn(page, user)`, `createLink`, and `page.setExtraHTTPHeaders(clientIpHeader())` so a spec never
  spends the per-address rate-limit budget another spec asserts on. Mock with `page.route` only what
  cannot be produced for real (a 500, a network failure, GeoIP data), with a comment saying why. Never
  write `if (await el.isVisible()) { … }`: it passes when the element never renders, and dozens of
  the old specs could not fail because of it.
- Mutation-check new regression tests: break the production code, watch the test fail, restore.
