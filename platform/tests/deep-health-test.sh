#!/usr/bin/env bash
# Test: deep-health.sh exists, runs, and detects failures (#2228)
# RED before script exists. GREEN after.
set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
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

# 2. Script runs without crashing
run_test "script runs" bash "$SCRIPT"

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

  # 6. Log-freshness issues go to warnings, not failures
  run_test "log-freshness in warnings not failures" python3 -c "
import json
d = json.load(open('$JSON_FILE'))
for f in d.get('details', []):
    assert 'log-freshness' not in f, f'log-freshness in failures: {f}'
"

  # 7. Status is not degraded when only warnings exist
  run_test "warnings-only does not degrade status" python3 -c "
import json
d = json.load(open('$JSON_FILE'))
real_failures = [f for f in d.get('details', []) if 'log-freshness' not in f]
if len(real_failures) == 0:
    assert d['status'] != 'degraded', 'status is degraded with no real failures'
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
