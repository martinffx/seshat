# Multi-stage build for Seshat
FROM rust:1.90 as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    protobuf-compiler \
    libprotobuf-dev \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy manifests for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/seshat/Cargo.toml ./crates/seshat/Cargo.toml
COPY crates/raft/Cargo.toml ./crates/raft/Cargo.toml
COPY crates/storage/Cargo.toml ./crates/storage/Cargo.toml
COPY crates/protocol/Cargo.toml ./crates/protocol/Cargo.toml
COPY crates/common/Cargo.toml ./crates/common/Cargo.toml

# Create dummy source files to build dependencies
RUN mkdir -p crates/seshat/src && echo "fn main() {}" > crates/seshat/src/main.rs && \
    mkdir -p crates/raft/src && echo "pub fn placeholder() {}" > crates/raft/src/lib.rs && \
    mkdir -p crates/storage/src && echo "pub fn placeholder() {}" > crates/storage/src/lib.rs && \
    mkdir -p crates/protocol/src && echo "pub fn placeholder() {}" > crates/protocol/src/lib.rs && \
    mkdir -p crates/common/src && echo "pub fn placeholder() {}" > crates/common/src/lib.rs

# Build dependencies (this layer is cached)
RUN cargo build --release

# Remove dummy source
RUN rm -rf crates/*/src

# Copy real source code
COPY crates ./crates

# Build application
RUN cargo build --release --bin seshat

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    libsnappy1v5 \
    liblz4-1 \
    libzstd1 \
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

ENTRYPOINT ["seshat"]
CMD ["--config", "/app/config/node.toml"]
