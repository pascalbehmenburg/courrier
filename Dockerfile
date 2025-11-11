# Multi-stage build for Courrier email fetching service

# Build stage
FROM rust:latest AS builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy manifest files
COPY Cargo.toml Cargo.lock ./

# Copy source code and assets
COPY src ./src
COPY assets ./assets

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 && \
    rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /build/target/release/courrier /usr/local/bin/courrier

# Create directories for config and data
RUN mkdir -p /config /data

# Set working directory to /config so Config.toml is found
WORKDIR /config

# Set database path to /data/courrier.db
ENV COURRIER_DB_PATH=/data/courrier.db

# Expose port for web dashboard
EXPOSE 3000

CMD ["courrier", "server", "3000"]

