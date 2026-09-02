#!/usr/bin/env bash
# test-nightly-refused-skip-4065.sh — a suite that DECLINES to run (rc=3, #4004)
# folds to a SKIP row: never red, never green, and never remapped to UNMEASURED.
#
# Before #4065 the runner scored test-product-membrane.sh's self-refusal as
# "pass | 0 pass, 1 fail"; the wrapper's contradiction rule (#3753) then flipped
# it to fail. Every night. This drives the wrapper's real fold functions (sourced,
# not copied — #4013) with the runner's new line and with the old one.
set -u
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NIGHTLY="$SCRIPT_DIR/nightly-suites.sh"
[ -f "$NIGHTLY" ] || { echo "FAIL: cannot find $NIGHTLY"; exit 1; }
# shellcheck disable=SC1090
source "$NIGHTLY"

PASS=0; FAIL=0
ok()   { echo "  PASS: $1"; PASS=$((PASS+1)); }
bad()  { echo "  FAIL: $1"; FAIL=$((FAIL+1)); }

fold() {
  # the two steps every nightly-unit line goes through before it becomes a SUITE row
  local verdict="$1" summary="$2" v r
  v=$(_classify_verdict "$verdict" "$summary" 2>/dev/null)
  r=$(_remap_unmeasured "$v" "$summary")
  printf '%s' "$r"
}

echo "=== #4065 self-refused suite folds to skip ==="

echo "Test 1: the runner's refused line stays a skip row with its reason"
got=$(fold skip "0 pass, 0 fail (SELF-REFUSED rc=3 — suite declined to run here)")
case "$got" in
  "skip|0 pass, 0 fail (SELF-REFUSED rc=3"*) ok "skip row kept: $got" ;;
  *) bad "expected skip row, got: $got" ;;
esac

echo "Test 2: a skip row is counted as skipped by the summary, not as red"
rows="SUITE|shell|platform/scripts/test-product-membrane.sh|silas|skip|0 pass, 0 fail (SELF-REFUSED rc=3 — suite declined to run here)
SUITE|bats|platform/tests/x.bats|silas|pass|3 pass, 0 fail"
skipped=$(printf '%s\n' "$rows" | awk -F'|' '$1=="SUITE" && $5=="skip"' | grep -c .)
reds=$(printf '%s\n' "$rows" | awk -F'|' '$1=="SUITE" && $5=="fail"' | grep -c .)
if [ "$skipped" = "1" ] && [ "$reds" = "0" ]; then ok "1 skipped, 0 red"; else bad "skipped=$skipped reds=$reds"; fi

echo "Test 3: NEGATIVE PROOF — the OLD runner line (pass with failed=1) is still flipped to fail"
got=$(fold pass "0 pass, 1 fail")
case "$got" in
  "fail|"*) ok "contradiction still caught: $got" ;;
  *) bad "old shape no longer flips to fail: $got" ;;
esac

echo "Test 4: NEGATIVE PROOF — a bare '0 pass, 0 fail' with no reason is still UNMEASURED, not a skip"
got=$(fold pass "0 pass, 0 fail")
case "$got" in
  "unmeasured|"*) ok "bare 0/0 stays unmeasured: $got" ;;
  *) bad "bare 0/0 should be unmeasured, got: $got" ;;
esac

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
exit "$FAIL"
