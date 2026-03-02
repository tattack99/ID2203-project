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
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json

# Build application
COPY . .
RUN cargo build --release --bin server

FROM debian:bookworm-slim AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/server /usr/local/bin
EXPOSE 8000
ENTRYPOINT ["/usr/local/bin/server"]

# Jepsen
FROM debian:bookworm-slim AS runtime
WORKDIR /app

# Install SSH, sudo, and network tools
RUN apt-get update && apt-get install -y \
    openssh-server \
    sudo \
    iptables \
    iproute2 \
    && rm -rf /var/lib/apt/lists/*

# Configure SSH for root login (standard Jepsen practice)
RUN mkdir /var/run/sshd
RUN echo 'root:root' | chpasswd
RUN sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin yes/' /etc/ssh/sshd_config
RUN sed -i 's/UsePAM yes/UsePAM no/' /etc/ssh/sshd_config

COPY --from=builder /app/target/release/server /usr/local/bin

EXPOSE 8000 22

# Start SSH in the background, then launch your Rust server
ENTRYPOINT service ssh start && /usr/local/bin/server