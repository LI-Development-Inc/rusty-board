# rusty-board — Multi-stage Docker build
#
# Stage 1: builder  — compiles the release binary
# Stage 2: runtime  — minimal image with the binary only
#
# Build args:
#   FEATURES  — comma-separated Cargo feature list (default: web-axum,db-postgres,auth-jwt,media-local,redis)
#
# Usage:
#   docker build -t rusty-board .
#   docker build --build-arg FEATURES="web-axum,db-postgres,auth-jwt,media-s3,redis,video" -t rusty-board-video .

ARG FEATURES="web-axum,db-postgres,auth-jwt,media-local,redis"

# ─── Stage 1: builder ─────────────────────────────────────────────────────────
FROM rust:1.75-slim AS builder

ARG FEATURES

# System dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Video feature dependencies (only if needed)
# Uncomment if compiling with the 'video' feature:
# RUN apt-get update && apt-get install -y --no-install-recommends \
#     libavcodec-dev libavformat-dev libavutil-dev libswscale-dev \
#     && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Cache dependencies — copy manifests first for layer caching
COPY Cargo.toml Cargo.lock ./
COPY cmd/rusty-board/Cargo.toml cmd/rusty-board/
COPY crates/domains/Cargo.toml      crates/domains/
COPY crates/services/Cargo.toml     crates/services/
COPY crates/storage-adapters/Cargo.toml crates/storage-adapters/
COPY crates/auth-adapters/Cargo.toml    crates/auth-adapters/
COPY crates/api-adapters/Cargo.toml     crates/api-adapters/
COPY crates/configs/Cargo.toml          crates/configs/
COPY crates/integration-tests/Cargo.toml crates/integration-tests/

# Create dummy lib.rs / main.rs stubs so `cargo build` can resolve deps
RUN mkdir -p \
      cmd/rusty-board/src \
      crates/domains/src \
      crates/services/src \
      crates/storage-adapters/src \
      crates/auth-adapters/src \
      crates/api-adapters/src \
      crates/configs/src \
      crates/integration-tests/src \
    && echo "fn main(){}" > cmd/rusty-board/src/main.rs \
    && for d in domains services storage-adapters auth-adapters api-adapters configs integration-tests; do \
         echo "" > crates/$d/src/lib.rs; \
       done

# Build deps only (exploits Docker layer caching)
RUN cargo build --release --features "$FEATURES" --bin rusty-board 2>&1 | tail -5 || true

# Now copy the real source and build
COPY . .

# SQLx offline mode — sqlx-data.json must be committed and up to date
ENV SQLX_OFFLINE=true

RUN cargo build --release --features "$FEATURES" --bin rusty-board

# Strip the binary to reduce size (~30% smaller)
RUN strip target/release/rusty-board

# ─── Stage 2: runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

# CA certificates for TLS outbound (S3, etc.)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Non-root user for running the application
RUN useradd --uid 1001 --create-home --shell /bin/false rustyboard

WORKDIR /app

# Copy binary
COPY --from=builder /build/target/release/rusty-board /app/rusty-board

# Copy templates and static assets
COPY templates/ /app/templates/
COPY static/ /app/static/

# Media directory (used when media-local feature is compiled in)
RUN mkdir -p /app/media && chown rustyboard:rustyboard /app/media

USER rustyboard

EXPOSE 8080

HEALTHCHECK --interval=15s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/healthz || exit 1

ENTRYPOINT ["/app/rusty-board"]
