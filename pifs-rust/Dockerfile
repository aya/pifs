FROM rust:1.83-bookworm

# Install runtime dependencies: FUSE, IPFS, test utilities
RUN apt-get update && apt-get install -y --no-install-recommends \
        fuse3 libfuse3-dev pkg-config \
        wget ca-certificates \
        python3 coreutils diffutils \
    && rm -rf /var/lib/apt/lists/*

# Install IPFS (kubo)
ARG IPFS_VERSION=v0.32.1
RUN ARCH=$(dpkg --print-architecture) && \
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

# Build
RUN cargo build 2>&1

# Run unit tests at build time (no FUSE needed)
RUN cargo test 2>&1

# Integration tests need FUSE — must run with: docker run --privileged
CMD ["/bin/bash", "-c", "\
    # Load fuse module if needed \n\
    modprobe fuse 2>/dev/null || true \n\
    # Start IPFS daemon in background \n\
    ipfs daemon --init &>/tmp/ipfs-daemon.log & \n\
    IPFS_PID=$! \n\
    # Wait for IPFS API to be ready \n\
    for i in $(seq 1 30); do \n\
        ipfs swarm peers &>/dev/null && break \n\
        sleep 1 \n\
    done \n\
    # Run integration tests \n\
    bash tests/integration.sh \n\
    EXIT_CODE=$? \n\
    kill $IPFS_PID 2>/dev/null \n\
    exit $EXIT_CODE \n\
"]
