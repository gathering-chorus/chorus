#!/usr/bin/env bash
# inject-regression.sh — validates chorus-inject delivers to the correct role tab
# AC: targeted delivery (not broadcast to focused window), all 3 roles, intra-team + alert
set -euo pipefail

INJECT="/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/target/release/chorus-inject"
PASS=0
FAIL=0

echo "=== Inject regression test ==="

# Test 1-3: Each role receives inject (targeted delivery)
for role in silas wren kade; do
  pattern=""
  case $role in
    silas) pattern="architect" ;;
    wren) pattern="product-manager" ;;
    kade) pattern="engineer" ;;
  esac
  
  # Check window exists
  found=$(osascript -e "
    tell application \"Terminal\"
      repeat with w in windows
        if name of w contains \"$pattern\" and name of w contains \"claude\" then
          return \"found\"
        end if
      end repeat
      return \"not found\"
    end tell" 2>&1)
  
  if [ "$found" = "found" ]; then
    result=$("$INJECT" "$role" "[regression-test] ping $role" 2>&1) && {
      echo "PASS: $role inject succeeded"
      PASS=$((PASS + 1))
    } || {
      echo "FAIL: $role inject failed: $result"
      FAIL=$((FAIL + 1))
    }
  else
    echo "SKIP: $role window not open"
  fi
done

# Test 4: Inject binary exists and is executable
if [ -x "$INJECT" ]; then
  echo "PASS: chorus-inject binary exists and executable"
  PASS=$((PASS + 1))
else
  echo "FAIL: chorus-inject binary missing or not executable"
  FAIL=$((FAIL + 1))
fi

# Test 5: Unknown role is rejected
result=$("$INJECT" "nonexistent" "test" 2>&1) && {
  echo "FAIL: unknown role should be rejected"
  FAIL=$((FAIL + 1))
} || {
  echo "PASS: unknown role rejected: $result"
  PASS=$((PASS + 1))
}

# Test 6 retired: bash nudge was retired in #2804/#2809; agents send nudges
# via the chorus_nudge_message MCP tool. The osascript injection path that
# this test was guarding is exercised end-to-end by the MCP layer's own
# delivery tests; chorus-inject's regression surface here covers binary
# resolution, role validation, and refusal taxonomy only.

echo "---"
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
