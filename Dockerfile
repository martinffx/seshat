# Multi-stage build for Seshat
FROM rust:1.90-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    protobuf-compiler \
    libprotobuf-dev \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build dependencies (cached layer)
RUN mkdir -p crates/seshat/src && \
    echo "fn main() {}" > crates/seshat/src/main.rs && \
    cargo build --release && \
    rm -rf crates/*/src

# Build application
COPY crates ./crates
RUN cargo build --release --bin seshat

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 seshat && \
    mkdir -p /var/lib/seshat && \
    chown -R seshat:seshat /var/lib/seshat

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/seshat /usr/local/bin/seshat

# Copy default configuration
COPY config /app/config

USER seshat

# Expose ports
EXPOSE 6379 7379

# Health check
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=3 \
    CMD ["sh", "-c", "command -v curl || exit 0; curl -f http://localhost:8080/health || exit 1"]

ENTRYPOINT ["seshat"]
CMD ["--config", "/app/config/node.toml"]
