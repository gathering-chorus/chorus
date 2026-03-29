#!/usr/bin/env bash
# spine-e2e.sh — End-to-end test for the spine event pipeline (#1075)
# Verifies: emit (chorus-log.sh) → chorus.log → index (chorus-index) → query (API)
# Target: runs in under 10s
set -euo pipefail

SCRIPTS_DIR="$(cd "$(dirname "$0")/../scripts" && pwd)"
MSG_SCRIPTS="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts"
CHORUS_LOG="/Users/jeffbridwell/CascadeProjects/chorus/platform/logs/chorus.log"
API_URL="http://localhost:3340"
PASS=0
FAIL=0
TOTAL=0

pass() { PASS=$((PASS+1)); TOTAL=$((TOTAL+1)); echo "  PASS: $1"; }
fail() { FAIL=$((FAIL+1)); TOTAL=$((TOTAL+1)); echo "  FAIL: $1"; }

echo "=== Spine E2E Test ==="
echo ""

# --- Step 1: Emit a unique spine event ---
TEST_ID="e2e-$(date +%s)-$$"
echo "Step 1: Emit spine event (marker=$TEST_ID)"
OUTPUT=$("$MSG_SCRIPTS/chorus-log.sh" spine.e2e.test silas marker="$TEST_ID" 2>&1)
if echo "$OUTPUT" | grep -q "spine.e2e.test"; then
  pass "chorus-log.sh emitted event"
else
  fail "chorus-log.sh did not emit event: $OUTPUT"
fi

# --- Step 2: Verify event in chorus.log ---
echo "Step 2: Verify event in chorus.log"
if grep -q "$TEST_ID" "$CHORUS_LOG"; then
  pass "Event found in chorus.log"
  # Verify it's valid JSON
  MATCH=$(grep "$TEST_ID" "$CHORUS_LOG" | tail -1)
  if echo "$MATCH" | python3 -c "import sys,json; json.load(sys.stdin)" 2>/dev/null; then
    pass "Event is valid JSON"
  else
    fail "Event is not valid JSON: $MATCH"
  fi
  # Verify fields
  EVENT_NAME=$(echo "$MATCH" | python3 -c "import sys,json; print(json.load(sys.stdin).get('event',''))")
  if [ "$EVENT_NAME" = "spine.e2e.test" ]; then
    pass "Event name correct"
  else
    fail "Event name wrong: expected 'spine.e2e.test', got '$EVENT_NAME'"
  fi
else
  fail "Event NOT found in chorus.log"
fi

# --- Step 3: Trigger indexer to pick up the event ---
echo "Step 3: Index spine events into chorus"
# The session indexer runs on sessions, not chorus.log.
# The artifact indexer handles briefs/activity/state files.
# Check if spine events have an indexer path:
if [ -f "$SCRIPTS_DIR/chorus-index-spine.sh" ]; then
  "$SCRIPTS_DIR/chorus-index-spine.sh" >/dev/null 2>&1
  pass "Spine indexer ran"
else
  fail "No spine indexer exists (chorus-index-spine.sh missing) — chorus.log events are not indexed"
fi

# --- Step 4: Query the API for the event ---
echo "Step 4: Query chorus API for event"
API_HEALTH=$(curl -s --max-time 5 "$API_URL/health" 2>/dev/null || echo "UNREACHABLE")
if echo "$API_HEALTH" | grep -q '"ok"'; then
  pass "Chorus API is healthy"
  # Search for our test marker
  SEARCH_RESULT=$(curl -s --max-time 5 "$API_URL/api/chorus/search?q=$TEST_ID&limit=5" 2>/dev/null || echo "{}")
  RESULT_COUNT=$(echo "$SEARCH_RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('results',[])))" 2>/dev/null || echo "0")
  if [ "$RESULT_COUNT" -gt 0 ]; then
    pass "Event queryable via API ($RESULT_COUNT results)"
  else
    fail "Event NOT queryable via API — spine events not reaching the index"
  fi
else
  fail "Chorus API unreachable: $API_HEALTH"
fi

# --- Step 5: Session start event ---
echo "Step 5: session_start lifecycle event"
START_MARKER="e2e-start-$(date +%s)-$$"
OUTPUT=$("$MSG_SCRIPTS/chorus-log.sh" session_start silas marker="$START_MARKER" 2>&1)
if echo "$OUTPUT" | grep -q "session"; then
  pass "session_start emitted"
else
  fail "session_start emission failed: $OUTPUT"
fi
if grep -q "$START_MARKER" "$CHORUS_LOG"; then
  MATCH=$(grep "$START_MARKER" "$CHORUS_LOG" | tail -1)
  EVT=$(echo "$MATCH" | python3 -c "import sys,json; print(json.load(sys.stdin).get('event',''))")
  ROLE=$(echo "$MATCH" | python3 -c "import sys,json; print(json.load(sys.stdin).get('role',''))")
  if echo "$EVT" | grep -q "session" && [ "$ROLE" = "silas" ]; then
    pass "session_start fields correct (event=$EVT role=$ROLE)"
  else
    fail "session_start fields wrong: event=$EVT role=$ROLE"
  fi
else
  fail "session_start not in chorus.log"
fi

# --- Step 6: Session end event ---
echo "Step 6: session_end lifecycle event"
END_MARKER="e2e-end-$(date +%s)-$$"
OUTPUT=$("$MSG_SCRIPTS/chorus-log.sh" session_end silas marker="$END_MARKER" cost="\$0.00" 2>&1)
if echo "$OUTPUT" | grep -q "session"; then
  pass "session_end emitted"
else
  fail "session_end emission failed: $OUTPUT"
fi
if grep -q "$END_MARKER" "$CHORUS_LOG"; then
  MATCH=$(grep "$END_MARKER" "$CHORUS_LOG" | tail -1)
  EVT=$(echo "$MATCH" | python3 -c "import sys,json; print(json.load(sys.stdin).get('event',''))")
  COST=$(echo "$MATCH" | python3 -c "import sys,json; print(json.load(sys.stdin).get('cost',''))")
  if echo "$EVT" | grep -q "session" && [ -n "$COST" ]; then
    pass "session_end fields correct (event=$EVT cost=$COST)"
  else
    fail "session_end fields wrong: event=$EVT cost=$COST"
  fi
else
  fail "session_end not in chorus.log"
fi

# --- Step 7: Index + query session lifecycle events ---
echo "Step 7: Session events queryable after indexing"
"$SCRIPTS_DIR/chorus-index-spine.sh" >/dev/null 2>&1
START_RESULT=$(curl -s --max-time 5 "$API_URL/api/chorus/search?q=$START_MARKER&limit=1" 2>/dev/null || echo "{}")
START_COUNT=$(echo "$START_RESULT" | python3 -c "import sys,json; print(len(json.load(sys.stdin).get('results',[])))" 2>/dev/null || echo "0")
if [ "$START_COUNT" -gt 0 ]; then
  pass "session_start queryable via API"
else
  fail "session_start NOT queryable via API"
fi
END_RESULT=$(curl -s --max-time 5 "$API_URL/api/chorus/search?q=$END_MARKER&limit=1" 2>/dev/null || echo "{}")
END_COUNT=$(echo "$END_RESULT" | python3 -c "import sys,json; print(len(json.load(sys.stdin).get('results',[])))" 2>/dev/null || echo "0")
if [ "$END_COUNT" -gt 0 ]; then
  pass "session_end queryable via API"
else
  fail "session_end NOT queryable via API"
fi

# --- Summary ---
echo ""
echo "=== Results: $PASS/$TOTAL passed, $FAIL failed ==="
if [ "$FAIL" -gt 0 ]; then
  echo ""
  echo "Gap identified: some spine events not flowing through the full pipeline."
  exit 1
fi
