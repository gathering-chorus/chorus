#!/usr/bin/env bash
# @test-type: integration — auto-classified (#3528 sweep); service-hitting=integration(skip-if-absent), static-guard=unit
: "${CHORUS_ROOT:=$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/../.." && pwd)}"
# Test: deep-health.sh exists, runs, and detects failures (#2228)
# RED before script exists. GREEN after.
set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-${CHORUS_ROOT}}"
SCRIPT="$CHORUS_ROOT/platform/scripts/deep-health.sh"
PASS=0
FAIL=0

run_test() {
  local name="$1"; shift
  if "$@" 2>/dev/null; then
    echo "  PASS: $name"
    PASS=$((PASS + 1))
  else
    echo "  FAIL: $name"
    FAIL=$((FAIL + 1))
  fi
}

echo "=== deep-health tests ==="

# 1. Script exists and is executable
run_test "script exists and is executable" test -x "$SCRIPT"

# 2. Script runs without crashing.
# #4057: this asserted exit 0, which quietly meant "and the machine is healthy".
# Now that a silent LaunchAgent is a failure, deep-health legitimately exits 1 on
# a box with a real problem, and this test would fail for the wrong reason. Ran
# = exited 0 (healthy) or 1 (degraded). A crash is anything else.
run_test "script runs" bash -c 'bash "$0" >/dev/null 2>&1; rc=$?; [ "$rc" -le 1 ]' "$SCRIPT"

# 3. Output contains either "all checks passed" or "failure(s)"
OUTPUT=$(bash "$SCRIPT" 2>/dev/null || true)
run_test "output is structured" echo "$OUTPUT" | grep -qE "(all checks passed|failure)"

# 4. JSON output exists and has correct structure
JSON_FILE="/tmp/deep-health-latest.json"
bash "$SCRIPT" 2>/dev/null || true
run_test "JSON output written" test -f "$JSON_FILE"

if [ -f "$JSON_FILE" ]; then
  # 5. JSON has warnings array (separate from failures)
  run_test "JSON has warnings array" python3 -c "import json; d=json.load(open('$JSON_FILE')); assert 'warnings' in d and isinstance(d['warnings'], list)"

  # 6. #4057 INVERTED. This used to assert log-freshness could only be a
  # warning. That rule is what made health say "warning, 28" forever: 20 of
  # those were logs belonging to LaunchAgents retired months ago, and a warning
  # nobody can clear means the lamp never goes green and never goes red.
  # deep-health now measures LOADED agents, so a stale log means a live agent
  # went silent — which is a failure.
  run_test "log-freshness is a failure, never a warning" python3 -c "
import json
d = json.load(open('$JSON_FILE'))
for w in d.get('warnings', []):
    assert 'log-freshness' not in w, f'log-freshness still in warnings: {w}'
"

  # 7. #4057 INVERTED. A log-freshness finding IS a real failure now, so it must
  # degrade the status on its own. The old assertion explicitly excused it, which
  # is how a dead agent could sit unobserved without the status ever moving.
  run_test "a log-freshness failure degrades status" python3 -c "
import json
d = json.load(open('$JSON_FILE'))
stale = [f for f in d.get('details', []) if 'log-freshness' in f]
if stale:
    assert d['status'] == 'degraded', f'log-freshness failure did not degrade status: {stale}'
"

  # 8. Nudge path resolves correctly (CHORUS_ROOT includes chorus/)
  run_test "nudge path resolves" python3 -c "
import json
d = json.load(open('$JSON_FILE'))
nudge_missing = [f for f in d.get('details', []) if 'nudge' in f and 'not found' in f]
assert len(nudge_missing) == 0, f'nudge not found: {nudge_missing}'
"

  # 9. LanceDB staleness is a failure, not a warning
  run_test "lancedb stale is failure not warning" python3 -c "
import json
d = json.load(open('$JSON_FILE'))
for w in d.get('warnings', []):
    assert 'lancedb' not in w, f'lancedb in warnings instead of failures: {w}'
"
fi

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
