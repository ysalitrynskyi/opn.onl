# Stage 1: Chef - prepare recipe
FROM rust:1.85-alpine AS chef
RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static pkgconfig curl
RUN cargo install cargo-chef
WORKDIR /app

# Stage 2: Planner - create recipe.json
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY migration ./migration
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

# Stage 3: Builder - build dependencies (cached) then app
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this layer is cached!
RUN cargo chef cook --release --recipe-path recipe.json
# Copy source and build app
COPY Cargo.toml Cargo.lock ./
COPY migration ./migration
COPY src ./src
RUN cargo build --release

# Stage 4: Runtime
FROM alpine:3.19
RUN apk add --no-cache ca-certificates libgcc postgresql-client wget
RUN addgroup -g 1000 app && adduser -u 1000 -G app -s /bin/sh -D app

WORKDIR /app
COPY --from=builder /app/target/release/opn_onl_backend /app/opn_onl_backend
COPY scripts/download-geoip.sh /app/scripts/download-geoip.sh
COPY scripts/entrypoint.sh /app/scripts/entrypoint.sh
RUN chmod +x /app/scripts/*.sh
RUN mkdir -p /app/logs /app/data && chown -R app:app /app

USER app
EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
  CMD wget --no-verbose --tries=1 --spider http://localhost:3000/health || exit 1

ENTRYPOINT ["/app/scripts/entrypoint.sh"]
