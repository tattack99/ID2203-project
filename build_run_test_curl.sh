#!/bin/bash
cargo build --bin client && \
docker compose -f ./build_scripts/docker-compose.shim.yml down && \
docker compose -f ./build_scripts/docker-compose.shim.yml build --no-cache && \
docker compose -f ./build_scripts/docker-compose.shim.yml up
./test_curl.sh