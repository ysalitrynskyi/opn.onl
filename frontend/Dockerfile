# Build stage - use full Node image for Puppeteer/Chromium support
FROM node:22-slim AS builder

# Don't let puppeteer download its own Chrome (Chrome for Testing has no
# linux/arm64 builds); we use Playwright's Chromium below instead.
ENV PUPPETEER_SKIP_CHROMIUM_DOWNLOAD=true

WORKDIR /app

# Copy package files
COPY package*.json ./

# Install dependencies
RUN npm ci --legacy-peer-deps

# Chromium for the prerender step. Debian's `chromium` package (150+) crashes
# with SIGTRAP under Docker on both amd64 and arm64, which surfaced as
# puppeteer "Failed to launch the browser process" with an empty stderr.
# Playwright's Chromium is version-pinned through package-lock.json, ships
# proper linux/amd64 AND linux/arm64 builds, and --with-deps installs the
# required system libraries.
RUN npx playwright install chromium --with-deps \
    && rm -rf /var/lib/apt/lists/*

# Copy source code
COPY . .

# Set build-time environment variables (API URLs are baked in at build time)
ARG VITE_API_URL
ARG VITE_FRONTEND_URL

ENV VITE_API_URL=$VITE_API_URL
ENV VITE_FRONTEND_URL=$VITE_FRONTEND_URL

# Build the application with prerendering. The Playwright Chromium path
# contains a version segment, so resolve it at build time; puppeteer picks it
# up via PUPPETEER_EXECUTABLE_PATH.
RUN PUPPETEER_EXECUTABLE_PATH="$(find /root/.cache/ms-playwright -name chrome -type f | head -1)" \
    npm run build

# Production stage with nginx
FROM nginx:alpine

# Install curl for health checks
RUN apk add --no-cache curl

# Copy custom nginx config
COPY nginx.conf /etc/nginx/conf.d/default.conf

# Bake the API origin into the CSP connect-src. The frontend fetches the API
# cross-origin (VITE_API_URL, e.g. https://l.opn.onl), so the document CSP must
# allow that origin — otherwise connect-src 'self' blocks every API call and the
# app loads no data. Derived from the same build arg the JS bundle uses, so the
# CSP and the baked-in API URL can never drift. Also allow the wss:// form for
# any future WebSocket/SSE use. Empty when VITE_API_URL is unset (same-origin dev).
ARG VITE_API_URL
RUN if [ -n "$VITE_API_URL" ]; then \
        WS_ORIGIN=$(printf '%s' "$VITE_API_URL" | sed -e 's#^https://#wss://#' -e 's#^http://#ws://#'); \
        CONNECT_SRC="$VITE_API_URL $WS_ORIGIN"; \
    else \
        CONNECT_SRC=""; \
    fi; \
    sed -i "s#__API_CONNECT_SRC__#${CONNECT_SRC}#g" /etc/nginx/conf.d/default.conf; \
    echo "CSP connect-src extra sources: '${CONNECT_SRC}'"

# Copy docker entrypoint script
COPY docker-entrypoint.sh /docker-entrypoint.sh
RUN chmod +x /docker-entrypoint.sh

# Copy built assets from builder
COPY --from=builder /app/dist /usr/share/nginx/html

# Create non-root user for nginx
RUN chown -R nginx:nginx /usr/share/nginx/html && \
    chown -R nginx:nginx /var/cache/nginx && \
    chown -R nginx:nginx /var/log/nginx && \
    touch /var/run/nginx.pid && \
    chown -R nginx:nginx /var/run/nginx.pid

# Expose port
EXPOSE 80

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=15s --retries=3 \
  CMD curl -fsS http://localhost:80/health || exit 1

# Use entrypoint for runtime env injection
ENTRYPOINT ["/docker-entrypoint.sh"]
CMD ["nginx", "-g", "daemon off;"]

