# opn.onl

A privacy-focused URL shortener built with Rust and React.

## Features

- **URL Shortening** - Create short links with custom aliases
- **Analytics** - Track clicks with geographic data, device info, and referrers
- **Password Protection** - Secure links with passwords
- **Link Scheduling** - Set start dates and expiration for time-limited access
- **Click Limits** - Define maximum clicks per link
- **QR Code Generation** - Get QR codes for any link
- **Passkey Authentication** - Passwordless login via WebAuthn
- **Organizations** - Team workspaces with role-based access
- **Folders & Tags** - Organize links efficiently
- **Bulk Operations** - Create, update, delete links in batches
- **CSV Export** - Export all your data
- **Real-time Updates** - WebSocket/SSE for live click notifications
- **API Documentation** - OpenAPI/Swagger UI included
- **Rate Limiting** - Built-in protection against abuse
- **Redis Caching** - Optional caching layer for faster redirects
- **Email Verification** - Secure account verification
- **Automated Backups** - S3-compatible backup support

## Tech Stack

**Backend:** Rust, Axum, SeaORM, PostgreSQL, Redis (optional), JWT, WebAuthn

**Frontend:** React 19, TypeScript, Vite, Tailwind CSS, Framer Motion, Recharts

## Quick Start

### Prerequisites

- Docker & Docker Compose
- Rust 1.70+ (for local development)
- Node.js 20+ (for local development)

### Development Setup

```bash
# Clone
git clone https://github.com/yourusername/opn.onl.git
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

## Production Deployment with Docker

### 1. Setup Cloudflare Tunnel

1. Go to [Cloudflare Zero Trust](https://one.dash.cloudflare.com/) → Access → Tunnels
2. Create a new tunnel named `opn-onl`
3. Copy the tunnel token
4. Configure public hostnames:
   - `opn.onl` → `http://frontend:80`
   - `api.opn.onl` → `http://backend:3000`

### 2. Configure Environment

```bash
# Copy and edit environment variables
cp .env.example .env

# Edit .env with your values
nano .env
```

Required variables:
```env
# Database
POSTGRES_PASSWORD=your-secure-password

# Security (generate with: openssl rand -base64 64)
JWT_SECRET=your-very-long-random-jwt-secret

# URLs
BASE_URL=https://api.opn.onl
FRONTEND_URL=https://opn.onl
VITE_API_URL=https://api.opn.onl
VITE_FRONTEND_URL=https://opn.onl

# Cloudflare Tunnel
CLOUDFLARE_TUNNEL_TOKEN=your-tunnel-token
```

### 3. Deploy

```bash
# Build and start all services
docker-compose up -d --build

# View logs
docker-compose logs -f

# Check status
docker-compose ps
```

### 4. Verify Deployment

```bash
# Health check
curl https://api.opn.onl/health

# Response:
{
  "status": "healthy",
  "database": "connected",
  "redis": "connected",
  "email": "configured",
  "backup": "configured"
}
```

## Docker Services

| Service | Description | Port |
|---------|-------------|------|
| `db` | PostgreSQL 15 database | 5432 (internal) |
| `redis` | Redis 7 cache | 6379 (internal) |
| `backend` | Rust API server | 3000 (internal) |
| `frontend` | React app via Nginx | 80 (internal) |
| `cloudflared` | Cloudflare tunnel | - |

## Environment Variables

### Required

| Variable | Description |
|----------|-------------|
| `POSTGRES_PASSWORD` | Database password |
| `JWT_SECRET` | Secret for JWT tokens (min 32 chars) |
| `BASE_URL` | Public API URL |
| `FRONTEND_URL` | Public frontend URL |
| `CLOUDFLARE_TUNNEL_TOKEN` | Cloudflare tunnel token |

### Optional (Recommended for Production)

| Variable | Default | Description |
|----------|---------|-------------|
| `SMTP_HOST` | - | SMTP server for emails |
| `SMTP_PORT` | 587 | SMTP port |
| `SMTP_USER` | - | SMTP username |
| `SMTP_PASS` | - | SMTP password |
| `SMTP_FROM_EMAIL` | noreply@opn.onl | From email address |
| `ADMIN_EMAIL` | admin@opn.onl | Admin email for contact form |
| `BACKUP_S3_ENDPOINT` | - | S3 endpoint for backups |
| `BACKUP_S3_BUCKET` | - | S3 bucket name |
| `BACKUP_S3_ACCESS_KEY` | - | S3 access key |
| `BACKUP_S3_SECRET_KEY` | - | S3 secret key |

### Performance Tuning

| Variable | Default | Description |
|----------|---------|-------------|
| `REDIS_CACHE_TTL` | 300 | Cache TTL in seconds |
| `CLICK_BUFFER_SIZE` | 100 | Click events before flush |
| `CLICK_FLUSH_INTERVAL` | 10 | Flush interval in seconds |
| `RUST_LOG` | info | Log level |

## Development

### Backend

```bash
cd backend
cargo run          # Dev server
cargo test         # Run tests
cargo clippy       # Lint
cargo build --release  # Production build
```

### Frontend

```bash
cd frontend
npm run dev        # Dev server
npm run test       # Unit tests
npm run test:e2e   # E2E tests (Playwright)
npm run build      # Production build
npm run lint       # Lint
```

## API Documentation

API documentation is available at `/swagger-ui/` when the backend is running.

### Main Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | /auth/register | Register |
| POST | /auth/login | Login |
| POST | /auth/verify-email | Verify email |
| POST | /auth/forgot-password | Request password reset |
| GET | /links | List links |
| POST | /links | Create link |
| GET | /{code} | Redirect |
| GET | /links/{id}/stats | Analytics |
| POST | /contact | Contact form |
| GET | /health | Health check |

## Project Structure

```
opn.onl/
├── backend/
│   ├── src/
│   │   ├── entity/       # Database models
│   │   ├── handlers/     # Route handlers
│   │   └── utils/        # Utilities (cache, jwt, geoip, email)
│   ├── migration/        # Database migrations
│   ├── tests/            # Integration tests
│   └── Dockerfile
├── frontend/
│   ├── src/
│   │   ├── components/   # UI components
│   │   ├── pages/        # Page components
│   │   └── config/       # Configuration
│   ├── e2e/              # E2E tests
│   ├── Dockerfile
│   └── nginx.conf
├── docker-compose.yml      # Production
├── docker-compose.dev.yml  # Development
└── .env.example
```

## Deployment Options

### Option 1: Docker Compose (with build)

```bash
cp .env.example .env
# Edit .env with your values
docker compose up -d
```

### Option 2: Portainer (pre-built images)

Docker images are **automatically built by GitHub Actions** on every push to `release`.

**In Portainer:**
1. Go to Stacks → Add Stack
2. Use either compose file (Docker will auto-select the correct architecture):
   - `docker-compose.portainer.amd64.yml` (for Intel/AMD servers)
   - `docker-compose.portainer.arm64.yml` (for ARM servers)
3. Add to your `.env`:
   ```
   BACKEND_IMAGE=ghcr.io/ysalitrynskyi/opn-backend:latest
   FRONTEND_IMAGE=ghcr.io/ysalitrynskyi/opn-frontend:latest
   ```
4. Deploy!

**Note:** Images are built as multi-arch manifests. Docker will automatically pull the correct architecture for your server.

**Troubleshooting:** If you get "exec format error", the multi-arch manifest might not be ready yet. Try:

```bash
# Remove old images
docker rmi ghcr.io/ysalitrynskyi/opn-backend:latest
docker rmi ghcr.io/ysalitrynskyi/opn-frontend:latest

# Pull with explicit platform
docker pull --platform linux/arm64 ghcr.io/ysalitrynskyi/opn-backend:latest
docker pull --platform linux/arm64 ghcr.io/ysalitrynskyi/opn-frontend:latest

# Or use platform-specific tags (if manifest not ready)
docker pull ghcr.io/ysalitrynskyi/opn-backend:latest-arm64
docker pull ghcr.io/ysalitrynskyi/opn-frontend:latest-arm64
```

Images are available at:
- `ghcr.io/ysalitrynskyi/opn-backend:latest`
- `ghcr.io/ysalitrynskyi/opn-frontend:latest`

Supported platforms: `linux/amd64`, `linux/arm64`

## GeoIP Analytics (Optional)

To enable country/city detection for click analytics:

### Docker Deployment (Automatic)
1. Create a free account at [MaxMind](https://www.maxmind.com/en/geolite2/signup)
2. Go to Account → Manage License Keys → Generate new license key
3. Add to your `.env`:
   ```
   MAXMIND_ACCOUNT_ID=123456
   MAXMIND_LICENSE_KEY=your-license-key
   ```
4. The database will be downloaded automatically on container start

### Local Development (Manual)
1. Download `GeoLite2-City.mmdb` from your MaxMind account
2. Place it in `backend/data/GeoLite2-City.mmdb`

**The app works without GeoIP** - location analytics will just be empty.

## Backup & Restore

### Manual Backup

```bash
# Backup database
docker exec opn_onl_db pg_dump -U postgres opn_onl > backup.sql

# Restore database
docker exec -i opn_onl_db psql -U postgres opn_onl < backup.sql
```

### Automated Backups

Configure S3-compatible storage (Cloudflare R2, AWS S3, etc.) in `.env`:

```env
BACKUP_S3_ENDPOINT=https://your-account-id.r2.cloudflarestorage.com
BACKUP_S3_BUCKET=opn-backups
BACKUP_S3_ACCESS_KEY=your-key
BACKUP_S3_SECRET_KEY=your-secret
```

Backups run automatically at 4 AM UTC and keep the last 7 backups.

## Contributing

1. Fork the repo
2. Create a feature branch
3. Commit changes
4. Push to your fork
5. Open a PR

## License

AGPL-3.0. See [LICENSE](LICENSE).
