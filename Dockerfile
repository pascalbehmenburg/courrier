# syntax=docker/dockerfile:1.7
#
# Multi-stage build for Courrier:
#   1. node-builder  → builds the React SPA into desktop/dist
#   2. rust-builder  → compiles the axum server, embedding desktop/dist
#                      into the binary via rust-embed
#   3. runtime       → tiny debian image, non-root user

###############################################################################
# Stage 1 — frontend
###############################################################################
FROM node:22-bookworm-slim AS node-builder

WORKDIR /app/desktop

# Use corepack-managed pnpm so the version stays pinned with the lockfile.
RUN corepack enable && corepack prepare pnpm@9 --activate

# Install deps with the lockfile only first to keep the layer cache hot when
# only frontend source changes.
COPY desktop/package.json desktop/pnpm-lock.yaml* ./
RUN pnpm install --frozen-lockfile || pnpm install

COPY desktop ./
RUN pnpm build

###############################################################################
# Stage 2 — Rust server
###############################################################################
FROM rust:1.84-bookworm AS rust-builder

WORKDIR /build

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
        ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Workspace manifest + crates.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# Tauri crate is a workspace member. We don't build it here, but cargo needs
# the manifest to resolve the workspace.
COPY desktop/src-tauri ./desktop/src-tauri

# Pull in the freshly built React bundle so rust-embed has files to embed.
COPY --from=node-builder /app/desktop/dist ./desktop/dist

RUN cargo build --release -p courrier-server -p courrier-migrate

###############################################################################
# Stage 3 — runtime
###############################################################################
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 && \
    rm -rf /var/lib/apt/lists/*

# Non-root user, fixed UID/GID so host bind mounts can be chowned predictably.
RUN groupadd --system --gid 10001 courrier && \
    useradd --system --uid 10001 --gid courrier \
            --home-dir /data --shell /usr/sbin/nologin courrier && \
    mkdir -p /data && chown -R courrier:courrier /data

COPY --from=rust-builder /build/target/release/courrier /usr/local/bin/courrier
COPY --from=rust-builder /build/target/release/courrier-migrate /usr/local/bin/courrier-migrate

USER courrier
WORKDIR /data

# Server defaults — override at `docker run -e`.
ENV COURRIER_DB_PATH=/data/courrier.db \
    COURRIER_STORAGE_PATH=/data/emails \
    COURRIER_BIND_ADDR=0.0.0.0:3000 \
    COURRIER_FETCH_ON_STARTUP=true \
    RUST_LOG=info

# COURRIER_ENCRYPTION_KEY must be supplied at runtime; the server refuses
# to start without it. Generate with:
#   head -c 32 /dev/urandom | base64

EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s \
  CMD wget -qO- http://127.0.0.1:3000/api/health || exit 1

CMD ["courrier"]
