# Build stage
FROM rust:1.85-alpine AS builder

# Install build dependencies (curl needed for utoipa-swagger-ui)
RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static pkgconfig curl

# Create app directory
WORKDIR /app

# Copy everything needed for build
COPY Cargo.toml Cargo.lock ./
COPY migration ./migration
COPY src ./src

# Build the application
RUN cargo build --release

# Runtime stage
FROM alpine:3.19

# Install runtime dependencies
RUN apk add --no-cache ca-certificates libgcc postgresql-client wget

# Create non-root user
RUN addgroup -g 1000 app && adduser -u 1000 -G app -s /bin/sh -D app

WORKDIR /app

# Copy the binary from builder
COPY --from=builder /app/target/release/opn_onl_backend /app/opn_onl_backend

# Copy scripts
COPY scripts/download-geoip.sh /app/scripts/download-geoip.sh
COPY scripts/entrypoint.sh /app/scripts/entrypoint.sh
RUN chmod +x /app/scripts/*.sh

# Create directories
RUN mkdir -p /app/logs /app/data && chown -R app:app /app

# Switch to non-root user
USER app

# Expose port
EXPOSE 3000

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
  CMD wget --no-verbose --tries=1 --spider http://localhost:3000/health || exit 1

# Use entrypoint script
ENTRYPOINT ["/app/scripts/entrypoint.sh"]
