#!/bin/bash

# build client & server nodes
cargo run --bin client && \
cargo run --bin server && \
docker compose -f ./build_scripts/docker-compose.shim.yml down && \
docker compose -f ./build_scripts/docker-compose.shim.yml build --no-cache && \
docker compose -f ./build_scripts/docker-compose.shim.yml up -d

# jepsen linearizability test
cd paxos-shim-test
lein clean
lein run check 