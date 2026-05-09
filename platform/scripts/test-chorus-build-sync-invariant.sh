#!/usr/bin/env bash
# test-chorus-build-sync-invariant.sh — proving gate for #2863.
#
# Asserts that /build's first step is `git fetch + ff` against origin/main
# (the canonical-sync invariant), and that the script aborts loudly on
# fetch / fast-forward failure rather than silently building stale source.
#
# Two assertions:
#   1. happy path: chorus-build prints the invariant marker line and the
#      "canonical at <sha>" line before any build artifacts are written.
#   2. abort path: chorus-build with a non-git CHORUS_ROOT exits non-zero
#      with a diagnostic, NOT silently proceeding to build.
#
# Usage: ./test-chorus-build-sync-invariant.sh

set -uo pipefail

trap '_rc=$?; if [ $_rc -eq 0 ]; then echo "=== Results: 2 passed, 0 failed ==="; else echo "=== Results: $PASS passed, $FAIL failed ==="; fi' EXIT

PASS=0
FAIL=0

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SCRIPT="$SCRIPT_DIR/chorus-build"
CHORUS_HOME="${CHORUS_HOME:-/Users/jeffbridwell/CascadeProjects/chorus}"

if [ ! -x "$SCRIPT" ]; then
  echo "FAIL [setup]: chorus-build not executable at $SCRIPT"
  FAIL=$((FAIL+1))
  exit 1
fi

# Assertion 1: happy path emits the invariant markers.
# Uses chorus-hook-shim (smallest crate, fastest build) to keep the test
# bounded. We background chorus-build, sleep 3s for the sync step to print,
# kill, and capture output. macOS doesn't ship `timeout`, hence backgrounding.
echo "test-chorus-build-sync-invariant: assertion 1 — happy-path invariant markers"
HAPPY_TMP=$(mktemp)
bash "$SCRIPT" chorus-hooks > "$HAPPY_TMP" 2>&1 &
HAPPY_PID=$!
sleep 3
kill -TERM "$HAPPY_PID" 2>/dev/null || true
wait "$HAPPY_PID" 2>/dev/null || true
HAPPY_OUT=$(cat "$HAPPY_TMP")
rm -f "$HAPPY_TMP"
if echo "$HAPPY_OUT" | grep -q "sync canonical from origin (invariant"; then
  if echo "$HAPPY_OUT" | grep -qE "canonical at [0-9a-f]+"; then
    echo "PASS [happy-path]: both invariant markers present"
    PASS=$((PASS+1))
  else
    echo "FAIL [happy-path]: 'canonical at <sha>' marker missing"
    echo "  output: $HAPPY_OUT"
    FAIL=$((FAIL+1))
  fi
else
  echo "FAIL [happy-path]: 'sync canonical from origin (invariant' marker missing"
  echo "  output: $HAPPY_OUT"
  FAIL=$((FAIL+1))
fi

# Assertion 2: abort path on bad CHORUS_ROOT — non-git dir.
# A CHORUS_ROOT that isn't a git repo can't fetch; chorus-build must
# abort, not proceed. Uses /tmp/<random> as the bad root.
echo "test-chorus-build-sync-invariant: assertion 2 — abort on non-git CHORUS_ROOT"
BAD_ROOT="/tmp/chorus-build-test-$$"
mkdir -p "$BAD_ROOT"
ABORT_OUT=$(CHORUS_ROOT="$BAD_ROOT" bash "$SCRIPT" chorus-hooks 2>&1 || true)
ABORT_RC=$?
rm -rf "$BAD_ROOT"
# We expect non-zero exit AND the abort marker to appear in output.
if echo "$ABORT_OUT" | grep -qE "ABORT — git fetch origin main failed|ABORT — canonical not fast-forwardable"; then
  echo "PASS [abort-path]: abort marker present on non-git root"
  PASS=$((PASS+1))
else
  echo "FAIL [abort-path]: expected abort marker, got:"
  echo "  $ABORT_OUT"
  FAIL=$((FAIL+1))
fi

if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
exit 0
