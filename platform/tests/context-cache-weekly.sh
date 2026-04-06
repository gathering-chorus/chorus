#!/usr/bin/env bash
# Test: context-cache-weekly subcommand exists and produces output
# AC: subcommand runs, cruft scan writes, stale cards audited, disk trend logged, non-empty stdout
set -euo pipefail

SHIM="/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/target/release/chorus-hook-shim"
PASS=0
FAIL=0

echo "=== context-cache-weekly test ==="

# Test 1: subcommand exists and runs
result=$("$SHIM" context-cache-weekly silas 2>&1) && {
  if [ -n "$result" ]; then
    echo "PASS: subcommand runs and produces output"
    PASS=$((PASS + 1))
  else
    echo "FAIL: subcommand runs but produces no output"
    FAIL=$((FAIL + 1))
  fi
} || {
  echo "FAIL: subcommand failed: $result"
  FAIL=$((FAIL + 1))
}

# Test 2: cruft scan written
if [ -f /tmp/cruft-scan-latest.md ] && [ -s /tmp/cruft-scan-latest.md ]; then
  echo "PASS: cruft scan file exists and non-empty"
  PASS=$((PASS + 1))
else
  echo "FAIL: cruft scan file missing or empty"
  FAIL=$((FAIL + 1))
fi

# Test 3: disk trend logged
TREND="/Users/jeffbridwell/Library/Logs/Chorus/disk-trend.log"
if [ -f "$TREND" ] && [ -s "$TREND" ]; then
  echo "PASS: disk trend log exists"
  PASS=$((PASS + 1))
else
  echo "FAIL: disk trend log missing"
  FAIL=$((FAIL + 1))
fi

# Test 4: output mentions stale cards
if echo "$result" | grep -qi "stale\|card"; then
  echo "PASS: output includes card audit"
  PASS=$((PASS + 1))
else
  echo "FAIL: output missing card audit"
  FAIL=$((FAIL + 1))
fi

# Test 5: rejects unknown role
"$SHIM" context-cache-weekly badrole 2>&1 && {
  echo "FAIL: should reject unknown role"
  FAIL=$((FAIL + 1))
} || {
  echo "PASS: unknown role rejected"
  PASS=$((PASS + 1))
}

echo "---"
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
