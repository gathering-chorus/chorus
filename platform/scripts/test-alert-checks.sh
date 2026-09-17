#!/usr/bin/env bash
# test-alert-checks.sh — #2861 verification, re-pointed by #4197.
# @test-type: unit — hermetic: fixture logs through the CRAWL_LOG / CHORUS_ROOT
# seams; no store, no nudge.
#
# The alert-runner's EXACT extraction + execution contract, applied to the two
# crawler alerts (crawler-error, crawler-stale): the healthy path must return the
# literal "ok" (anything else fires), and the broken path must NOT.
#
# alert-runner contract (proving/scripts/alert-runner.sh:39-41):
#   check_script=$(awk '/^check: \|/{found=1; next} /^[a-z]/{if(found) exit} found{print}' "$rule_file")
#   result=$(bash -c "$check_script" 2>&1) || true
#   if [[ "$result" == "ok" ]]; then OK
#
# #2861 also smoke-tested three python siblings and hydration-divergence.yml;
# #3380 tombstoned all three crawler alerts when the index-crawler loop was
# retired; #4197 re-pointed crawler-stale and crawler-error at the crawler that
# runs now (chorus-crawl, #4173) and retired the siblings and the divergence
# alert with the loop they measured. test-crawler-alerts-live.sh carries the
# fire/quiet receipts per condition; this file pins the runner contract.

set -euo pipefail

# #2856 canonical contract: emit "=== Results: N passed, M failed ===" on EXIT
trap 'echo "=== Results: $PASS passed, $FAIL failed ==="' EXIT

PASS=0
FAIL=0

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHORUS_ROOT="${CHORUS_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
ALERT_DIR="$CHORUS_ROOT/proving/domains/alerts"
FX=$(mktemp -d)
trap_orig=$(trap -p EXIT)
trap 'rm -rf "$FX"; eval "$trap_orig"' EXIT

# the runner's extraction, verbatim
extract() { awk '/^check: \|/{found=1; next} /^[a-z]/{if(found) exit} found{print}' "$1"; }
# run <yaml> <log> <root> -> prints the check's stdout, like the runner reads it
run_as_runner() { CRAWL_LOG="$2" CHORUS_ROOT="$3" bash -c "$(extract "$1")" 2>&1 || true; }

mkdir -p "$FX/root"; echo 8f8ade821 > "$FX/root/.chorus-crawl-watermark"
printf '%s\n' 'chorus-crawl: full (no watermark on the graph — first run) · tracked=6214 read=Complete' \
  'chorus-crawl: posted=0 replaced=0 unchanged=5572 deleted=0 skipped=642' \
  'chorus-crawl: cases posted=0 replaced=0 unchanged=8222 deleted=0 · test files parsed=1049 declared=494 inferred=546 no-case=9' \
  'chorus-crawl: wrote=0 failed=0' 'chorus-crawl: watermark -> 8f8ade821' > "$FX/healthy.log"
sed 's/wrote=0 failed=0/wrote=6180 failed=447/; s/watermark -> 8f8ade821/watermark HELD — a write failed/' "$FX/healthy.log" > "$FX/broken.log"

check() { # <label> <expected: ok|fire> <actual>
  if [ "$2" = ok ] && [ "$3" = "ok" ]; then PASS=$((PASS+1)); echo "PASS [$1]: returned 'ok'"
  elif [ "$2" = fire ] && [ "$3" != "ok" ]; then PASS=$((PASS+1)); echo "PASS [$1]: fired with '$3'"
  else FAIL=$((FAIL+1)); echo "FAIL [$1]: expected $2, returned '$3'"; fi
}

for y in crawler-error crawler-stale; do
  [ -f "$ALERT_DIR/$y.yml" ] || { FAIL=$((FAIL+1)); echo "FAIL [$y.yml exists]: missing"; continue; }
  check "$y healthy → literal ok" ok   "$(run_as_runner "$ALERT_DIR/$y.yml" "$FX/healthy.log" "$FX/root")"
  check "$y broken pass → fires"  fire "$(run_as_runner "$ALERT_DIR/$y.yml" "$FX/broken.log"  "$FX/root")"
done

# the runner's own awk must stop at the next top-level key: a check that leaked
# its action: block would nudge on every tick. NEGATIVE PROOF: the extraction
# of either yml contains no "OPS_NUDGE".
for y in crawler-error crawler-stale; do
  if extract "$ALERT_DIR/$y.yml" | grep -q OPS_NUDGE; then FAIL=$((FAIL+1)); echo "FAIL [$y extraction stops before action:]: action leaked into check"
  else PASS=$((PASS+1)); echo "PASS [$y extraction stops before action:]"; fi
done

[ "$FAIL" -eq 0 ]
