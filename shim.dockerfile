FROM rust:1.85 AS chef
RUN set -eux
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Dependencies are still cached!
RUN cargo chef cook --release --recipe-path recipe.json

# Build application
COPY . .
# 1. CHANGE: Build the 'shim' binary specifically
RUN cargo build --release --bin shim

FROM debian:bookworm-slim AS runtime
WORKDIR /app
# 2. CHANGE: Copy 'shim' instead of 'client'
COPY --from=builder /app/target/release/shim /usr/local/bin
EXPOSE 8000
# 3. CHANGE: Run 'shim' as the entrypoint
ENTRYPOINT ["/usr/local/bin/shim"]
