# opn.onl

A privacy-focused, open-source URL shortener built with Rust and React. Self-host your own link shortening service with full analytics, team collaboration, and comprehensive admin controls.

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-blue.svg)](LICENSE)
[![Docker](https://img.shields.io/badge/Docker-Ready-blue)](docker-compose.yml)
[![Rust](https://img.shields.io/badge/Rust-1.85+-orange)](backend/)
[![React](https://img.shields.io/badge/React-19-blue)](frontend/)

## Features

### Core Features
- **URL Shortening** - Create short links with custom aliases
- **Analytics** - Track clicks with geographic data, device info, referrers, browsers, and OS
- **Password Protection** - Secure links with passwords
- **Link Scheduling** - Set start dates and expiration for time-limited access
- **Click Limits** - Define maximum clicks per link
- **QR Code Generation** - Generate QR codes for any link

### Organization & Management
- **Organizations** - Team workspaces with role-based access (owner, admin, member, viewer)
- **Folders** - Organize links into folders with color coding
- **Tags** - Add colored tags to categorize links
- **Bulk Operations** - Create, update, delete links in batches
- **CSV Export** - Export all your link data

### Security & Authentication
- **Email Verification** - Secure account verification flow
- **Password Reset** - Self-service password recovery via email
- **Passkey Authentication** - Passwordless login via WebAuthn/FIDO2
- **Rate Limiting** - Built-in protection against abuse (configurable)
- **JWT Authentication** - Secure token-based auth with expiration

### Admin Features
- **Admin Dashboard** - Full statistics and management interface
- **User Management** - View, promote, demote, delete, restore users
- **Content Blocking** - Block specific URLs or entire domains
- **First User = Admin** - First registered user automatically becomes admin
- **Audit Log** - Track all organization activities
- **Database Backups** - Automated S3-compatible backup support

### Performance
- **Redis Caching** - Optional caching layer for faster redirects
- **Click Buffering** - Batch click events for better DB performance
- **Real-time Updates** - WebSocket/SSE for live click notifications
- **GeoIP Lookup** - Country and city detection (optional MaxMind)

### Developer Experience
- **API Documentation** - OpenAPI/Swagger UI included at `/swagger-ui/`
- **Structured Logging** - JSON logs with tracing
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
- Rust 1.85+ (for local development)
- Node.js 20+ (for local development)

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

# Security (generate with: openssl rand -base64 64)
JWT_SECRET=your-very-long-random-jwt-secret-min-32-chars

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
# {"status":"healthy","database":"connected","redis":"connected",...}
```

### Option 2: Portainer (Pre-built Images)

Docker images are automatically built by GitHub Actions on every push to `release`.

1. In Portainer: Stacks → Add Stack
2. Choose the appropriate file for your architecture:
   - `docker-compose.portainer.amd64.yml` - Intel/AMD servers
   - `docker-compose.portainer.arm64.yml` - ARM servers (Raspberry Pi, Apple Silicon, etc.)
3. Add environment variables:
   ```
   BACKEND_IMAGE=ghcr.io/ysalitrynskyi/opn-backend:latest
   FRONTEND_IMAGE=ghcr.io/ysalitrynskyi/opn-frontend:latest
   ```
4. Deploy!

**Images:** `ghcr.io/ysalitrynskyi/opn-backend:latest`, `ghcr.io/ysalitrynskyi/opn-frontend:latest`  
**Platforms:** `linux/amd64`, `linux/arm64`

## Environment Variables

### Required

| Variable | Description |
|----------|-------------|
| `POSTGRES_PASSWORD` | Database password |
| `JWT_SECRET` | Secret for JWT tokens (min 32 chars) |
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
| `SMTP_FROM_EMAIL` | noreply@opn.onl | From email address |
| `ADMIN_EMAIL` | admin@opn.onl | Admin email for contact form |

### Link Management

| Variable | Default | Description |
|----------|---------|-------------|
| `ENABLE_CUSTOM_ALIASES` | true | Allow users to create custom aliases |
| `ALLOW_DELETED_SLUG_REUSE` | false | Allow reusing slugs from deleted links |
| `ENABLE_ACCOUNT_DELETION` | false | Allow users to delete their own accounts |

### Performance Tuning

| Variable | Default | Description |
|----------|---------|-------------|
| `REDIS_URL` | - | Redis connection URL |
| `REDIS_CACHE_TTL` | 300 | Cache TTL in seconds |
| `CLICK_BUFFER_SIZE` | 100 | Click events before DB flush |
| `CLICK_FLUSH_INTERVAL` | 10 | Flush interval in seconds |

### Backups (S3-compatible)

| Variable | Description |
|----------|-------------|
| `BACKUP_S3_ENDPOINT` | S3 endpoint (e.g., Cloudflare R2) |
| `BACKUP_S3_BUCKET` | S3 bucket name |
| `BACKUP_S3_ACCESS_KEY` | S3 access key |
| `BACKUP_S3_SECRET_KEY` | S3 secret key |
| `BACKUP_S3_REGION` | S3 region (default: auto) |

### Analytics (Optional)

| Variable | Description |
|----------|-------------|
| `VITE_GA_ID` | Google Analytics Measurement ID (e.g., G-XXXXXXXXXX) |
| `MAXMIND_ACCOUNT_ID` | MaxMind account ID for GeoIP |
| `MAXMIND_LICENSE_KEY` | MaxMind license key for GeoIP |

### Other

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | info | Log level (trace, debug, info, warn, error) |
| `FORCE_HTTPS` | true | Force HTTPS redirects |
| `WEBAUTHN_RP_ID` | (from FRONTEND_URL) | WebAuthn Relying Party ID |

## API Reference

Full API documentation available at `/swagger-ui/` when backend is running.

### Authentication

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/auth/register` | Register new account |
| POST | `/auth/login` | Login with email/password |
| POST | `/auth/verify-email` | Verify email with token |
| POST | `/auth/resend-verification` | Resend verification email |
| POST | `/auth/forgot-password` | Request password reset |
| POST | `/auth/reset-password` | Reset password with token |
| POST | `/auth/change-password` | Change password (authenticated) |
| DELETE | `/auth/delete-account` | Delete own account (if enabled) |

### Links

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/links` | List user's links |
| POST | `/links` | Create new link |
| GET | `/links/{id}` | Get link details |
| PUT | `/links/{id}` | Update link |
| DELETE | `/links/{id}` | Delete link |
| GET | `/links/{id}/qr` | Get QR code image |
| GET | `/links/{id}/stats` | Get link analytics |
| POST | `/links/bulk` | Create multiple links |
| POST | `/links/bulk/delete` | Delete multiple links |
| POST | `/links/bulk/update` | Update multiple links |
| GET | `/links/export` | Export links as CSV |

### Redirects

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/{code}` | Redirect to original URL |
| POST | `/{code}/verify` | Verify password-protected link |

### Organizations

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/organizations` | List user's organizations |
| POST | `/organizations` | Create organization |
| GET | `/organizations/{id}` | Get organization |
| PUT | `/organizations/{id}` | Update organization |
| DELETE | `/organizations/{id}` | Delete organization |
| GET | `/organizations/{id}/members` | List members |
| POST | `/organizations/{id}/members` | Add member |
| DELETE | `/organizations/{id}/members/{user_id}` | Remove member |
| GET | `/organizations/{id}/audit-log` | View audit log |

### Folders

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/folders` | List folders |
| POST | `/folders` | Create folder |
| GET | `/folders/{id}` | Get folder |
| PUT | `/folders/{id}` | Update folder |
| DELETE | `/folders/{id}` | Delete folder |
| GET | `/folders/{id}/links` | Get links in folder |
| POST | `/folders/{id}/links` | Move links to folder |

### Tags

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/tags` | List tags |
| POST | `/tags` | Create tag |
| PUT | `/tags/{id}` | Update tag |
| DELETE | `/tags/{id}` | Delete tag |
| GET | `/tags/{id}/links` | Get links with tag |
| POST | `/links/{id}/tags` | Add tags to link |
| DELETE | `/links/{id}/tags` | Remove tags from link |

### Admin (Admin only)

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/admin/stats` | Get system statistics |
| GET | `/admin/users` | List all users |
| DELETE | `/admin/users/{id}` | Soft delete user |
| DELETE | `/admin/users/{id}/hard` | Permanently delete user |
| POST | `/admin/users/{id}/restore` | Restore deleted user |
| POST | `/admin/users/{id}/make-admin` | Promote to admin |
| POST | `/admin/users/{id}/remove-admin` | Demote from admin |
| GET | `/admin/blocked/links` | List blocked URLs |
| POST | `/admin/blocked/links` | Block URL |
| DELETE | `/admin/blocked/links/{id}` | Unblock URL |
| GET | `/admin/blocked/domains` | List blocked domains |
| POST | `/admin/blocked/domains` | Block domain |
| DELETE | `/admin/blocked/domains/{id}` | Unblock domain |
| POST | `/admin/backup` | Create database backup |
| GET | `/admin/backup` | List backups |
| DELETE | `/admin/backup/cleanup/{keep}` | Clean old backups |

### Other

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/health` | Health check |
| POST | `/contact` | Contact form |
| GET | `/analytics/dashboard` | User analytics dashboard |

## Project Structure

```
opn.onl/
├── backend/
│   ├── src/
│   │   ├── entity/       # SeaORM database models
│   │   ├── handlers/     # API route handlers
│   │   └── utils/        # Utilities (cache, jwt, geoip, email, backup)
│   ├── migration/        # Database migrations
│   ├── scripts/          # Helper scripts (GeoIP download, entrypoint)
│   ├── tests/            # Integration tests
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

Backups can be triggered via Admin panel or API. Old backups are automatically cleaned up.

## Security Features

### Rate Limiting
- **Per-second:** 10 requests/second per IP
- **General:** 100 requests/minute per IP
- **Link creation:** 100 links/hour per user
- **Auth endpoints:** 10 attempts/minute per IP
- **Redirects:** 100/second per IP

### Content Blocking
Admins can block:
- **Specific URLs:** Block exact malicious URLs
- **Entire domains:** Block all links to a domain (including subdomains)

Blocked content cannot be shortened via any endpoint (single, bulk, API).

### Data Protection
- Passwords hashed with bcrypt
- JWT tokens with expiration
- Soft-delete for links and users (data preserved)
- Email verification required (configurable)
- HTTPS enforced in production

## Development

### Backend

```bash
cd backend
cargo run          # Dev server on :3000
cargo test         # Run tests
cargo clippy       # Lint
cargo build --release
```

### Frontend

```bash
cd frontend
npm run dev        # Dev server on :5173
npm run test       # Unit tests (Vitest)
npm run test:e2e   # E2E tests (Playwright)
npm run build      # Production build
npm run lint       # ESLint
```

### Database Migrations

```bash
cd backend
# Migrations run automatically on startup
# To run manually:
cargo run -- migrate
```

## Troubleshooting

### Container Health Checks Failing
```bash
# Check backend health
curl http://localhost:3000/health

# Check frontend health
curl http://localhost:80/health

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
