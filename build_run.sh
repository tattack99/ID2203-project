#!/bin/bash

docker compose -f ./build_scripts/docker-compose.shim.yml down && \
docker compose -f ./build_scripts/docker-compose.shim.yml build && \
docker compose -f ./build_scripts/docker-compose.shim.yml up