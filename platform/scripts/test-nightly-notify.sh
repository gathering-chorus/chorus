#!/bin/bash
# @test-type: unit — stubbed ops-nudge capture, hermetic
# #3254 — nightly-suites must ALERT the owning role on completion (the call-to-action).
# notify_results() parses the run's SUITE|kind|path|owner|status|summary lines, groups the
# failures by owner, and fires ONE ops-nudge per owning role with their red suites. Green →
# a single confirmation to the nightly owner so "green" is also a signal. This is what closes
# Jeff's loop: job done → role alerted immediately → role acts, no Jeff in the middle.
#
# Hermetic: sources nightly-suites.sh for the function, stubs ops-nudge via $OPS_NUDGE so the
# test captures the calls instead of POSTing to pulse.

set -u
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
# #3922 — source the SIBLING copy, never $CHORUS_ROOT: a werk run must test
# the werk's code, not canonical's (this line silently validated old code).
SCRIPT="$(cd "$(dirname "$0")" && pwd)/nightly-suites.sh"
PASS=0; FAIL=0
p() { PASS=$((PASS+1)); echo "✅ $*"; }
f() { FAIL=$((FAIL+1)); echo "❌ $*"; }

STUB=$(mktemp -d -t nn.XXXXXX)
CAP="$STUB/calls"
cat > "$STUB/ops-nudge" <<EOF
#!/bin/bash
# capture: "<to>|<content>" per line
echo "\$1|\$2" >> "$CAP"
EOF
chmod +x "$STUB/ops-nudge"
export OPS_NUDGE="$STUB/ops-nudge"

# shellcheck disable=SC1090
source "$SCRIPT"

if ! declare -F notify_results >/dev/null; then
  f "notify_results is not defined in nightly-suites.sh"
  echo "=== Results: $PASS passed, $((FAIL+1)) failed ==="; exit 1
fi

# --- mixed: kade has 1 red, silas has 2 reds, plus greens ---
: > "$CAP"
RESULTS='SUITE|npm|/x/directing/products/cards|kade|fail|1 failed
SUITE|bats|/x/platform/tests/pulse-bar.bats|silas|fail|6 failed
SUITE|bats|/x/platform/tests/session-health.bats|silas|fail|8 failed
SUITE|cargo|/x/platform/services/chorus-hooks|silas|pass|ok
SUITE|npm|/x/platform/pulse|kade|pass|ok'
notify_results "$RESULTS"

grep -q '^kade|' "$CAP"  && p "kade alerted on his red"            || f "kade NOT alerted; cap: $(cat "$CAP")"
grep -q '^silas|' "$CAP" && p "silas alerted on his reds"          || f "silas NOT alerted; cap: $(cat "$CAP")"
grep '^kade|'  "$CAP" | grep -q 'cards'        && p "kade's nudge names his red suite"   || f "kade nudge missing suite name: $(grep '^kade|' "$CAP")"
grep '^silas|' "$CAP" | grep -qE 'pulse-bar|session-health' && p "silas's nudge names his red suites" || f "silas nudge missing suites: $(grep '^silas|' "$CAP")"
# exactly one nudge per owner (not one per suite)
# #3904 — since #3606 kade (nightly owner) gets TWO nudges by design: his own
# slice + the board-level TOTAL (the 2026-08-04 5x-undercount fix). 2 is correct.
[ "$(grep -c '^kade|'  "$CAP")" = "2" ] && p "kade alerted twice (slice + board total, #3606)" || f "kade alerted $(grep -c '^kade|' "$CAP")× (expected 2: slice + total)"
grep -q '^kade|nightly TOTAL:' "$CAP" && p "board-level TOTAL nudge present" || f "no board-total nudge — the #3606 aggregate is missing"
[ "$(grep -c '^silas|' "$CAP")" = "1" ] && p "silas alerted once (grouped)" || f "silas alerted $(grep -c '^silas|' "$CAP")× (expected 1)"
# passing suites' owners are NOT spuriously alerted beyond their real reds
grep '^kade|' "$CAP" | grep -q 'pulse$' && f "kade nudge wrongly lists a passing suite" || p "passing suites not listed as red"

# --- all green: single confirmation to the nightly owner (kade), no red alerts ---
: > "$CAP"
GREEN='SUITE|cargo|/x/a|silas|pass|ok
SUITE|npm|/x/b|kade|pass|ok'
notify_results "$GREEN"
[ "$(wc -l < "$CAP" | tr -d ' ')" = "1" ] && p "all-green → exactly one confirmation nudge" || f "all-green nudges: $(cat "$CAP")"
grep -qi 'green' "$CAP" && p "all-green nudge says green" || f "all-green nudge content: $(cat "$CAP")"

# --- #3922: a red SECURITY-lane suite routes to the security owner as its own signal ---
: > "$CAP"
SEC='SUITE|security|platform/scripts/authn-coverage.sh|silas|fail|0 pass, 1 fail (coverage DROPPED)
SUITE|npm|/x/b|kade|pass|ok'
notify_results "$SEC"
grep '^silas|' "$CAP" | grep -q 'SECURITY lane: 1 red' && p "security red routes to the security owner, named lane" || f "no security-lane nudge: $(cat "$CAP")"
grep '^silas|SECURITY' "$CAP" | grep -q 'authn-coverage.sh' && p "security nudge names the failing probe" || f "security nudge missing suite: $(cat "$CAP")"
# NEGATIVE PROOF (#3734): a PASSING security suite fires NO security-lane nudge.
: > "$CAP"
SECOK='SUITE|security|platform/scripts/authn-coverage.sh|silas|pass|1 pass, 0 fail'
notify_results "$SECOK"
grep -q 'SECURITY lane' "$CAP" && f "security-lane nudge fired on green: $(cat "$CAP")" || p "green security lane stays quiet"

rm -rf "$STUB"
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
