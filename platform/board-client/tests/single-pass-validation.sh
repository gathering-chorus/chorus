#!/usr/bin/env bash
# #2032 — Test: cards add reports ALL missing fields in one pass
# RED: currently reports one field at a time
# GREEN: after fix, reports all missing fields together

set -uo pipefail

CARDS="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards"
PASS=0
FAIL=0

assert_contains() {
  local desc="$1" output="$2" expected="$3"
  if echo "$output" | grep -qi "$expected"; then
    echo "  ✓ $desc"
    ((PASS++))
  else
    echo "  ✗ $desc — expected '$expected' in output"
    ((FAIL++))
  fi
}

assert_not_contains() {
  local desc="$1" output="$2" unexpected="$3"
  if echo "$output" | grep -qi "$unexpected"; then
    echo "  ✗ $desc — found unexpected '$unexpected'"
    ((FAIL++))
  else
    echo "  ✓ $desc"
    ((PASS++))
  fi
}

echo "=== #2032: Single-pass validation ==="

# Test 1: Missing domain + type → both reported in one error
echo "Test 1: multiple missing fields reported together"
output=$(bash "$CARDS" add "test-multifield" --owner silas --priority P2 -q 2>&1 || true)
assert_contains "reports domain missing" "$output" "domain"
assert_contains "reports type missing" "$output" "type"

# Test 2: Type inference from title verb
echo "Test 2: type inference from title"
output=$(bash "$CARDS" add "fix broken nudge" --owner silas --priority P2 --domain chorus -q 2>&1 || true)
# Should auto-infer type:fix from "fix" in title, not error on missing type
assert_contains "infers type from title" "$output" "Auto-tagged type:fix"
# Clean up if it created
card_id=$(echo "$output" | grep -oE '#[0-9]+' | head -1 | tr -d '#')
[ -n "$card_id" ] && bash "$CARDS" move "$card_id" "won't do" 2>/dev/null || true

# Test 3: --quick still works (no desc gate)
echo "Test 3: --quick bypasses desc gate"
output=$(bash "$CARDS" add "test quick card" --owner silas --priority P2 --domain chorus --type chore --sequence ops -q 2>&1 || true)
assert_not_contains "no desc error with --quick" "$output" "ERROR.*desc"
card_id=$(echo "$output" | grep -oE '#[0-9]+' | head -1 | tr -d '#')
[ -n "$card_id" ] && bash "$CARDS" move "$card_id" "won't do" 2>/dev/null || true

# Test 4: Well-formed call → zero errors
echo "Test 4: well-formed call has no errors"
output=$(bash "$CARDS" add "test well-formed" --owner silas --priority P2 --domain chorus --type chore --sequence ops -q 2>&1 || true)
assert_not_contains "no ERROR in output" "$output" "^ERROR"
card_id=$(echo "$output" | grep -oE '#[0-9]+' | head -1 | tr -d '#')
[ -n "$card_id" ] && bash "$CARDS" move "$card_id" "won't do" 2>/dev/null || true

echo ""
echo "Results: $PASS pass, $FAIL fail"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
