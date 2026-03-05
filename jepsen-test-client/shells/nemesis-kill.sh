#!/bin/bash
ID=1
NODE="s$ID"
PORT=8000

echo "1. Checking Status & Port"
docker exec $NODE ps aux | grep server
docker exec $NODE ss -tlnp | grep ":$PORT"

echo "2. Killing Server"
docker exec $NODE pkill -9 -f server || true
docker exec $NODE fuser -k $PORT/tcp || true

echo "3. Verifying Port is FREE"
docker exec $NODE ss -tlnp | grep ":$PORT" || echo "Port $PORT is now empty"

echo "4. Restarting Server (Jepsen Style)"
docker exec $NODE sh -c "export SERVER_CONFIG_FILE=/app/server-config.toml && \
                         export CLUSTER_CONFIG_FILE=/app/cluster-config.toml && \
                         cd /app && \
                         /usr/local/bin/server > /app/logs/nemesis.log 2>&1 &"

echo "5. Final Verification"
sleep 2
docker exec $NODE ps aux | grep server
docker exec $NODE ss -tlnp | grep ":$PORT"
