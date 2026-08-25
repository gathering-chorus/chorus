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

# ── no retry: one suite, one execution, whatever the verdict ──
# #4004 — this used to stub run_one_attempt and count its calls. #3974 retired
# that walker: run_one now routes every kind through run_cargo_lane (the one
# werk-test runner), so the stub was never called and the count was 0 — the
# test measured a function that no longer exists while the INVARIANT (no
# retries) held fine. Assert the invariant against today's structure instead:
# the retry machinery is gone from the runner, and one call means one lane run.
NS="${NS:-$(dirname "$0")/nightly-suites.sh}"
_needs_stack() { return 1; }              # not stack-gated
CNT="$TMP/attempts"; : > "$CNT"           # count via a file — run_one runs in a subshell
run_cargo_lane() { echo x >> "$CNT"; echo "SUITE|npm|${2:-unit}|${3:-kade}|fail|0 pass, 1 fail"; }
line=$(run_one npm "$TMP/fakepkg" kade)
n=$(wc -l < "$CNT" | tr -d ' ')
[ "$n" -eq 1 ] && ok || bad "run_one must invoke the runner ONCE (no retry), got $n"
# NEGATIVE PROOF (#3734): a retry loop would be caught — a stub that is called
# twice fails this same assertion, so the check can still separate its states.
grep -qE 'for +_?(attempt|try|retry)|while .*retry' "$NS" \
  && bad "a retry loop reappeared in the runner" || ok

echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
