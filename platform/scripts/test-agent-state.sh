#!/bin/bash
# test-agent-state.sh — Tests for agent-state.sh (#2009)
# AC: Script manages LaunchAgent start/stop/restart/status for all agents
# Tests what Jeff experiences when running the tool.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
AGENT_STATE="$SCRIPT_DIR/agent-state.sh"
PASS=0
FAIL=0

assert_contains() {
  local label="$1" needle="$2" haystack="$3"
  if echo "$haystack" | grep -qi "$needle"; then
    echo "  PASS: $label"
    PASS=$((PASS+1))
  else
    echo "  FAIL: $label (expected to contain '$needle')"
    FAIL=$((FAIL+1))
  fi
}

echo "=== agent-state.sh tests ==="
echo ""

# --- Test 1: Script exists and is executable ---
echo "Test 1: Script exists and is executable"
if [ -x "$AGENT_STATE" ]; then
  echo "  PASS: agent-state.sh is executable"
  PASS=$((PASS+1))
else
  echo "  FAIL: agent-state.sh not found or not executable"
  FAIL=$((FAIL+1))
fi

# --- Test 2: No args prints usage ---
echo "Test 2: No args prints usage"
output=$("$AGENT_STATE" 2>&1 || true)
assert_contains "usage shown" "Usage:" "$output"

# --- Test 3: health shows agent counts ---
echo "Test 3: health shows running/crashed/stopped counts"
output=$("$AGENT_STATE" health 2>&1 || true)
assert_contains "shows total" "total" "$output"
assert_contains "shows running" "running" "$output"
assert_contains "shows critical services" "Critical services" "$output"

# --- Test 4: status lists agents with state ---
echo "Test 4: status lists agents with PID and state"
output=$("$AGENT_STATE" status 2>&1 || true)
assert_contains "shows header" "AGENT" "$output"
assert_contains "shows chorus agents" "com.chorus" "$output"
assert_contains "shows gathering agents" "com.gathering" "$output"

# --- Test 5: status filter ---
echo "Test 5: status filter narrows output"
output=$("$AGENT_STATE" status api 2>&1 || true)
assert_contains "shows api" "api" "$output"
if echo "$output" | grep -q "com.gathering.fuseki"; then
  echo "  FAIL: filter should exclude unrelated agents"
  FAIL=$((FAIL+1))
else
  echo "  PASS: filter excludes unrelated agents"
  PASS=$((PASS+1))
fi

# --- Test 6: short name resolves ---
echo "Test 6: start resolves short name"
output=$("$AGENT_STATE" start heartbeat 2>&1 || true)
assert_contains "resolves to full label" "com.chorus.heartbeat" "$output"

# --- Test 7: bad service name errors ---
echo "Test 7: nonexistent service errors clearly"
output=$("$AGENT_STATE" start fake-service-xyz 2>&1 || true)
assert_contains "clear error" "No agent found" "$output"

# --- Test 8: orphans scan runs ---
echo "Test 8: orphans scan completes"
output=$(echo "n" | "$AGENT_STATE" orphans 2>&1 || true)
assert_contains "scans" "Scanning\|orphan\|No orphans" "$output"

# --- Test 9: health shows duplicate detection ---
echo "Test 9: health reports duplicates section"
output=$("$AGENT_STATE" health 2>&1 || true)
assert_contains "duplicate section" "Duplicates" "$output"

echo ""

# --- Test 15 (#3750): resolve finds an UNLOADED agent via its plist ---
echo "Test 15 (#3750): resolve falls back to plist for unloaded agents"
FAKE_HOME=$(mktemp -d)
mkdir -p "$FAKE_HOME/Library/LaunchAgents"
touch "$FAKE_HOME/Library/LaunchAgents/com.chorus.test-3750-fixture.plist"
output=$(HOME="$FAKE_HOME" "$AGENT_STATE" resolve test-3750-fixture 2>&1 || true)
assert_contains "unloaded agent resolves via plist" "com.chorus.test-3750-fixture" "$output"

# --- Test 16 (#3750 negative proof): no plist + not loaded = resolve FAILS ---
echo "Test 16 (#3750): resolve refuses when neither loaded nor plist exists"
if HOME="$FAKE_HOME" "$AGENT_STATE" resolve does-not-exist-anywhere >/dev/null 2>&1; then
  echo "  FAIL: resolve should exit 1 for an unresolvable name"
  FAIL=$((FAIL+1))
else
  echo "  PASS: unresolvable name exits non-zero"
  PASS=$((PASS+1))
fi
rm -rf "$FAKE_HOME"


echo "=== Results: $PASS passed, $FAIL failed ==="
exit $FAIL
