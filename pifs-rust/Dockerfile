FROM rust:1.83-alpine

# Install build & runtime dependencies (musl-based)
RUN apk add --no-cache \
        fuse3 fuse3-dev pkgconf musl-dev \
        bash coreutils diffutils python3 \
        wget ca-certificates

# Install IPFS (kubo)
ARG IPFS_VERSION=v0.32.1
RUN ARCH=$(uname -m) && \
    case "$ARCH" in \
        x86_64)  ARCH=amd64 ;; \
        aarch64) ARCH=arm64 ;; \
    esac && \
    wget -qO /tmp/kubo.tar.gz \
      "https://dist.ipfs.tech/kubo/${IPFS_VERSION}/kubo_${IPFS_VERSION}_linux-${ARCH}.tar.gz" && \
    tar -xzf /tmp/kubo.tar.gz -C /tmp && \
    mv /tmp/kubo/ipfs /usr/local/bin/ipfs && \
    rm -rf /tmp/kubo /tmp/kubo.tar.gz

# Initialize IPFS repo (offline mode is fine for tests)
RUN ipfs init --profile=test

# Copy source
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY fuser-patched/ fuser-patched/
COPY src/ src/
COPY tests/ tests/

# Build — force dynamic linking (musl default is static, but fuse3 has no .a)
ENV RUSTFLAGS="-C target-feature=-crt-static"
RUN cargo build 2>&1

# Run unit tests at build time (no FUSE needed)
RUN cargo test 2>&1

# Integration tests need FUSE — must run with: docker run --privileged
CMD ["/bin/bash", "-c", "\
    modprobe fuse 2>/dev/null || true \n\
    ipfs daemon --init &>/tmp/ipfs-daemon.log & \n\
    IPFS_PID=$! \n\
    for i in $(seq 1 30); do \n\
        ipfs swarm peers &>/dev/null && break \n\
        sleep 1 \n\
    done \n\
    bash tests/integration.sh \n\
    EXIT_CODE=$? \n\
    kill $IPFS_PID 2>/dev/null \n\
    exit $EXIT_CODE \n\
"]
