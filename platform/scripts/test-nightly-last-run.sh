#!/usr/bin/env bash
# test-nightly-last-run.sh — #3606: ONE runner, one run. daily-review must READ
# the 03:00 nightly's results, never re-run the suites (#3272 — the 6 AM
# launchd env has no cargo, so its re-run manufactured a 30-suite false-red
# wall on 2026-07-04 and clobbered the real failure logs).
#
# Contract under test: `nightly-suites.sh --last-run`
#   - emits exactly the LATEST run's SUITE lines from the log (not earlier runs)
#   - exits 0 when the latest block exists and is fresh
#   - a missing log emits a loud SUITE fail line + rc 1 (no silent empty)
#   - a stale log (>26h) emits a loud staleness fail line + rc 1 (no third state)
#
# Hermetic: fixture log via NIGHTLY_LOG_PATH; nothing executes any suite.

set -u

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
NIGHTLY="$SCRIPT_DIR/nightly-suites.sh"

PASS=0; FAIL=0
p() { PASS=$((PASS+1)); echo "  ok: $1"; }
f() { FAIL=$((FAIL+1)); echo "  FAIL: $1"; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

LOG="$TMP/nightly-suites.log"
# Two appended runs; suites A+B appear in both (the repeat boundary), suite C only in run 2.
cat > "$LOG" <<'EOF'
SUITE|npm|/x/pkg-a|kade|fail|Tests: 1 failed, 9 passed, 10 total
SUITE|shell|/x/test-b.sh|silas|pass|3 pass, 0 fail
SUITE|npm|/x/pkg-a|kade|pass|Tests: 10 passed, 10 total
SUITE|shell|/x/test-b.sh|silas|pass|3 pass, 0 fail
SUITE|cargo|/x/crate-c|silas|pass|suites: 5 ok, 0 failed
EOF

echo "--- latest block only ---"
OUT=$(NIGHTLY_LOG_PATH="$LOG" bash "$NIGHTLY" --last-run 2>&1); RC=$?
[ "$RC" -eq 0 ] && p "exits 0 on a fresh log" || f "expected rc 0, got $RC: $OUT"
COUNT=$(echo "$OUT" | grep -c '^SUITE|')
[ "$COUNT" -eq 3 ] && p "emits exactly the 3 latest-run lines" || f "expected 3 SUITE lines, got $COUNT: $OUT"
echo "$OUT" | grep -q '|cargo|/x/crate-c|' && p "includes the run-2-only suite" || f "missing crate-c line"
if echo "$OUT" | grep -q 'pkg-a|kade|fail'; then f "leaked run-1's stale fail for pkg-a"; else p "run-1's stale result not leaked"; fi

echo "--- missing log fails loud ---"
OUT=$(NIGHTLY_LOG_PATH="$TMP/absent.log" bash "$NIGHTLY" --last-run 2>&1); RC=$?
[ "$RC" -ne 0 ] && p "nonzero rc on missing log" || f "missing log must not read as success"
echo "$OUT" | grep -qi 'SUITE|.*fail' && p "emits a parseable fail line" || f "no loud fail line: $OUT"

echo "--- stale log fails loud ---"
touch -t 202601010000 "$LOG"
OUT=$(NIGHTLY_LOG_PATH="$LOG" bash "$NIGHTLY" --last-run 2>&1); RC=$?
[ "$RC" -ne 0 ] && p "nonzero rc on stale log" || f "stale log must not read as success"
echo "$OUT" | grep -qi 'stale' && p "names staleness" || f "staleness not named: $OUT"

echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
