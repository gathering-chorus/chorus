#!/usr/bin/env bats
# @test-type: unit — drives person_session's record expiry with stub I/O (the 4412 fixture); no network, no store
# @domain: identity
# @card: #5021 (folded from card 5026, AC "close-at-sign-in works again") · owner: kade
# Covers: platform/scripts/lib/person_session.py, platform/tests/fixtures/person-session-4412.py
# Covers: platform/scripts/chorus-share-guard.py, platform/tests/fixtures/browser-signin-5015.py
#
# The guard's record keeps the COOKIE's expiry (a renewal moves it), and a record whose cookie
# is dead is pruned: by expiresAt or, for a name-only record from before this card, once its
# login's exp plus the idle window has passed. Never at the token's hour: the guard reads a
# pruned record as signed out. So each principal's next sign-in closes its expired open rows.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  FX="$ROOT/platform/tests/fixtures/person-session-4412.py"
}

CASES=(
  "the record keeps the cookie's expiry (sign-in + the idle window), never the token's hour"
  "NEGATIVE (#5026): a live cookie's record outlives the token's hour, and is pruned when the cookie dies"
  "a name-only record from before #5026 is pruned once its login's exp plus the idle window has passed, not before"
  "a record with no expiry at all, or no idle window given, is kept: never guess an end"
  "a renewal moves the record's expiresAt to the renewed cookie's exp"
  "a renewal given no cookie exp leaves the record's expiresAt as it was"
)

@test "the record keeps expiresAt, renewals move it, and ended records are pruned (both kinds)" {
  run python3 "$FX"
  for c in "${CASES[@]}"; do
    printf '%s\n' "$output" | grep -qxF "PASS $c" || { echo "missing: PASS $c"; echo "$output"; return 1; }
  done
}

# NEGATIVE PROOF: with record_ended as #4517 left it (nothing ever ends), the prune cases go red
@test "NEGATIVE: a guard that never prunes a record fails the close-at-sign-in cases" {
  run python3 -c "
import os, runpy, sys
sys.path.insert(0, os.path.join('$ROOT', 'platform/scripts/lib'))
import person_session
person_session.record_ended = lambda rec, now, lasts=0: False
runpy.run_path('$FX', run_name='__main__')"
  [ "$status" -ne 0 ]
  printf '%s\n' "$output" | grep -qF "FAIL NEGATIVE (#5026): a live cookie's record outlives the token's hour, and is pruned" || { echo "$output"; return 1; }
  printf '%s\n' "$output" | grep -qF "FAIL a name-only record from before #5026 is pruned" || { echo "$output"; return 1; }
}

# NEGATIVE PROOF (code review 11-21): record_ended as first written ended a record at the
# token's hour; the name-only "not before" case goes red on it
@test "NEGATIVE: a record_ended that ends at the token's hour fails the fixture" {
  run python3 -c "
import os, runpy, sys
sys.path.insert(0, os.path.join('$ROOT', 'platform/scripts/lib'))
import person_session
person_session.record_ended = lambda rec, now, lasts=0: int(rec.get('exp', 0) or 0) <= int(now)
runpy.run_path('$FX', run_name='__main__')"
  [ "$status" -ne 0 ]
  printf '%s\n' "$output" | grep -qF "FAIL a name-only record from before #5026 is pruned once" || { echo "$output"; return 1; }
}

# Through the guard's REAL sign-in (the 5015 driver): the guard reads a missing record as signed
# out, so a second sign-in must never drop a record whose cookie is still alive.
step() { # <n> <key> -> that key of the walk's n-th JSON line (the guard's own log lines are skipped)
  printf '%s\n' "$output" | grep '^{' | sed -n "${1}p" | python3 -c 'import json,sys; print(json.load(sys.stdin)[sys.argv[1]])' "$2"
}

walk() { # <tree root> -> one JSON line per printed step
  local fx="$1/platform/tests/fixtures/browser-signin-5015.py" st="$BATS_TEST_TMPDIR/st-$RANDOM"
  python3 "$fx" "$st" signin phone   >/dev/null || return 1
  python3 "$fx" "$st" idle 7200      >/dev/null || return 1   # two hours: past the token's hour
  python3 "$fx" "$st" signin desktop || return 1
  python3 "$fx" "$st" visit phone    || return 1
  python3 "$fx" "$st" idle 46800     >/dev/null || return 1   # thirteen idle hours: both cookies dead
  python3 "$fx" "$st" signin laptop  || return 1
}

@test "a later sign-in leaves a live session signed in, and drops the records of dead cookies" {
  run walk "$ROOT"
  echo "$output"
  [ "$status" -eq 0 ]
  [ "$(step 1 sessions)" = 2 ] || return 1
  [ "$(step 2 state)" = ok ] || return 1
  [ "$(step 3 sessions)" = 1 ] || return 1
}

# NEGATIVE PROOF: the reviewed bug, restored in a copy (the record ends at the token's hour):
# the phone is signed out by the desktop's sign-in
@test "NEGATIVE: with the record ending at the token's hour, a second sign-in signs the first out" {
  T="$BATS_TEST_TMPDIR/tree"; mkdir -p "$T/platform/scripts" "$T/platform/tests"
  cp -R "$ROOT/platform/scripts/lib" "$T/platform/scripts/lib"
  cp "$ROOT/platform/scripts/chorus-share-guard.py" "$T/platform/scripts/"
  cp -R "$ROOT/platform/tests/fixtures" "$T/platform/tests/fixtures"
  python3 - "$T/platform/scripts/lib/person_session.py" <<'PY'
import sys
p = sys.argv[1]; s = open(p).read()
good = '"expiresAt": iso(int(now) + int(lasts))'
assert s.count(good) == 1, "the line this negative restores the bug on has moved"
open(p, "w").write(s.replace(good, '"expiresAt": iso(exp)'))
PY
  run walk "$T"
  echo "$output"
  [ "$status" -eq 0 ]
  [ "$(step 2 state)" = logged-out ] || return 1
}
