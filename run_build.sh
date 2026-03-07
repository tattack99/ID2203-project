#!/bin/bash
set -e
docker compose -f ./build_scripts/docker-compose.shim.yml down --remove-orphans
docker compose -f ./build_scripts/docker-compose.shim.yml build
docker compose -f ./build_scripts/docker-compose.shim.yml up
