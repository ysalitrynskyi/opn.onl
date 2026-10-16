# opn.onl

A privacy-focused, open-source URL shortener built with Rust and React. Self-host your own link shortening service with full analytics, team collaboration, and comprehensive admin controls.

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](LICENSE)
[![Docker](https://img.shields.io/badge/Docker-Ready-blue)](docker-compose.yml)
[![Rust](https://img.shields.io/badge/Rust-1.98.1-orange)](backend/)
[![React](https://img.shields.io/badge/React-19-blue)](frontend/)

## Features

### Core Features
- **URL Shortening** - Create short links with custom aliases
- **Analytics** - Track clicks with geographic data, device info, referrers, browsers, and OS
- **Password Protection** - Secure links with passwords
- **Link Scheduling** - Set start dates and expiration for time-limited access
- **Click Limits** - Define maximum clicks per link
- **Burn After Reading** - Optional one-time / self-destruct links that disable themselves after they're opened. A `HEAD` request or a link-preview bot (Slack, Discord, WhatsApp and similar chat apps) never opens one, so pasting the link into a chat does not use it up
- **Safe-Link Interstitial** - Optional "you're leaving to X — looks safe ✓" destination preview with a reputation check, on links that opt in
- **Smart Conditional Routing** - Optionally route one short link to different destinations by device, OS, country or language, with weighted A/B splits
- **Link-in-Bio** - Privacy-first public profile page (`/@username`) aggregating your links — public only when you opt in: each user claims a username and enables their own page
- **API & MCP Server** - Personal API keys + an [official MCP server](#mcp-server) so AI assistants (Claude, etc.) can manage links — against opn.onl or your own self-hosted instance
- **Branded QR Codes** - Generate a QR for any link, with optional brand colour, centre logo and PNG/SVG export

### Organization & Management
- **Organizations** - Team workspaces with role-based access (owner, admin, editor, viewer; see [Organizations](#organizations)). A team's links open on the dashboard
- **Folders** - Organize links into folders with color coding, on the dashboard
- **Tags** - Add colored tags to categorize and filter links, on the dashboard
- **Bulk Operations** - Create, update, delete links in batches
- **CSV Export** - Export all your link data

### Security & Authentication
- **Email Verification** - Secure account verification flow
- **Password Reset** - Self-service password recovery via email
- **Passkey Authentication** - Passwordless login via WebAuthn/FIDO2
- **Rate Limiting** - Built-in protection against abuse (configurable)
- **JWT Authentication** - Secure token-based auth with expiration
- **API Keys** - Long-lived personal access tokens (`opn_…`) for the MCP server and programmatic API access

### Admin Features
- **Admin Dashboard** - Full statistics and management interface
- **User Management** - View, promote, demote, delete, restore users
- **Content Blocking** - Block specific URLs or entire domains
- **First User = Admin** - First registered user automatically becomes admin
- **Audit Log** - Record of organization changes: creation, settings, invitations, role changes, removals and ownership transfers (link, folder and tag changes are not logged)
- **Database Backups** - Scheduled backups to S3-compatible storage, with retention

### Performance
- **Redis Caching** - Optional caching layer for faster redirects
- **Click Buffering** - Batch click events for better DB performance
- **Real-time Updates** - WebSocket/SSE for live click notifications
- **GeoIP Lookup** - Country and city detection (optional MaxMind)
- **Database Indexes** - Composite and partial indexes on the redirect, listing and analytics paths

### Developer Experience
- **API Documentation** - OpenAPI/Swagger UI included at `/swagger-ui/`
- **Logging** - tracing's text format, on the console and in a daily log file
- **Health Checks** - Built-in health endpoint for monitoring
- **Docker Support** - Production-ready containers

## Tech Stack

| Layer | Technologies |
|-------|-------------|
| **Backend** | Rust, Axum, SeaORM, PostgreSQL, Redis |
| **Frontend** | React 19, TypeScript, Vite, Tailwind CSS |
| **Auth** | JWT, bcrypt, WebAuthn |
| **Deployment** | Docker, Nginx, Cloudflare Tunnel |

## Quick Start

### Prerequisites
- Docker & Docker Compose
- Rust 1.98.1 (pinned by `backend/rust-toolchain.toml` for local development)
- Node.js 22, or 20.19+ (for local development; `frontend/.nvmrc` pins 22)

### Development Setup

```bash
# Clone
git clone https://github.com/ysalitrynskyi/opn.onl.git
cd opn.onl

# Start PostgreSQL and Redis
docker-compose -f docker-compose.dev.yml up -d

# Backend setup
cd backend
cp .env.example .env
# The server refuses to start without a signing secret (JWT_SECRET is empty in
# the template); generate one into .env. The `.bak` suffix keeps this working
# with both GNU and BSD/macOS sed.
sed -i.bak "s|^JWT_SECRET=.*|JWT_SECRET=$(openssl rand -hex 32)|" .env && rm .env.bak
cargo run

# Frontend setup (new terminal)
cd frontend
cp .env.example .env
npm install
npm run dev
```

Open http://localhost:5173

**First user to register becomes admin!**

## Production Deployment

### Option 1: Docker Compose (Recommended)

#### 1. Setup Cloudflare Tunnel

1. Go to [Cloudflare Zero Trust](https://one.dash.cloudflare.com/) → Access → Tunnels
2. Create a new tunnel named `opn-onl`
3. Copy the tunnel token
4. Configure public hostnames:
   - `opn.onl` → `http://frontend:80`
   - `api.opn.onl` → `http://backend:3000` (or use `l.opn.onl` for shorter links)

> **Note:** You can use any subdomain for the API (e.g., `l.opn.onl`, `api.opn.onl`, `go.opn.onl`). 
> Short links work on both the main domain (`opn.onl/abc123`) and API subdomain (`l.opn.onl/abc123`).

#### 2. Configure Environment

```bash
cp .env.example .env
nano .env
```

**Required Variables:**
```env
# Database
POSTGRES_PASSWORD=your-secure-password-here

# Security — generate a unique value (placeholders are rejected at boot):
#   openssl rand -base64 64
JWT_SECRET=

# URLs
BASE_URL=https://api.opn.onl
FRONTEND_URL=https://opn.onl
VITE_API_URL=https://api.opn.onl
VITE_FRONTEND_URL=https://opn.onl

# Cloudflare Tunnel
CLOUDFLARE_TUNNEL_TOKEN=your-tunnel-token
```

#### 3. Deploy

```bash
docker-compose up -d --build
docker-compose logs -f
```

#### 4. Verify

```bash
curl https://api.opn.onl/health
# {"status":"healthy","version":"1.6.0","database":"connected","redis":"connected",...}
```

`status` is `healthy`, `degraded` or `unhealthy`. Only the database can fail the check (503, `unhealthy`).
Redis is optional: when `REDIS_URL` is set but Redis does not answer a PING, `redis` reads `disconnected`
and `status` is `degraded`, still with a 200 so the container is not restarted for it.

### Option 2: Portainer (Pre-built Images)

Docker images are automatically built by GitHub Actions on every push to `release`.

1. In Portainer: Stacks → Add Stack
2. Choose the appropriate file for your architecture:
   - `docker-compose.portainer.amd64.yml` - Intel/AMD servers
   - `docker-compose.portainer.arm64.yml` - ARM servers (Raspberry Pi, Apple Silicon, etc.)
3. Add environment variables (rolling `:latest`, or pin a release tag such as `:1.6.0`):
   ```
   BACKEND_IMAGE=ghcr.io/ysalitrynskyi/opn-backend:latest
   FRONTEND_IMAGE=ghcr.io/ysalitrynskyi/opn-frontend:latest
   ```
4. Deploy!

**Images:** `ghcr.io/ysalitrynskyi/opn-backend:latest`, `ghcr.io/ysalitrynskyi/opn-frontend:latest`  
**Platforms:** multi-arch `linux/amd64` + `linux/arm64` (built from `release` / version tags)

The published frontend image bakes `VITE_API_URL` at image-build time
(GitHub Actions defaults it to `https://l.opn.onl`). Unlike `GA_ID`, that
origin is not substituted when the container starts, so setting
`VITE_API_URL` in `.env` has no effect on `ghcr.io/ysalitrynskyi/opn-frontend`.
Those images are for the opn.onl deployment. For any other API host, build
the frontend yourself (`docker compose up -d --build` from Option 1, or
`docker build --build-arg VITE_API_URL=https://api.example.com frontend`).

## Environment Variables

### Required

| Variable | Description |
|----------|-------------|
| `POSTGRES_PASSWORD` | Database password |
| `JWT_SECRET` | Unique signing secret (≥32 bytes; known placeholders rejected). Generate with `openssl rand -base64 64` |
| `BASE_URL` | Public API URL (e.g., https://api.opn.onl) |
| `FRONTEND_URL` | Public frontend URL (e.g., https://opn.onl) |
| `CLOUDFLARE_TUNNEL_TOKEN` | Cloudflare tunnel token |

### Email (Required for verification & password reset)

| Variable | Default | Description |
|----------|---------|-------------|
| `SMTP_HOST` | - | SMTP server hostname |
| `SMTP_PORT` | 587 | SMTP port (587 for STARTTLS, 465 for SSL) |
| `SMTP_USER` | - | SMTP username |
| `SMTP_PASS` | - | SMTP password |
| `SMTP_TLS` | starttls | TLS mode: `starttls`, `tls`, or `none` |
| `SMTP_FROM_EMAIL` | noreply@opn.onl | From email address |
| `SMTP_FROM_NAME` | opn.onl | From display name (commas and quotes are fine) |
| `ADMIN_EMAIL` | - | Where contact-form messages are delivered. There is no default: while it is unset the contact form answers `503` and mails nobody |
| `EMAIL_RATE_LIMIT_PER_HOUR` | 500 | Most emails the instance sends per hour in total. Split 70% account mail (verification, password reset, welcome), 20% security notices (password changed) and 10% contact form, so a flood of one kind cannot starve the others |
| `REQUIRE_EMAIL_VERIFICATION` | true | Set to false only if your instance has no SMTP; anyone can then sign up with an address they do not own. |

Without SMTP, verification, password-reset and password-changed emails are not sent.
`POST /auth/resend-verification` then answers `503` (`Email is not configured on this instance`) for every caller, except a signed-in user whose own address is already verified. That refusal does not say whether the address has an account. `REQUIRE_EMAIL_VERIFICATION` stays on unless you set it false: missing SMTP does not turn it off, and accounts stay unable to create links, API keys or passkeys until you do. The contact form answers `503` ("not delivered") instead of accepting a message it has nowhere to send.
Every email carries a plain-text part alongside the HTML.

Mail that a request can trigger (`register`, `forgot-password`, `resend-verification`) is limited
per address and per client, on top of the hourly total. An address is sent at most one verification
email and one password-reset email every 5 minutes, and five of each a day. One client IP can
trigger at most 20 of these emails an hour. When email is configured, a request past a limit gets the same answer as one that
sent mail, sends nothing, and leaves the link already mailed valid, so the answers never reveal
whether an address has an account. A signed-in user asking for their own verification email is told
instead: `429` with `Retry-After`, or `503` when the server cannot send email. Client IPs follow
`TRUST_PROXY_HEADERS` and `REAL_IP_HEADER`; behind a reverse proxy set them, or every visitor shares
one budget. The limits are kept in memory, so a restart clears them.

### Link Management

Every on/off switch in this README (`ENABLE_*`, `BLOCK_*`, `FORCE_HTTPS`, `TRUST_PROXY_HEADERS`, `REQUIRE_EMAIL_VERIFICATION`)
accepts `true` or `false`, also written `1`/`0`, `yes`/`no` or `on`/`off`, in any case. Any other value
is ignored with a warning in the log and the default applies.

| Variable | Default | Description |
|----------|---------|-------------|
| `ENABLE_CUSTOM_ALIASES` | true | Allow users to create custom aliases |
| `MIN_ALIAS_LENGTH` | 5 | Minimum custom alias length |
| `MAX_ALIAS_LENGTH` | 25 | Maximum custom alias length. The Compose files and `.env.example` set 25; a backend started without the variable allows up to 50. A custom alias is letters, digits, hyphens and underscores, and cannot start or end with a hyphen or underscore |
| `ENABLE_URL_SANITIZATION` | true | Sanitize URLs for security |
| `BLOCK_DANGEROUS_FILE_EXTENSIONS` | true | Refuse to shorten links pointing directly at executable/script files (`.hta`, `.exe`, `.msi`, `.jar`, `.apk`, `.dmg`, …) — a common malware-delivery vector. Recommended on for any public instance |
| `BLOCK_RAW_IP_URLS` | true | Refuse to shorten links whose host is a bare IP address. Set `false` if you legitimately shorten internal/LAN IPs on a self-hosted instance |
| `ENABLE_ACCOUNT_DELETION` | false | Allow users to delete their own accounts. The account closes at once; see `ACCOUNT_PURGE_DAYS` for when its data is removed |

### Privacy & Analytics

| Variable | Default | Description |
|----------|---------|-------------|
| `ANALYTICS_PII_RETENTION_DAYS` | 396 | Days before per-visitor click identifiers (truncated IP, user agent) and the truncated IP on organization audit entries are anonymized by a daily sweep; aggregate stats and the audit entries themselves are kept. `0` disables the sweep |
| `ACCOUNT_PURGE_DAYS` | 30 | Days a deleted account (deleted by its owner or an admin) is kept before a daily sweep removes it for good: the user, their links with URLs, titles and notes, click history, passkeys and API keys. An admin can restore the account until then, not after. Team links stay with their organization. An account that still owns an organization with other members is kept until ownership moves. A link deleted on its own (by its owner, or by an admin) is removed for good after the same window, with its click history, tags and routing rules; its short code stays reserved. `0` disables both |

Click analytics never store the full visitor IP: addresses are truncated at
collection (IPv4 to /24, IPv6 to /48) after an in-memory, local GeoIP city
lookup — no IP ever leaves the server.

Google Analytics is separate and off unless you set `GA_ID`. When it is set, the
consent banner gates it in the direction `GA_CONSENT_MODE` selects: `opt-in`
(default) loads GA only after the visitor accepts, `opt-out` loads it on the
first page view and stops on Decline. Global Privacy Control blocks GA in both
directions; Do-Not-Track is honored in `opt-in` only. Advertising signals are
denied in both. Pick the direction your jurisdiction allows, and keep the
privacy policy in sync — the bundled one describes whichever mode is active.

### Performance Tuning

| Variable | Default | Description |
|----------|---------|-------------|
| `REDIS_URL` | - | Redis connection URL |
| `REDIS_CACHE_TTL` | 300 | Cache TTL in seconds |
| `CLICK_BUFFER_SIZE` | 100 | Click events before DB flush |
| `CLICK_FLUSH_INTERVAL` | 10 | Flush interval in seconds (10 in the shipped compose files and `.env.example`; the backend's own default when unset is 5) |

### Backups (S3-compatible)

| Variable | Description |
|----------|-------------|
| `BACKUP_S3_ENDPOINT` | S3 endpoint (e.g., Cloudflare R2) |
| `BACKUP_S3_BUCKET` | S3 bucket name |
| `BACKUP_S3_ACCESS_KEY` | S3 access key |
| `BACKUP_S3_SECRET_KEY` | S3 secret key |
| `BACKUP_S3_REGION` | S3 region (default: auto) |
| `BACKUP_INTERVAL_HOURS` | Hours between automatic backups (default: 24; `0` turns them off) |
| `BACKUP_KEEP` | Automatic backups kept; after each automatic backup all but the newest N automatic backups are deleted (default: 14, minimum 1). Backups taken by hand are never deleted by the schedule |

### Analytics (Optional)

| Variable | Description |
|----------|-------------|
| `GA_ID` | Google Analytics Measurement ID (e.g., G-XXXXXXXXXX) - Runtime ENV |
| `GA_CONSENT_MODE` | Consent banner direction: `opt-in` (default - GA loads only after Accept) or `opt-out` (GA loads immediately, stops on Decline). Global Privacy Control is honored in both; Do-Not-Track only in `opt-in`. Runtime ENV |
| `MAXMIND_ACCOUNT_ID` | MaxMind account ID for GeoIP |
| `MAXMIND_LICENSE_KEY` | MaxMind license key for GeoIP |

### Other

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | info | Log level (trace, debug, info, warn, error). At `debug`, `tower_http` logs two lines for every request |
| `LOG_MAX_FILES` | 14 | Daily log files kept in the backend's logs volume; older ones are deleted. `0` keeps all of them. Docker's own container logs are rotated by the compose files (10 MB x 5 per service) |
| `WS_MAX_CONNECTIONS_PER_USER` | 5 | Open `/ws` and `/sse` connections one account may hold at once, together; more are refused with `429`. `0` lifts the limit |
| `TRUST_PROXY_HEADERS` | true (compose) | Take the client IP from proxy headers. Leave on behind Cloudflare or a proxy you control; off, every visitor behind a proxy shares one rate-limit bucket |
| `REAL_IP_HEADER` | cf-connecting-ip | Header the trusted edge sets to the real client IP. **Not behind Cloudflare? Set `REAL_IP_HEADER=none`**, or name the header your own proxy overwrites (for example `x-real-ip`). An empty value does not disable it in the compose files: they substitute the default, and any client can forge `CF-Connecting-IP` when Cloudflare is not in front |
| `TRUSTED_PROXY_HOPS` | 0 | `X-Forwarded-For` fallback when `REAL_IP_HEADER` is `none` or absent from the request: trusted proxies in front of the app (0 uses the right-most entry) |
| `POSTGRES_HOST_PORT` | 5433 | Host port for the loopback-published postgres in the arm64 Portainer compose. 5432 is often already taken on a host running more than one postgres. Bound to 127.0.0.1 only |
| `FORCE_HTTPS` | true | Redirect plain HTTP to HTTPS, going by `X-Forwarded-Proto`. The Compose files default it to `true`; a backend started without the variable does not redirect. `/health` is exempt: the container health check asks over plain HTTP and must see the real answer |
| `WEBAUTHN_RP_ID` | (from FRONTEND_URL) | WebAuthn Relying Party ID |

## API Reference

Full API documentation available at `/swagger-ui/` when backend is running.

Timestamps in responses are RFC 3339 in UTC, for example `2026-11-15T04:59:00Z`.

Errors come in a few shapes, so a generic client should look at the status code first:

- Most endpoints answer `{"error": "..."}`. A `429` from the rate limiter adds `retry_after` and
  `message` next to `error`.
- `/admin/*` and `POST /contact` answer `{"success": false, "message": "..."}`.
- `POST /links/bulk` answers a refusal as `{"links": [], "errors": ["..."]}`.
- A request body, query string or path id that does not parse (malformed JSON, a wrong type, a
  non-numeric id) is refused before the handler runs, with status `400`, `415` or `422` and a
  plain-text reason.
- `GET /{code}` answers a missing or dead link with a small HTML page when the client accepts
  `text/html`, and with the plain-text reason otherwise. `/sse` and `/api/bio/avatar` also answer
  errors as plain text.

### Authentication

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/auth/register` | Register new account |
| POST | `/auth/login` | Login with email/password |
| POST | `/auth/verify-email` | Verify email with token |
| POST | `/auth/resend-verification` | Resend verification email |
| POST | `/auth/forgot-password` | Request password reset |
| POST | `/auth/reset-password` | Reset password with token |
| POST | `/auth/change-password` | Change password (authenticated). Signs out every session, so the response carries a fresh `token` to keep using |
| POST | `/auth/delete-account` | Delete own account (if enabled; refused with 409 while you own orgs with other members). The account closes at once and is removed for good after `ACCOUNT_PURGE_DAYS` |
| GET / POST | `/auth/api-keys` | List / create personal API keys |
| DELETE | `/auth/api-keys/{id}` | Revoke an API key |
| POST | `/auth/passkey/register/start` · `/finish` | Enroll a passkey for the authenticated account (WebAuthn; requires `ENABLE_PASSKEYS`) |
| POST | `/auth/passkey/login/start` · `/finish` | Passwordless login with a registered passkey |
| GET | `/auth/passkeys` | List your registered passkeys |
| POST | `/auth/passkey/delete` · `/auth/passkey/rename` | Remove or rename a passkey |

Authenticate any request with `Authorization: Bearer <token>` — either a **JWT** (from `/auth/login`) or a personal **API key** (`opn_…`, created in **Settings → API Keys**). API keys are for scripts and the [MCP server](#mcp-server). Create accepts optional `scope` (`full`, the default, or `read_only`) and `expires_in_days` (1–730); a read-only key may only send GET, HEAD and OPTIONS, and a key with no lifetime lasts until you revoke it. Responses add `scope`, `expires_at` (RFC 3339 UTC, or null) and `expired`.

### Links

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/links` | List your links, or with `?org_id=` every link of that organization (members only). Pinned links first, then newest first. Filters: `folder_id`, `tag_id`, `search`. Pages with `limit` (default and maximum 1000) and `offset`; the body is a JSON array and the `X-Total-Count` response header carries how many links match, ignoring `limit`/`offset` |
| POST | `/links` | Create new link (`custom_alias` sets the code). Without an `Authorization` header the link is anonymous; a header that does not authenticate (expired, revoked or unknown token) is a 401, not an anonymous link |
| GET | `/links/{id}` | Get one link, in the same shape as an item of `GET /links`. Organization links: any current member. A missing, deleted or someone else's link is a `403` |
| PUT | `/links/{id}` | Update link. The short code cannot be changed. Only the fields you send change; `null` counts as not sent, so clear a field with its flag: `remove_expiration`, `remove_starts_at`, `remove_max_clicks`, `remove_password`, `remove_folder`. An empty `title` or `notes` clears it. A burn-after-reading link always has a click cap (1 unless you set another): turning burn off removes the cap it implied, and `remove_max_clicks` on a burn link turns burn off |
| POST | `/links/{id}/clone` | Copy a link under a new code: destination, password, schedule, click cap, burn-after-reading, interstitial, folder, tags and routing rules. Clicks start at zero and the copy is not pinned |
| DELETE | `/links/{id}` | Delete link (organization links: org admins and above, or the creator while still an editor) |
| GET | `/links/{id}/qr` | Get QR code image. Optional query: `color` (hex), `bg` (hex), `logo` (`true`), `format` (`png`\|`svg`), `size` (256–1024) |
| GET / PUT | `/links/{id}/rules` | List or replace smart-routing rules (requires `ENABLE_CONDITIONAL_ROUTING`) |
| GET | `/links/{id}/stats` | Get link analytics |
| POST | `/links/bulk` | Create multiple links |
| POST | `/links/bulk/delete` | Delete multiple links |
| POST | `/links/bulk/update` | Update multiple links: `folder_id` or `remove_folder`, `expires_at` or `remove_expiration`. Links you may not edit are skipped, and so are links whose new `expires_at` would not be after their own `starts_at` |
| GET | `/links/export` | Export links as CSV, UTF-8 with a byte order mark and RFC 3339 timestamps (`?org_id=` for an organization's links, members only). A database error is a 500, never a header-only file |

### Redirects

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET, HEAD | `/{code}` | Temporary redirect (`307`) to the destination. A password-protected link with no `X-Link-Password` header and no valid `unlock` goes to the password page (`{FRONTEND_URL}/password/{code}`), not a `401`. A link behind the safe-link interstitial goes to `{FRONTEND_URL}/r/{code}` until `confirm=1`, including a `HEAD`. A `401` is only a wrong `X-Link-Password`. On a one-time or click-capped link that is neither password-protected nor behind the interstitial, a `HEAD` and a `GET` from a known link-preview bot get a `200` without the destination and use nothing up. A `HEAD` never counts as a click |
| GET | `/{code}/preview` | Link preview + safety/reputation signal (append `+` to any short link). The destination is left out (empty `original_url` and `domain`) for a link that is not live, is password-protected, is one-time, or has several destinations (a weighted A/B split has no single answer). `inactive_reason` says why a link is not live (`not_started`, `expired`, `used_up` or `blocked`) and is `null` while it is |
| POST | `/{code}/verify` | Verify a password-protected link. A correct password answers `{"redirect_url", "expires_in_seconds"}`; open `redirect_url` (it carries a short-lived `?unlock=` proof) to be redirected |

### Link-in-Bio

| Method | Endpoint | Description |
|--------|----------|-------------|
| PUT | `/auth/bio` | Update your link-in-bio settings (username, enabled, theme) — requires `ENABLE_LINK_IN_BIO` |
| GET | `/api/bio/{username}` | Public bio profile (only when the feature and the user's page are enabled) |
| GET | `/api/bio/avatar?url=…` | Server-side image proxy for bio avatars, so a visitor's browser never hits the external avatar host (SSRF-guarded, inert-raster images only) |

### Organizations

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/orgs` | List user's organizations |
| POST | `/orgs` | Create organization |
| GET | `/orgs/{org_id}` | Get organization |
| PUT | `/orgs/{org_id}` | Update organization |
| DELETE | `/orgs/{org_id}` | Delete organization (owner only; needs a sign-in session, an API key gets 403) |
| GET | `/orgs/{org_id}/members` | List members |
| POST | `/orgs/{org_id}/members` | Invite member by email (`role`: admin, editor, viewer) |
| PUT | `/orgs/{org_id}/members/{member_id}` | Change a member's role |
| DELETE | `/orgs/{org_id}/members/{member_id}` | Remove member |
| POST | `/orgs/{org_id}/leave` | Leave an organization (any member except the owner, who gets 409 until ownership is transferred or the organization deleted). Links you created stay with the organization |
| POST | `/orgs/{org_id}/transfer-ownership` | Transfer ownership to another member (owner only; needs a sign-in session, an API key gets 403; body: `{"new_owner_user_id": <id>}`) |
| GET | `/orgs/{org_id}/audit` | View audit log (admin+) |

What a member may do depends on their current role. The creator of a link gets nothing extra
from having made it, except that an editor may delete their own.

| Role | Links | Folders and tags | The organization |
|------|-------|------------------|------------------|
| `owner` | Read, create, edit and delete any | Read, create, edit and delete | Everything an admin can, plus delete the organization and transfer ownership |
| `admin` | Read, create, edit and delete any | Read, create, edit and delete | Rename it, invite members (as admin, editor or viewer), change roles, remove members, read the audit log |
| `editor` | Read, create and edit any; delete only the ones they created | Read, create, edit and delete | View the member list |
| `viewer` | Read only (stats and QR codes included) | Read only | View the member list |

An account that owns organizations with other members cannot be deleted
(HTTP 409 with the list of blocking orgs) until ownership is transferred or
the organization is deleted — one person leaving can't wipe a shared team.

Deleting an organization (or an admin hard-deleting a user) removes the links
for good, but their short codes stay reserved: a retired code is never issued
again, as a custom alias or as a random code, so an old printed link can't end
up pointing at someone else's page.

### Folders

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/folders` | List folders |
| POST | `/folders` | Create folder |
| GET | `/folders/{id}` | Get folder |
| PUT | `/folders/{id}` | Update folder |
| DELETE | `/folders/{id}` | Delete folder |
| GET | `/folders/{id}/links` | Get links in folder, newest first. Takes `limit` (default and maximum 1000) and `offset`; `X-Total-Count` carries the folder's total |
| POST | `/folders/{id}/links` | Move links to folder |

### Tags

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/tags` | List tags |
| POST | `/tags` | Create tag |
| PUT | `/tags/{id}` | Update tag |
| DELETE | `/tags/{id}` | Delete tag |
| GET | `/tags/{id}/links` | Get links with tag, newest first. Takes `limit` (default and maximum 1000) and `offset`; `X-Total-Count` carries the tag's total |
| POST | `/links/{id}/tags` | Add tags to link |
| DELETE | `/links/{id}/tags` | Remove tags from link |

### Admin (Admin only)

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/admin/stats` | Get system statistics |
| GET | `/admin/activity` | Daily signups/links/clicks timeseries (`?days=30`) |
| GET | `/admin/users` | List all users — paginated, searchable, with per-user link/click/key/org counts |
| DELETE | `/admin/users/{id}` | Soft delete user (their links, and the links of organizations they are the only member of, stop redirecting); removed for good after `ACCOUNT_PURGE_DAYS`, the list shows `purge_at` |
| DELETE | `/admin/users/{id}/hard` | Permanently delete user |
| POST | `/admin/users/{id}/restore` | Restore deleted user and the links that deletion took down (409 if the email was registered again meanwhile; 404 once the account has been purged) |
| POST | `/admin/users/{id}/make-admin` | Promote to admin |
| POST | `/admin/users/{id}/remove-admin` | Demote from admin |
| POST | `/admin/users/{id}/verify-email` | Force-verify a user's email |
| GET | `/admin/links` | List every link across all users — paginated, searchable (code/URL/title/owner email), filter by user, deleted status, or `suspicious=true` (dangerous file type / raw-IP host); each row carries a `suspicious` flag + reason |
| DELETE | `/admin/links/{id}` | Soft delete any link (takedown; stops redirecting immediately) |
| POST | `/admin/links/{id}/restore` | Restore a deleted link |
| POST | `/admin/links/bulk/delete` | Soft delete many links at once (body: `{"ids":[…]}`) |
| POST | `/admin/links/bulk/restore` | Restore many links at once |
| POST | `/admin/links/{id}/block-domain` | One-click takedown: block the link's destination host and delete the link |
| GET | `/admin/orgs` | List all organizations with owner, member and link counts |
| GET | `/admin/blocked/links` | List blocked URLs |
| POST | `/admin/blocked/links` | Block URL |
| DELETE | `/admin/blocked/links/{id}` | Unblock URL |
| GET | `/admin/blocked/domains` | List blocked domains |
| POST | `/admin/blocked/domains` | Block domain |
| DELETE | `/admin/blocked/domains/{id}` | Unblock domain |
| POST | `/admin/backup` | Create database backup |
| GET | `/admin/backup` | List backups |
| DELETE | `/admin/backup/cleanup/{keep}` | Delete all but the newest `keep` backups, automatic and manual together (`keep` must be at least 1) |

### Other

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/health` | Health check, including the running `version` |
| POST | `/contact` | Contact form |
| GET | `/analytics/dashboard` | User analytics dashboard |

## MCP Server

opn.onl has an official [Model Context Protocol](https://modelcontextprotocol.io) server, so AI assistants (Claude Desktop, Cursor, etc.) can shorten links, read analytics and manage links in natural language — against the hosted service **or your own self-hosted instance**.

[![opn-mcp MCP server](https://glama.ai/mcp/servers/ysalitrynskyi/opn-mcp/badges/score.svg)](https://glama.ai/mcp/servers/ysalitrynskyi/opn-mcp)

**Repo:** [github.com/ysalitrynskyi/opn-mcp](https://github.com/ysalitrynskyi/opn-mcp)

1. Create an API key in **Settings → API Keys**.
2. Add the server to your MCP client config (Claude Desktop shown):

```json
{
  "mcpServers": {
    "opn": {
      "command": "npx",
      "args": ["-y", "opn-mcp"],
      "env": {
        "OPN_API_KEY": "opn_your_key_here",
        "OPN_BASE_URL": "https://l.opn.onl"
      }
    }
  }
}
```

Set `OPN_BASE_URL` to your own instance's API to use a **self-hosted copy** (omit it to default to the hosted `https://l.opn.onl`).

**Tools:** `shorten_url`, `list_links`, `get_link_stats`, `update_link`, `delete_link`, `get_qr_code`, `check_url_health`.

## Project Structure

```
opn.onl/
├── backend/
│   ├── src/
│   │   ├── entity/       # SeaORM database models
│   │   ├── handlers/     # API route handlers
│   │   ├── utils/        # Utilities (cache, jwt, geoip, email, backup, privacy)
│   │   ├── lib.rs        # Library target: AppState + build_router (used by tests)
│   │   └── main.rs       # Thin binary: env, logging, DB, serve
│   ├── migration/        # Database migrations
│   ├── scripts/          # Helper scripts (GeoIP download, entrypoint)
│   ├── tests/            # Integration tests (real router + real Postgres)
│   └── Dockerfile
├── frontend/
│   ├── src/
│   │   ├── components/   # Reusable UI components
│   │   ├── pages/        # Page components
│   │   ├── config/       # API configuration
│   │   └── test/         # Test utilities
│   ├── public/           # Static assets
│   ├── e2e/              # Playwright E2E tests
│   ├── scripts/          # Build scripts
│   ├── Dockerfile
│   └── nginx.conf
├── .github/
│   └── workflows/        # GitHub Actions CI/CD
├── docker-compose.yml          # Production deployment
├── docker-compose.dev.yml      # Development (DB + Redis only)
├── docker-compose.portainer.amd64.yml  # Portainer (Intel/AMD)
├── docker-compose.portainer.arm64.yml  # Portainer (ARM)
└── .env.example                # Environment template
```

## GeoIP Analytics (Optional)

Enable country/city detection for click analytics:

### Docker (Automatic)
1. Create free account at [MaxMind](https://www.maxmind.com/en/geolite2/signup)
2. Generate license key: Account → Manage License Keys
3. Add to `.env`:
   ```
   MAXMIND_ACCOUNT_ID=123456
   MAXMIND_LICENSE_KEY=your-license-key
   ```
4. Database downloads automatically on container start

### Local Development
1. Download `GeoLite2-City.mmdb` from MaxMind
2. Place in `backend/data/GeoLite2-City.mmdb`

**The app works without GeoIP** - location analytics will just be empty.

## Backup & Restore

### Manual Backup

```bash
# Backup
docker exec opn_onl_db pg_dump -U postgres opn_onl > backup.sql

# Restore
docker exec -i opn_onl_db psql -U postgres opn_onl < backup.sql
```

### Automated Backups

Configure S3-compatible storage in `.env`:

```env
BACKUP_S3_ENDPOINT=https://your-account-id.r2.cloudflarestorage.com
BACKUP_S3_BUCKET=opn-backups
BACKUP_S3_ACCESS_KEY=your-key
BACKUP_S3_SECRET_KEY=your-secret
```

With all four `BACKUP_S3_*` values set, the backend runs `pg_dump` on a schedule and uploads the gzipped dump as `backup_auto_YYYYMMDD_HHMMSS.sql.gz` (UTC). By default that is every 24 hours (`BACKUP_INTERVAL_HOURS`; `0` turns it off), and after each successful automatic backup everything but the newest 14 automatic backups (`BACKUP_KEEP`) is deleted. The first automatic backup runs one interval after the backend starts, not at startup, so a container that keeps restarting does not dump the database on every boot.

A backup taken from the Admin panel or with `POST /admin/backup` is named `backup_YYYYMMDD_HHMMSS.sql.gz` and is never deleted by the schedule; only `DELETE /admin/backup/cleanup/{keep}` removes it. That endpoint is a deliberate admin action and counts automatic and manual backups together. Only objects named like these are ever listed or deleted, so the bucket can hold other files. The Admin panel shows the schedule and the existing backups, automatic ones marked.

Backups are not encrypted by the application. They travel to the storage endpoint over TLS when the endpoint is an `https://` URL, and they are encrypted at rest only if the storage provider does that (Cloudflare R2 always does). Use a bucket that is private and encrypted at rest.

## Security Features

### Rate Limiting
- **Per-second:** 10 requests/second per IP
- **General:** 100 requests/minute per IP
- **Link creation:** 100 links/hour per IP (`POST /links` and clone; a bulk create spends one per URL)
- **Sign-in and other auth writes:** 10/minute per IP (`POST /auth/*`; reads such as `/auth/me`
  use the general limit)
- **Password verification:** 5 attempts/minute per IP+link, 20/minute per IP across links
  (anti-bruteforce)
- **Same URL:** 10 times per 10 minutes per user (the one limit counted per account, not per IP)
- **Redirects:** 100/second per IP

A refused request gets `429` with `Retry-After` (seconds, rounded up) and `X-RateLimit-Limit` /
`X-RateLimit-Remaining`; all three are readable by cross-origin clients.

### Content Blocking
Admins can block:
- **Specific URLs:** Block exact malicious URLs
- **Entire domains:** Block all links to a domain (including subdomains)

Blocked content cannot be shortened via any endpoint (single, bulk, API).

### Data Protection
- Passwords hashed with bcrypt, off the request threads; new passwords are limited to 72 bytes,
  bcrypt's input limit
- Email verification and password-reset tokens stored only as SHA-256 hashes
- JWT tokens with expiration
- Soft-delete for links and users; deleted accounts and deleted links are removed for good after
  a grace window (`ACCOUNT_PURGE_DAYS`, default 30 days)
- Email verification required before creating links
- The account owner is emailed when their password is changed or reset
- HTTPS enforced in production; the API sends `X-Content-Type-Options`, `X-Frame-Options`, and HSTS
  behind an HTTPS proxy
- URL sanitization and blocking
- Visitor IPs truncated before storage (IPv4 /24, IPv6 /48); remaining
  click identifiers, and the truncated address on organization audit entries,
  anonymized after a configurable retention window
  (`ANALYTICS_PII_RETENTION_DAYS`, default ~13 months)

## Development

### Backend

```bash
cd backend
cargo run          # Dev server on :3000
cargo test         # Run tests (needs Postgres, see below)
cargo clippy       # Lint
cargo build --release
```

The test suite includes real integration tests that exercise the actual
router against Postgres. Point `DATABASE_URL` at a throwaway database
(migrations run automatically):

```bash
createdb opn_test
DATABASE_URL=postgres://localhost/opn_test cargo test
```

### Frontend

```bash
cd frontend
npm run dev        # Dev server on :5173
npm run test       # Unit tests (Vitest)
npm run test:e2e:ci  # E2E tests (Playwright, Chromium), against a running backend
npm run build      # Production build
npm run lint       # ESLint
```

The E2E suite needs the backend running on a fresh database; see
[frontend/e2e/README.md](frontend/e2e/README.md).

### Database Migrations

Migrations run automatically every time the backend starts (`cargo run`, or the container), and
the test suite runs them on its first connection. There is no separate migrate command: the
binary takes no arguments.

## Troubleshooting

### Container Health Checks Failing
```bash
# Check backend health (the ports are not published, so ask from inside the
# container; a non-200 answer means the database is unreachable)
docker-compose exec backend curl -fsS http://localhost:3000/health

# Check frontend health
docker-compose exec frontend curl -fsS http://localhost:80/health

# View logs
docker-compose logs -f backend
docker-compose logs -f frontend
```

### "exec format error" in Docker
Multi-arch image not pulled correctly:
```bash
docker rmi ghcr.io/ysalitrynskyi/opn-backend:latest
docker pull --platform linux/amd64 ghcr.io/ysalitrynskyi/opn-backend:latest
```

### Email Not Sending
- Check SMTP credentials in `.env`
- Port 587 uses STARTTLS, port 465 uses SSL/TLS
- Check backend logs for SMTP errors

### Rate Limited
- Default: 10 requests/second, 100/minute
- Adjust in code or wait for cooldown

## Contributing

1. Fork the repo
2. Create a feature branch (`git checkout -b feature/amazing`)
3. Commit changes (`git commit -m 'Add amazing feature'`)
4. Push to branch (`git push origin feature/amazing`)
5. Open a Pull Request

## Security

Found a vulnerability? Please report it privately — see [SECURITY.md](SECURITY.md).

## License

This project is licensed under the AGPL-3.0 License - see the [LICENSE](LICENSE) file for details.

**AGPL-3.0 requires:**
- Open source any modifications you deploy
- Provide source code to users of your service
- Keep the same license for derivatives

## Disclaimer

opn.onl is not responsible for content accessible through shortened links. We actively remove malicious links when reported but cannot verify all content. Report abuse to abuse@opn.onl.

---

Made with ❤️ by [ysalitrynskyi](https://github.com/ysalitrynskyi)

## 💖 Support

If you find opn.onl useful, please consider supporting its development:

[![GitHub Sponsors](https://img.shields.io/badge/Sponsor-%E2%9D%A4-pink?logo=github)](https://github.com/sponsors/ysalitrynskyi)

Your sponsorship helps cover hosting costs and enables continued development of new features!
