#!/bin/bash
set -e

# Clear persistent state first (interactive sudo prompt)
sudo rm -rf build_scripts/logs/server-*-snapshot.json build_scripts/logs/omnipaxos-node-*

# Start cluster in background (skip the sudo rm since we just did it)
docker compose -f ./build_scripts/docker-compose.shim.yml down > /tmp/cluster.log 2>&1
docker compose -f ./build_scripts/docker-compose.shim.yml build >> /tmp/cluster.log 2>&1
docker compose -f ./build_scripts/docker-compose.shim.yml up -d >> /tmp/cluster.log 2>&1

echo "Waiting 20s for cluster to elect a leader..."
sleep 20
echo "Cluster ready."

cd jepsen-test-client
LEIN_JVM_OPTS="-Xmx4g" lein run -m nemesis-partition.core test --time-limit 60 --concurrency 3
