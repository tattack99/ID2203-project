#!/bin/bash
# test_curl.sh

KEY=$((1 + RANDOM % 100))
VAL=$((1000 + RANDOM % 9000))

echo "🔢 Testing Numeric Replication (Key: $KEY, Val: $VAL)"

# 1. PUT to C1 (Port 3001)
curl -s -X POST http://localhost:3001/put \
  -H "Content-Type: application/json" \
  -d "{\"key\": \"$KEY\", \"value\": \"$VAL\"}" > /dev/null

echo "✅ Sent to C1. Fetching from C2 (Port 3002)..."

# 2. GET from C2 - this blocks until Paxos consensus is reached
RESULT=$(curl -s http://localhost:3002/get/$KEY)

echo "📊 Result from C2: $RESULT"

if [ "$RESULT" == "$VAL" ]; then
    echo "🎊 SUCCESS: Cluster is synchronized!"
else
    echo "❌ ERROR: Mismatch."
fi
