#!/bin/bash
# Persistence + leader crash recovery test.
# Write a key, kill the LEADER, restart it, verify data via all nodes.

log() { echo "[$(date '+%H:%M:%S')] $*"; }

log "=== Clean old state ==="
for C in s1 s2 s3; do
  docker exec "$C" rm -rf /app/logs/server-*-snapshot.json /app/logs/omnipaxos-node-* /app/logs/recovery.log 2>/dev/null || true
done

# Detect leader from logs
LEADER_ID=""
for C in s1 s2 s3; do
  LINE=$(docker logs "$C" 2>&1 | grep "Leader:" | tail -1 || true)
  if [ -n "$LINE" ]; then
    LEADER_ID=$(echo "$LINE" | grep -oP 'Leader: \K[0-9]+')
    break
  fi
done
if [ -z "$LEADER_ID" ]; then
  log "Cannot detect leader. Is the cluster running?"
  exit 1
fi
log "Leader is node $LEADER_ID"

# Map IDs to containers/ports
declare -A CMAP=( [1]="s1" [2]="s2" [3]="s3" )
declare -A PMAP=( [1]=3001 [2]=3002 [3]=3003 )
LEADER_C="${CMAP[$LEADER_ID]}"
LEADER_P="${PMAP[$LEADER_ID]}"

# Pick a surviving port for writes during downtime
SURV_P=""
for ID in 1 2 3; do
  if [ "$ID" != "$LEADER_ID" ]; then
    SURV_P="${PMAP[$ID]}"
    break
  fi
done

log "=== PUT test=hello via :$SURV_P ==="
RESULT=$(curl -s --max-time 10 -X POST "http://localhost:$SURV_P/put" \
  -H 'Content-Type: application/json' \
  -d '{"key":"test","value":"hello"}' || echo "CURL_FAILED")
log "Response: '$RESULT'"
if [ "$RESULT" != "Ok" ]; then
  log "PUT failed. Is cluster ready?"
  exit 1
fi
sleep 1

log "=== Verify from all nodes ==="
for ID in 1 2 3; do
  P="${PMAP[$ID]}"
  VAL=$(curl -s --max-time 10 "http://localhost:$P/get/test" || echo "CURL_FAILED")
  log "  :$P => '$VAL'"
  if [ "$VAL" != "hello" ]; then
    log "FAIL pre-crash read"
    exit 1
  fi
done

log "=== Kill leader $LEADER_C ==="
docker exec "$LEADER_C" pkill -9 -f server || true
sleep 2
docker exec "$LEADER_C" pkill -9 -f server 2>/dev/null || true
sleep 5

log "=== Write new key while leader is down ==="
RESULT=$(curl -s --max-time 15 -X POST "http://localhost:$SURV_P/put" \
  -H 'Content-Type: application/json' \
  -d '{"key":"test2","value":"world"}' || echo "CURL_FAILED")
log "PUT test2=world via :$SURV_P => '$RESULT'"
# This might fail or timeout during re-election, that's ok
sleep 2

log "=== Restart $LEADER_C ==="
docker exec -d "$LEADER_C" sh -c "exec /usr/local/bin/server >> /app/logs/recovery.log 2>&1"
sleep 3

if docker exec "$LEADER_C" pgrep -f server > /dev/null 2>&1; then
  log "$LEADER_C server is running"
else
  log "FAIL: $LEADER_C did not start. recovery.log:"
  docker exec "$LEADER_C" cat /app/logs/recovery.log 2>&1
  exit 1
fi

log "Waiting 30s for rejoin..."
sleep 30

log "=== recovery.log ==="
docker exec "$LEADER_C" tail -25 /app/logs/recovery.log 2>&1

log "=== Read test=hello from all nodes ==="
ALL_PASS=true
for ID in 1 2 3; do
  P="${PMAP[$ID]}"
  VAL="ERROR"
  for i in $(seq 1 8); do
    VAL=$(curl -s --max-time 5 "http://localhost:$P/get/test" 2>/dev/null || echo "CURL_FAILED")
    if [ "$VAL" = "hello" ]; then break; fi
    sleep 3
  done
  log "  :$P => '$VAL'"
  if [ "$VAL" != "hello" ]; then ALL_PASS=false; fi
done

if $ALL_PASS; then
  log "=== PASS: data survived leader crash ==="
else
  log "=== FAIL ==="
  exit 1
fi