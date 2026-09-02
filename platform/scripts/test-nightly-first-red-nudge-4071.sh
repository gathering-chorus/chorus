#!/usr/bin/env bash
# test-nightly-first-red-nudge-4071.sh — a red row nudges its owner the moment
# it lands, during the run. Jeff, 2026-09-02: "once there is one error in the
# daily test run why do we wait to the end?"
#
# Drives the REAL function (sourced, #4013) with OPS_NUDGE pointed at a stub
# that records every call. Positive: a fail row -> one nudge to the owner naming
# the unit. Negative (#3734): pass / skip / unmeasured rows -> no nudge at all.
set -u
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NIGHTLY="$SCRIPT_DIR/nightly-suites.sh"
[ -f "$NIGHTLY" ] || { echo "FAIL: cannot find $NIGHTLY"; exit 1; }
# shellcheck disable=SC1090
source "$NIGHTLY"

PASS=0; FAIL=0
ok()  { echo "  PASS: $1"; PASS=$((PASS+1)); }
bad() { echo "  FAIL: $1"; FAIL=$((FAIL+1)); }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
STUB="$TMP/ops-nudge"; LOG="$TMP/nudges.log"
printf '#!/usr/bin/env bash\nprintf "%%s|%%s\\n" "$1" "$2" >> "%s"\n' "$LOG" > "$STUB"; chmod +x "$STUB"
export OPS_NUDGE="$STUB"

echo "=== #4071 first-red nudge ==="

echo "Test 1: a fail row nudges the unit's owner, naming the unit, during the run"
: > "$LOG"
_first_red_nudge bats platform/tests/session-health.bats fail "7 pass, 2 fail"
if [ "$(grep -c . "$LOG")" = "1" ] && grep -q 'platform/tests/session-health.bats' "$LOG" && grep -q 'RED now' "$LOG"; then
  ok "one nudge: $(cat "$LOG" | cut -c1-90)"
else
  bad "expected exactly one nudge naming the unit, got: $(cat "$LOG")"
fi

echo "Test 2: the nudge goes to the owner the runner already knows (owner_for)"
want=$(owner_for platform/tests/session-health.bats)
if grep -q "^${want}|" "$LOG"; then ok "owner=$want"; else bad "expected owner $want, got: $(cut -d'|' -f1 "$LOG")"; fi

echo "Test 3: NEGATIVE PROOF — pass, skip and unmeasured rows never nudge"
: > "$LOG"
_first_red_nudge bats platform/tests/x.bats pass "9 pass, 0 fail"
_first_red_nudge shell platform/scripts/test-product-membrane.sh skip "0 pass, 0 fail (SELF-REFUSED rc=3 — suite declined to run here)"
_first_red_nudge npm platform/api unmeasured "0 pass, 0 fail (UNMEASURED — suite produced no parseable output)"
if [ ! -s "$LOG" ]; then ok "no nudges for non-red rows"; else bad "nudged on a non-red row: $(cat "$LOG")"; fi

echo "Test 4: a cargo unit resolves to its services path for ownership"
: > "$LOG"
_first_red_nudge cargo chorus-hooks fail "539 pass, 1 fail"
if grep -q 'platform/services/chorus-hooks' "$LOG"; then ok "cargo path resolved"; else bad "cargo unit path not resolved: $(cat "$LOG")"; fi

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
exit "$FAIL"
