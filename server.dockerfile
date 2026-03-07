FROM rust:1.85 AS chef

# Stop if a command fails
RUN set -eux

# Only fetch crates.io index for used crates
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse

# cargo-chef will be cached from the second build onwards
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
RUN apt-get update && apt-get install -y libclang-dev cmake && rm -rf /var/lib/apt/lists/*
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json

# Build application
COPY . .
RUN cargo build --release --bin server

# FINAL SINGLE RUNTIME STAGE
FROM debian:bookworm-slim AS runtime
WORKDIR /app

# 1. Install all dependencies in one layer
RUN apt-get update && apt-get install -y libclang-dev cmake -y \
    openssh-server \
    sudo \
    iptables \
    iproute2 \
    procps \
    curl \
    && rm -rf /var/lib/apt/lists/*

# 2. Configure SSH (Standard Jepsen setup)
RUN mkdir /var/run/sshd && \
    echo 'root:root' | chpasswd && \
    sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin yes/' /etc/ssh/sshd_config && \
    sed -i 's/UsePAM yes/UsePAM no/' /etc/ssh/sshd_config

# 3. Copy the binary from the builder stage
COPY --from=builder /app/target/release/server /usr/local/bin/server

# Ensure logs directory exists for the redirection in ENTRYPOINT
RUN mkdir -p /app/logs

EXPOSE 8000 22

# 4. The "OmniPaxos Lifecycle" Entrypoint
# - service ssh start: Allows Jepsen to connect
# - nohup ... &: Starts your Rust server immediately on boot
# - tail -f: Keeps the container alive even if the Rust server is killed
# ... previous stages ...
# We use 'sh -c' to ensure the redirection and backgrounding work as intended.
ENTRYPOINT ["sh", "-c", "service ssh start && cd /app && stdbuf -oL -eL /usr/local/bin/server 2>&1 | tee /app/logs/stdout.log & sleep 2 && tail -f /dev/null"]
