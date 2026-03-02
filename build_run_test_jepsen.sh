#!/bin/bash
set -e # Exit if any command fails

# 1. Build and Start
cargo build --bin client
cargo build --bin server
docker compose -f ./build_scripts/docker-compose.shim.yml down
docker compose -f ./build_scripts/docker-compose.shim.yml build --no-cache
docker compose -f ./build_scripts/docker-compose.shim.yml up -d

echo "⏳ Waiting for SSH and Paxos nodes to initialize..."
# This uses bash's internal socket handling to wait for port 2222
for i in {1..20}; do
  if (echo > /dev/tcp/localhost/2222) >/dev/null 2>&1; then
    echo "✅ s1 SSH is up!"
    break
  fi
  echo "Still waiting for s1 SSH (attempt $i)..."
  sleep 1
done


# 3. Run Jepsen
cd paxos-shim-test
# Use 'check' as default if no argument is provided
lein run "${1:-check}"