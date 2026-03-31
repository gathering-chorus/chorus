#!/usr/bin/env bash
# pulse-integration-test.sh — End-to-end pulse level verification (#1922)
# Emits a test event → checks chorus.log → checks Loki → checks Clearing
# Usage: bash pulse-integration-test.sh

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHORUS_LOG="$SCRIPT_DIR/chorus-log.sh"
LOKI_URL="http://localhost:3102"
CLEARING_URL="http://localhost:3470"

# Unique marker so we can find our test event
MARKER="pulse-test-$(date +%s)-$$"
PASS=0
FAIL=0
TOTAL=0

result() {
  TOTAL=$((TOTAL + 1))
  if [ "$1" = "PASS" ]; then
    PASS=$((PASS + 1))
    echo "  ✅ $2"
  else
    FAIL=$((FAIL + 1))
    echo "  ❌ $2"
  fi
}

echo "=== Pulse Integration Test ==="
echo "Marker: $MARKER"
echo ""

# Step 1: Emit event with --level critical
echo "1. Emit critical event..."
if bash "$CHORUS_LOG" "pulse.test" "kade" "marker=$MARKER" --level critical 2>/dev/null; then
  result "PASS" "Event emitted with --level critical"
else
  result "FAIL" "chorus-log.sh failed to emit"
fi

# Step 2: Verify in chorus.log file
echo "2. Verify in chorus.log..."
sleep 1
LOG_FILE="$SCRIPT_DIR/../logs/chorus.log"
if grep -q "$MARKER" "$LOG_FILE" 2>/dev/null; then
  LINE=$(grep "$MARKER" "$LOG_FILE" | tail -1)
  if echo "$LINE" | grep -q '"level":"critical"'; then
    result "PASS" "Event in chorus.log with level=critical"
  else
    result "FAIL" "Event in chorus.log but level field missing"
  fi
else
  result "FAIL" "Event not found in chorus.log"
fi

# Step 3: Verify in Loki
echo "3. Verify in Loki..."
sleep 3
LOKI_RESP=$(curl -s -G "$LOKI_URL/loki/api/v1/query" \
  --data-urlencode "query={job=\"chorus-events\"} |= \"$MARKER\"" \
  --data-urlencode "limit=1" 2>/dev/null)
LOKI_COUNT=$(echo "$LOKI_RESP" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('data',{}).get('result',[])))" 2>/dev/null || echo "0")
if [ "$LOKI_COUNT" -gt 0 ] 2>/dev/null; then
  result "PASS" "Event found in Loki"
else
  result "FAIL" "Event not in Loki (may need Promtail delay)"
fi

# Step 4: Post to Clearing with level field
echo "4. Post to Clearing..."
CLEAR_RESP=$(curl -s -X POST "$CLEARING_URL/api/message" \
  -H 'Content-Type: application/json' \
  -d "{\"from\": \"kade\", \"text\": \"[critical] Pulse test: $MARKER\", \"level\": \"critical\"}" 2>/dev/null)
if echo "$CLEAR_RESP" | grep -q '"ok":true'; then
  result "PASS" "Clearing accepted message with level"
else
  result "FAIL" "Clearing rejected message: $CLEAR_RESP"
fi

# Step 5: Verify in Clearing messages API
echo "5. Verify in Clearing messages..."
sleep 1
MSGS=$(curl -s "$CLEARING_URL/api/messages" 2>/dev/null)
if echo "$MSGS" | grep -q "$MARKER"; then
  result "PASS" "Event in Clearing messages"
else
  result "FAIL" "Event not in Clearing messages"
fi

# Step 6: Verify level field preserved in Clearing
echo "6. Verify level field..."
if echo "$MSGS" | python3 -c "
import sys,json
msgs = json.load(sys.stdin)
found = [m for m in msgs if '$MARKER' in (m.get('text',''))]
if found and found[0].get('level') == 'critical':
    print('HAS_LEVEL')
elif found:
    print('NO_LEVEL')
else:
    print('NOT_FOUND')
" 2>/dev/null | grep -q "HAS_LEVEL"; then
  result "PASS" "Clearing message has level=critical"
else
  result "FAIL" "Clearing message missing level field"
fi

echo ""
echo "=== Results: $PASS pass, $FAIL fail, $TOTAL total ==="
[ "$FAIL" -eq 0 ] && echo "PULSE PATH: ALL LAYERS VERIFIED" || echo "PULSE PATH: BROKEN — check failed layers above"
exit "$FAIL"
