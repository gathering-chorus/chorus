#!/usr/bin/env bash
# test-quality-health.sh — #2862 verification.
#
# Smoke test for the quality-health fitness function. Asserts:
#   - Script runs without crashing (exits 0 or 1, not 2/error).
#   - Default output contains expected section headers.
#   - --json mode produces valid JSON with expected top-level keys.
#   - --verbose flag is recognized.
#
# Same shape as spine-health/commit-health smoke testing — verify the
# fitness function's contract, not its specific computed numbers (those
# vary with live system state).

set -uo pipefail

# Self-locate: quality-health sits beside this test in platform/scripts/.
# (Was hardcoded to the retired persistent-werk path chorus-werk/kade — #2913.)
SCRIPT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/quality-health}"
PASS=0
FAIL=0

trap '_rc=$?; echo "=== Results: $PASS passed, $FAIL failed ==="' EXIT

# AC1: script exists + executable
if [ -x "$SCRIPT" ]; then
  echo "PASS [exists+executable]"
  PASS=$((PASS+1))
else
  echo "FAIL [exists+executable]: $SCRIPT not present or not executable"
  FAIL=$((FAIL+1))
  exit 1
fi

# AC2: default invocation runs cleanly (rc 0 or 1; never 2/127/segfault)
out=$(bash "$SCRIPT" 2>&1)
rc=$?
if [ "$rc" -eq 0 ] || [ "$rc" -eq 1 ]; then
  echo "PASS [default invocation rc=$rc (0 or 1 acceptable)]"
  PASS=$((PASS+1))
else
  echo "FAIL [default invocation rc=$rc — should be 0 or 1, got something unexpected]"
  echo "  output: $out"
  FAIL=$((FAIL+1))
fi

# AC3: output contains expected section headers
expected_sections=("data window" "suites" "stale-skip" "gate-skip rate")
for s in "${expected_sections[@]}"; do
  if echo "$out" | grep -qiE "$s"; then
    echo "PASS [section: $s]"
    PASS=$((PASS+1))
  else
    echo "FAIL [section missing: $s]"
    FAIL=$((FAIL+1))
  fi
done

# AC4: --json mode produces valid JSON with expected top-level keys
json_out=$(bash "$SCRIPT" --json 2>&1)
if echo "$json_out" | python3 -c '
import json,sys
try:
  d = json.loads(sys.stdin.read())
  required = ["suites", "stale_skip", "gate_skip_rate", "window_days"]
  missing = [k for k in required if k not in d]
  if missing:
    print(f"missing keys: {missing}")
    sys.exit(1)
  print("OK")
except json.JSONDecodeError as e:
  print(f"invalid JSON: {e}")
  sys.exit(1)
' >/dev/null 2>&1; then
  echo "PASS [--json valid + has expected keys]"
  PASS=$((PASS+1))
else
  echo "FAIL [--json invalid or missing keys]"
  echo "  output: $(echo "$json_out" | head -3)"
  FAIL=$((FAIL+1))
fi

# AC5: --verbose recognized (doesn't error, may produce more output)
verbose_out=$(bash "$SCRIPT" --verbose 2>&1)
verbose_rc=$?
if [ "$verbose_rc" -eq 0 ] || [ "$verbose_rc" -eq 1 ]; then
  echo "PASS [--verbose recognized rc=$verbose_rc]"
  PASS=$((PASS+1))
else
  echo "FAIL [--verbose rc=$verbose_rc unexpected]"
  FAIL=$((FAIL+1))
fi

if [ "$FAIL" -gt 0 ]; then exit 1; fi
exit 0
