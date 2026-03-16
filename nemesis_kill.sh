#!/bin/bash
set -e

# Clear persistent state first (interactive sudo prompt)
sudo rm -rf build_scripts/logs/server-*-snapshot.json build_scripts/logs/omnipaxos-node-*

# Start cluster in background (skip the sudo rm since we just did it)
docker compose -f ./build_scripts/docker-compose.shim.yml down > /tmp/cluster.log 2>&1
docker compose -f ./build_scripts/docker-compose.shim.yml build >> /tmp/cluster.log 2>&1
docker compose -f ./build_scripts/docker-compose.shim.yml up -d >> /tmp/cluster.log 2>&1

echo "Waiting for leader election..."
READY=0
for i in $(seq 1 30); do
  LOGS=$(docker compose -f ./build_scripts/docker-compose.shim.yml logs 2>&1)
  if echo "$LOGS" | grep -q "Leader fully initialized"; then
    LEADER=$(echo "$LOGS" | grep "Leader:" | tail -1)
    echo "[OK] Leader elected — $LEADER"
    READY=1
    break
  fi
  echo "  ... waiting for leader ($i/30)"
  sleep 2
done
if [ $READY -eq 0 ]; then
  echo "[ERROR] Timed out waiting for leader election"
  exit 1
fi
sleep 3  # brief pause for shim HTTP readiness
echo "Cluster ready — starting Jepsen."

cd jepsen-test-client
LEIN_JVM_OPTS="-Xmx4g" lein run -m nemesis-kill.core test --time-limit 300 --concurrency 4
