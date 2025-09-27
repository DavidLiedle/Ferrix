# Multi-stage build for Ferrix

# Build stage
FROM rust:1.70 AS builder

WORKDIR /usr/src/ferrix

# Copy manifest files
COPY Cargo.toml Cargo.lock ./

# Create dummy main to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy actual source code
COPY . .

# Build the application
RUN cargo build --release --no-default-features

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y \
        ca-certificates \
        libssl3 \
        && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 ferrix

# Copy binary from builder
COPY --from=builder /usr/src/ferrix/target/release/ferrix /usr/local/bin/ferrix

# Create config directory
RUN mkdir -p /home/ferrix/.ferrix && \
    chown -R ferrix:ferrix /home/ferrix

# Switch to non-root user
USER ferrix
WORKDIR /home/ferrix

# Default socket directory
ENV FERRIX_SOCKET_DIR=/tmp/ferrix

# Expose default remote server port (if enabled)
EXPOSE 9999

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ferrix list || exit 1

# Default command
ENTRYPOINT ["ferrix"]
CMD ["server", "--foreground"]