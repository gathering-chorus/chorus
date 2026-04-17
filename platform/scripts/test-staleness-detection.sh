#!/bin/bash
# test-staleness-detection.sh — Tests for team-scan staleness detection.
# #2031 added STALE flag. #2224 tightened rule: stale only if BOTH the
# declared-state ts >45min old AND last observation >5min old.
#
# Isolation (#2149 pair w/ silas): the scan reads from CACHE_DIR. Instead of
# reading/mutating /tmp/claude-team-scan (the live team state for the running
# session), point werk-init.sh at a fresh fixture directory and populate it
# with controlled files. Assertions then reflect the STALE logic, not the
# machine's current activity.
set -uo pipefail
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

PASS=0
FAIL=0

FIXTURE_ROOT=$(mktemp -d -t test-staleness-fixtures.XXXX)
cleanup() {
  [ -n "$FIXTURE_ROOT" ] && [ -d "$FIXTURE_ROOT" ] && rm -rf "$FIXTURE_ROOT"
}
trap cleanup EXIT

# Start each test with a clean fixture dir — avoids rate-limiter collisions on
# ${role}-last-scan and any other cross-test state leakage.
new_fixture() {
  FIXTURE_DIR=$(mktemp -d "$FIXTURE_ROOT/test.XXXX")
}

# Write a declared.json with given ts (unix epoch).
write_declared() {
  local role="$1" ts="$2" state="$3" card="$4"
  python3 - "$FIXTURE_DIR/${role}-declared.json" "$role" "$ts" "$state" "$card" <<'PY'
import json, sys
path, role, ts, state, card = sys.argv[1:6]
d = {"role": role, "state": state, "ts": int(ts), "pid": 1}
if card:
    d["card"] = int(card)
with open(path, "w") as f:
    json.dump(d, f)
PY
}

# Write an observations.jsonl single line with given ISO ts.
write_observation() {
  local role="$1" iso_ts="$2"
  echo "{\"role\":\"$role\",\"ts\":\"$iso_ts\",\"digest\":\"fixture observation\"}" \
    > "$FIXTURE_DIR/${role}-observations.jsonl"
}

now_epoch() { date +%s; }
iso_now() { date -u +"%Y-%m-%dT%H:%M:%SZ"; }
iso_hours_ago() {
  # macOS date: -v can subtract; GNU: date -d. Use python for portability.
  python3 -c "
from datetime import datetime, timezone, timedelta
print((datetime.now(timezone.utc) - timedelta(hours=$1)).strftime('%Y-%m-%dT%H:%M:%SZ'))
"
}

# Run the scan against the fixture dir. Returns scan stdout.
scan_fixture() {
  local scanner="$1"
  CACHE_DIR="$FIXTURE_DIR" DEPLOY_ROLE="$scanner" \
    bash "${CHORUS_ROOT}/platform/scripts/werk-init.sh" --scan "$scanner" 2>&1 || true
}

echo "=== team-scan staleness detection tests ==="
echo ""

# Test 1: Fresh state does NOT show STALE.
# Fixture: silas declared building (fresh), kade declared building (fresh),
# kade observation fresh. Neither role should carry [STALE].
echo "Test 1: Fresh state has no STALE flag"
new_fixture
NOW=$(now_epoch)
write_declared silas "$NOW" building 2031
write_declared kade "$NOW" building 9999
write_observation kade "$(iso_now)"
SCAN=$(scan_fixture silas)
if echo "$SCAN" | grep -q "STALE"; then
  echo "  FAIL: fresh state shows STALE — scan:"
  echo "$SCAN" | sed 's/^/    /'
  ((FAIL++))
else
  echo "  PASS: no STALE on fresh state"
  ((PASS++))
fi

# Test 2: Stale state + stale observation DOES show STALE.
# Backdate kade's declared ts to 1h ago AND kade's observation to 1h ago.
# Scan from silas (fresh scanner), kade should carry [STALE].
echo "Test 2: Stale state + stale observation shows STALE flag"
new_fixture
STALE_EPOCH=$((NOW - 3600))
write_declared silas "$NOW" building 2031
write_declared kade "$STALE_EPOCH" building 9999
write_observation kade "$(iso_hours_ago 1)"
SCAN=$(scan_fixture silas)
if echo "$SCAN" | grep -q "STALE"; then
  echo "  PASS: stale state shows STALE"
  ((PASS++))
else
  echo "  FAIL: stale state missing STALE flag — scan:"
  echo "$SCAN" | sed 's/^/    /'
  ((FAIL++))
fi

# Test 3: STALE flag appears specifically on the stale role's line.
echo "Test 3: STALE appears on role's line"
new_fixture
write_declared silas "$NOW" building 2031
write_declared kade "$STALE_EPOCH" building 9999
write_observation kade "$(iso_hours_ago 1)"
SCAN=$(scan_fixture silas)
if echo "$SCAN" | grep "kade" | grep -q "STALE"; then
  echo "  PASS: STALE on kade's line"
  ((PASS++))
else
  echo "  FAIL: STALE not on kade's line — scan:"
  echo "$SCAN" | sed 's/^/    /'
  ((FAIL++))
fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
exit $FAIL
