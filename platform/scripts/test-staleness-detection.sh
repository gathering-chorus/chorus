#!/bin/bash
# test-staleness-detection.sh — Tests for team-scan staleness detection (#2031)
set -uo pipefail
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

PASS=0
FAIL=0

echo "=== team-scan staleness detection tests ==="
echo ""

# Test 1: Fresh state does NOT show STALE
echo "Test 1: Fresh state has no STALE flag"
${CHORUS_ROOT}/platform/scripts/role-state silas building card=2031 2>/dev/null
rm -f /tmp/claude-team-scan/silas-last-scan; SCAN=$(DEPLOY_ROLE=silas bash ${CHORUS_ROOT}/platform/scripts/werk-init.sh --scan silas 2>&1 || true)
if echo "$SCAN" | grep -q "STALE"; then
  echo "  FAIL: fresh state shows STALE"
  ((FAIL++))
else
  echo "  PASS: no STALE on fresh state"
  ((PASS++))
fi

# Test 2: Stale state DOES show STALE (simulate by backdating state file)
echo "Test 2: Stale state shows STALE flag"
STATE_FILE="/tmp/claude-team-scan/kade-declared.json"
if [ -f "$STATE_FILE" ]; then
  # Save original
  cp "$STATE_FILE" "${STATE_FILE}.bak"
  # Backdate the ts by 1 hour
  python3 -c "
import json, time
d = json.load(open('$STATE_FILE'))
d['ts'] = int(time.time()) - 3600
json.dump(d, open('$STATE_FILE','w'))
" 2>/dev/null
  rm -f /tmp/claude-team-scan/silas-last-scan; SCAN=$(DEPLOY_ROLE=silas bash ${CHORUS_ROOT}/platform/scripts/werk-init.sh --scan silas 2>&1 || true)
  if echo "$SCAN" | grep -q "STALE"; then
    echo "  PASS: stale state shows STALE"
    ((PASS++))
  else
    echo "  FAIL: stale state missing STALE flag"
    ((FAIL++))
  fi
  # Restore
  mv "${STATE_FILE}.bak" "$STATE_FILE"
else
  echo "  SKIP: no kade state file to test with"
  ((FAIL++))
fi

# Test 3: STALE flag appears next to role name
echo "Test 3: STALE appears on role's line"
if [ -f "$STATE_FILE" ]; then
  cp "$STATE_FILE" "${STATE_FILE}.bak"
  python3 -c "
import json, time
d = json.load(open('$STATE_FILE'))
d['ts'] = int(time.time()) - 3600
json.dump(d, open('$STATE_FILE','w'))
" 2>/dev/null
  rm -f /tmp/claude-team-scan/silas-last-scan; SCAN=$(DEPLOY_ROLE=silas bash ${CHORUS_ROOT}/platform/scripts/werk-init.sh --scan silas 2>&1 || true)
  if echo "$SCAN" | grep "kade" | grep -q "STALE"; then
    echo "  PASS: STALE on kade's line"
    ((PASS++))
  else
    echo "  FAIL: STALE not on kade's line"
    ((FAIL++))
  fi
  mv "${STATE_FILE}.bak" "$STATE_FILE"
else
  echo "  SKIP: no kade state file"
  ((FAIL++))
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
exit $FAIL
