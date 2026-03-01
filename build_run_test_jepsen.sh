#!/bin/bash

# build client & server nodes
cargo build --bin client && \
docker compose -f ./build_scripts/docker-compose.shim.yml down && \
docker compose -f ./build_scripts/docker-compose.shim.yml build --no-cache && \
docker compose -f ./build_scripts/docker-compose.shim.yml up -d

# jepsen linearizability test
cd paxos-shim-test
lein clean
lein run