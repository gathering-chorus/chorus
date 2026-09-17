#!/usr/bin/env bash
# test-crawler-alerts-live.sh — each crawler alert detects its target condition.
# @test-type: unit — hermetic: the checks read fixture logs handed through the
# CRAWL_LOG / CHORUS_ROOT seams; no store, no nudge (only the check: block runs).
#
# #2817 wrote this for the retired index-crawler loop; #4197 re-pointed both
# alerts at chorus-crawl (#4173) and this test with them. Every alert is proven
# BOTH ways (#3734): a fixture that makes it fire, and one that keeps it quiet.

set -uo pipefail

PASS=0
FAIL=0
p() { PASS=$((PASS+1)); echo "  PASS: $*"; }
f() { FAIL=$((FAIL+1)); echo "  FAIL: $*"; }

CHORUS_ROOT_REAL="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
ALERT_DIR="$CHORUS_ROOT_REAL/proving/domains/alerts"
FX=$(mktemp -d -t crawler-alerts.XXXX)
trap 'rm -rf "$FX"' EXIT

extract_check() {
  awk '
    /^check: \|/ { in_check=1; next }
    in_check && /^[a-zA-Z_]+:/ { in_check=0 }
    in_check { sub(/^  /,""); print }
  ' "$1"
}
# run_check <yaml> <fixture log> <fixture root> -> prints rc
run_check() {
  local check; check=$(extract_check "$1")
  CRAWL_LOG="$2" CHORUS_ROOT="$3" bash -c "$check" >/dev/null 2>&1
  echo $?
}

CLEAN='chorus-crawl: full (no watermark on the graph — first run) · tracked=6214 read=Complete
chorus-crawl: posted=0 replaced=0 unchanged=5572 deleted=0 skipped=642
chorus-crawl: cases posted=0 replaced=0 unchanged=8222 deleted=0 · test files parsed=1049 declared=494 inferred=546 no-case=9
chorus-crawl: wrote=0 failed=0
chorus-crawl: watermark -> 8f8ade821'

echo "=== crawler alert live-fire receipts (#2817 → #4197) ==="

# ── crawler-stale ──
mkdir -p "$FX/root"; echo 8f8ade821 > "$FX/root/.chorus-crawl-watermark"
printf '%s\n' "$CLEAN" > "$FX/fresh.log"
RC=$(run_check "$ALERT_DIR/crawler-stale.yml" "$FX/fresh.log" "$FX/root")
[ "$RC" = "0" ] && p "crawler-stale: quiet on a fresh clean pass with a watermark" || f "crawler-stale: expected rc=0 on a fresh clean pass, got $RC"

printf '%s\n' "$CLEAN" | sed 's/watermark -> 8f8ade821/watermark HELD — a write failed — the graph does not match this commit/' > "$FX/held.log"
RC=$(run_check "$ALERT_DIR/crawler-stale.yml" "$FX/held.log" "$FX/root")
[ "$RC" = "1" ] && p "crawler-stale: FIRES when the last pass held its watermark" || f "crawler-stale: expected rc=1 on a held watermark, got $RC"

printf '%s\n' "$CLEAN" > "$FX/old.log"; touch -t 202601010000 "$FX/old.log"
RC=$(run_check "$ALERT_DIR/crawler-stale.yml" "$FX/old.log" "$FX/root")
[ "$RC" = "1" ] && p "crawler-stale: FIRES when the log has not been written in 26h" || f "crawler-stale: expected rc=1 on an old log, got $RC"

RC=$(run_check "$ALERT_DIR/crawler-stale.yml" "$FX/does-not-exist.log" "$FX/root")
[ "$RC" = "1" ] && p "crawler-stale: FIRES when there is no crawl log at all" || f "crawler-stale: expected rc=1 with no log, got $RC"

# ── crawler-error ──
RC=$(run_check "$ALERT_DIR/crawler-error.yml" "$FX/fresh.log" "$FX/root")
[ "$RC" = "0" ] && p "crawler-error: quiet on failed=0" || f "crawler-error: expected rc=0 on failed=0, got $RC"

printf '%s\n' "$CLEAN" | sed 's/wrote=0 failed=0/wrote=6180 failed=447 · elapsed=6550s rate=1.0\/s mints=12/' > "$FX/failed.log"
RC=$(run_check "$ALERT_DIR/crawler-error.yml" "$FX/failed.log" "$FX/root")
[ "$RC" = "1" ] && p "crawler-error: FIRES on failed=447 (the 2026-09-16 20:24 line)" || f "crawler-error: expected rc=1 on failed>0, got $RC"

{ echo 'chorus-crawl: MASS DELETE REFUSED — the plan would delete 7883 of 7883 case rows. A full pass never removes half the registry; this tree is not the tree the graph describes (wrong CHORUS_ROOT?). No case row is deleted this run.'; printf '%s\n' "$CLEAN"; } > "$FX/mass.log"
# the refusal is printed BEFORE the header line; a real log carries it inside the pass too
printf '%s\n' "$CLEAN" | sed '2i\
chorus-crawl: MASS DELETE REFUSED — the plan would delete 5572 of 5572 file rows.' > "$FX/mass.log"
RC=$(run_check "$ALERT_DIR/crawler-error.yml" "$FX/mass.log" "$FX/root")
[ "$RC" = "1" ] && p "crawler-error: FIRES on a refused mass delete" || f "crawler-error: expected rc=1 on MASS DELETE REFUSED, got $RC"

# an OLD red pass followed by a clean one is quiet: only the LAST pass counts
{ printf '%s\n' "$CLEAN" | sed 's/wrote=0 failed=0/wrote=0 failed=3/'; printf '%s\n' "$CLEAN"; } > "$FX/recovered.log"
RC=$(run_check "$ALERT_DIR/crawler-error.yml" "$FX/recovered.log" "$FX/root")
[ "$RC" = "0" ] && p "crawler-error: quiet when a red pass was followed by a clean one (last pass decides)" || f "crawler-error: expected rc=0 after recovery, got $RC"

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
