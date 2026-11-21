#!/usr/bin/env bats
# @test-type: unit — drives person_session's record expiry with stub I/O (the 4412 fixture); no network, no store
# @domain: identity
# @card: #5021 (folded from card 5026, AC "close-at-sign-in works again") · owner: kade
# Covers: platform/scripts/lib/person_session.py, platform/tests/fixtures/person-session-4412.py
#
# The guard's record keeps the row's expiresAt (a renewal moves it), and a record whose
# sign-in ran out is pruned, by expiresAt or, for a name-only record from before this card,
# by its login's own exp. So the next sign-in of each principal closes its expired open rows.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  FX="$ROOT/platform/tests/fixtures/person-session-4412.py"
}

CASES=(
  "the record keeps the row's expiresAt, the login's own expiry"
  "NEGATIVE (#5026): a record past its expiry is pruned, one before it is kept"
  "a name-only record from before #5026 is pruned by its login's own exp, never kept forever"
  "a renewal moves the record's expiresAt with the row's"
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
person_session.record_ended = lambda rec, now: False
runpy.run_path('$FX', run_name='__main__')"
  [ "$status" -ne 0 ]
  printf '%s\n' "$output" | grep -qF "FAIL NEGATIVE (#5026): a record past its expiry is pruned" || { echo "$output"; return 1; }
  printf '%s\n' "$output" | grep -qF "FAIL a name-only record from before #5026 is pruned" || { echo "$output"; return 1; }
}
