#!/usr/bin/env bash
# @test-type: unit — #3597 nightly determinism. Hermetic: sources nightly-suites.sh
# and drives acquire_single_flight_lock + run_one with a private temp lockdir and
# stubbed internals. No real suites, no network, no scheduled run.
set -u
# Test THIS werk's script (the one under change), not canonical — derive from $0.
SCRIPT="$(cd "$(dirname "$0")" && pwd)/nightly-suites.sh"
PASS=0; FAIL=0
ok()  { PASS=$((PASS+1)); }
bad() { FAIL=$((FAIL+1)); echo "FAIL: $*"; }

TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
export NIGHTLY_LOCKDIR="$TMP/lock.d"

# shellcheck source=/dev/null
source "$SCRIPT"

# ── single-flight lock ──────────────────────────────────────────────
acquire_single_flight_lock && ok || bad "first acquire should succeed"

# a second acquire while the first holds it (live pid) must be refused
( acquire_single_flight_lock ) && bad "second acquire (lock held) must FAIL" || ok

release_single_flight_lock
acquire_single_flight_lock && ok || bad "acquire after release should succeed"
release_single_flight_lock

# stale lock (holder pid dead) is stolen, not wedged
mkdir -p "$NIGHTLY_LOCKDIR"; echo 999999 > "$NIGHTLY_LOCKDIR/pid"
acquire_single_flight_lock && ok || bad "stale lock (dead pid 999999) must be stolen"
release_single_flight_lock

# NOTE: the CLI-level "--run-all exits 0 when locked" behavior is asserted via the
# function (acquire returns 1 → dispatch echoes + exit 0). We do NOT invoke the real
# `--run-all` here — that would execute every suite. The lock contract is fully
# covered by the acquire/steal function tests above.

# ── retry removed: run_one calls run_one_attempt exactly once, even on failure ──
_needs_stack() { return 1; }              # not stack-gated
CNT="$TMP/attempts"; : > "$CNT"           # count via a file — run_one runs in a subshell
run_one_attempt() { echo x >> "$CNT"; echo "SUITE|npm|$2|$3|fail|0 pass, 1 fail"; }
line=$(run_one npm "$TMP/fakepkg" kade)
n=$(wc -l < "$CNT" | tr -d ' ')
[ "$n" -eq 1 ] && ok || bad "run_one must call attempt ONCE (retry removed), got $n"
echo "$line" | grep -q '|fail|' && ok || bad "run_one should pass through the single-attempt fail verdict"

echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
