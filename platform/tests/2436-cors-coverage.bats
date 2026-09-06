#!/usr/bin/env bats
# @test-type: fitness:security
#
# #2436 — the CORS coverage probe must separate the states it exists to separate.
#
# The probe's whole job is to tell four situations apart. A check that collapses
# any two of them would have stayed green through the defect this card fixes, so
# each is asserted against a fixture rather than assumed from the code.

setup() {
  PROBE="${BATS_TEST_DIRNAME}/../scripts/cors-coverage.sh"
}

@test "scoring: no Allow-Origin for a stranger is the only SECURED state" {
  run bash "$PROBE" score ""
  [ "$status" -eq 0 ]
  [ "$output" = "SECURED" ]
}

@test "NEGATIVE PROOF: a wildcard is not secured — the exact state that shipped since April" {
  run bash "$PROBE" score '*'
  [ "$status" -eq 0 ]
  [ "$output" = "WILDCARD" ]
}

@test "NEGATIVE PROOF: echoing a stranger's origin back is not secured either" {
  run bash "$PROBE" score 'http://evil.example' 'http://evil.example'
  [ "$status" -eq 0 ]
  [ "$output" = "REFLECTED" ]
}

@test "a FIXED other origin is a refusal, not a reflection — the browser compares" {
  # #2431's block always answers http://localhost:3000 whatever you ask from. A
  # scorer without the asked-origin called that REFLECTED and failed three
  # correctly-configured routes. The browser refuses a value that is not its own
  # origin, so this is secured.
  run bash "$PROBE" score 'http://localhost:3000' 'http://localhost.localtest.me:9999'
  [ "$status" -eq 0 ]
  [ "$output" = "SECURED" ]
}

@test "a dead route scores MISPROBE and can never read as secured" {
  # The #3837/#3958 lesson the sibling probes learned: an unreachable route must
  # not score as if it passed. Absence of a header from a route that never
  # answered is not the same fact as a route that deliberately withheld one.
  run bash "$PROBE" score MISPROBE
  [ "$status" -eq 0 ]
  [ "$output" = "MISPROBE" ]
  [ "$output" != "SECURED" ]
}

@test "the companion question: our own pages must still be granted" {
  run bash "$PROBE" score-local 'http://localhost:3000'
  [ "$status" -eq 0 ]
  [ "$output" = "GRANTED" ]
}

@test "NEGATIVE PROOF: refusing EVERYONE is a failure, not a perfect score" {
  # Without this leg, a surface that answered nobody would score 100% on the
  # stranger test while every demo page broke. That is the hollow-gate shape.
  run bash "$PROBE" score-local ""
  [ "$status" -eq 0 ]
  [ "$output" = "DENIED" ]
}

# The two live legs below are BOX-DEPENDENT: their answer is a fact about
# whichever chorus-api the caller points at, not about this code. Run against a
# host that has not deployed the fix they are correctly red, which would deadlock
# a land on its own gate. So they refuse to score unless a system-under-test is
# named explicitly, and say so rather than passing quietly (Jeff, 2026-09-02: a
# box-dependent test reports UNMEASURED, never a green).
#
# The real live proof for this card is the demo, where prod and the variant are
# probed side by side and disagree.

@test "live (opt-in): the named system refuses a stranger and serves our own pages" {
  if [ -z "${CORS_SUT:-}" ]; then
    skip "UNMEASURED — no CORS_SUT named; set CORS_SUT=http://host:port to score a running system"
  fi
  run env CHORUS_API_URL="$CORS_SUT" bash "$PROBE"
  echo "$output"
  [ "$status" -eq 0 ]
}
