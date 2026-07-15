# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Machine context (this host)

Cross-repo / MCP / disk / Azure / efficiency unlocks:
`~/work/AGENTS.md` (walk-up discovery; **this file does not replace it**).

- Registry: `~/.config/agent-coordination/AGENT-REGISTRY.md`
- Runbooks (on demand): `~/work/_runbooks/` - azure-models, disk-cleanup, deploy-verify, ...
- **SSH:** never connect without operator OK naming exact host — `~/work/_runbooks/ssh-and-hosts.md` (many servers; do not guess).
- **Efficiency unlocks:** fast env help -> DIY if safe, or ask operator once (what / why / ~sec). Not invent-scope.
- **Azure:** credit cutoff ~2026-07-20 -> ask before any Azure call after that (`_runbooks/azure-models.md`).

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
npm run test:e2e   # Playwright E2E
```

## Architecture

### Backend

- **`src/lib.rs` is the single source of truth for the app**: `AppState` + `build_router()`. The binary (`src/main.rs`) is a thin wrapper: env, logging, DB connect, migrations, serve. Integration tests import the real router via the lib target (`opn_onl_backend`). `build_router` must stay byte-for-byte what the binary serves; middleware order (with_state → https_redirect → rate limit → CORS → tracing) must not be reordered.
- **`src/handlers/`** — one module per domain (auth, links, analytics, admin, organizations, folders, tags, passkeys, api_keys, bio, websocket, contact). Handlers do their own auth: they parse the `Authorization: Bearer` header themselves (JWT via `utils::decode_jwt`, or `opn_…` API keys); there is no auth middleware/extractor layer. Admin handlers gate on `require_admin` in `handlers/admin.rs` (checks `is_admin` and excludes soft-deleted users).
- **`src/entity/`** — SeaORM models. **Soft delete is the norm**: `users` and `links` have `deleted_at`; most list queries must filter `DeletedAt.is_null()`. Soft delete is an UPDATE, so FK `ON DELETE CASCADE` does not fire — related cleanup (e.g. passkeys on user delete) must be done explicitly.
- **`migration/`** — SeaORM migration crate; migrations run automatically on startup and on first test-suite connect.
- **`utils/`** — `ClickBuffer` (batches click events before DB flush), `RedisCache` (optional redirect cache — handlers that change link state must invalidate it or blocks/edits take up to the TTL to apply; use `links::invalidate_cached_codes` / `active_link_codes_for_user`), `EmailService` (optional; unset SMTP = emails silently skipped), `BackupService` (S3; optional), rate limiters, JWT, GeoIP, privacy sweep (IP truncation at collection, retention anonymization; referer stored host-only; `purge_click_pii_for_user` on account delete). `RateLimiters` lives on `AppState` (shared by the rate-limit middleware and handlers, e.g. the redirect password path enforces the `password_verify` limiter in-handler). Middleware classifies redirect vs API by route prefix, not path length.
- **Auth/roles**: single `is_admin` flag on users (no role table). First registered user becomes admin (`ensure_admin_exists`). `token_version` on users invalidates old JWTs on credential change.
- **Route order matters**: `/:code` redirect routes are registered last so they don't shadow API routes.
- API docs generated via utoipa; new handlers should carry `#[utoipa::path]` annotations and be registered in `src/openapi.rs`.

### Frontend

- SPA with react-router (`src/App.tsx`). Marketing pages are **eagerly imported** so the production build prerenders them to static HTML (SEO); app pages (Dashboard, Admin, Settings…) are lazy-loaded and not prerendered.
- **Adding a page route requires touching up to three places**: `src/App.tsx` (route), `vite.config.ts` `PRERENDER_ROUTES` (only if it should be prerendered), and `frontend/nginx.conf` SPA-route allowlist regex (production only — anything not on that allowlist matching `/[a-zA-Z0-9]{4,50}` is proxied to the backend as a short-link code and will 404 as a page).
- API access goes through `src/config/api.ts` (`API_ENDPOINTS`, `authFetch` — reads JWT from localStorage). No state-management library; pages fetch directly.
- Prerender in Docker must use Playwright/Puppeteer's bundled Chromium — Debian's apt chromium SIGTRAPs in the builder container (see vite.config.ts sandbox flags).

### Deployment

- Production: docker-compose + Cloudflare Tunnel; frontend nginx proxies `/{code}` redirects and `/{code}/verify|preview` to the backend, serves prerendered HTML for static routes, and falls back to the SPA shell.
- Images are built by GitHub Actions on push to the `release` branch (`ghcr.io/ysalitrynskyi/opn-{backend,frontend}`); Portainer compose files consume them.

## Testing conventions

- Backend integration tests live in `backend/tests/*.rs` and use `common::spawn_real_app()` (real router + real Postgres via `axum_test::TestServer`). Write new tests this way — do not stub the router or hit a running server with shell scripts.
- For WebSocket/SSE tests use `common::spawn_real_app_ws()`, which builds the router over an HTTP transport (mock transport can't upgrade) with a real `WsState` so you can broadcast and observe what a `/ws` subscriber receives (see `tests/websocket_comprehensive_tests.rs`). Requires the `axum-test` `ws` feature.
- Tests run in parallel against one shared database: generate unique emails/codes via `common::unique_email()` / `unique_code()`, and don't assert on global counts.
- Frontend unit tests colocate as `*.test.tsx` next to the component (Vitest + Testing Library, jsdom).
