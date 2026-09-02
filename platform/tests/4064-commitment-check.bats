#!/usr/bin/env bats
# @test-type: unit — runs commitment-check.sh over TTL fixtures written to BATS_TEST_TMPDIR; no store, no service, no network.
# #4064 AC3: open Commitment with no card = FAIL; closed Commitment whose probe fails = FAIL.
# Both negative proofs are fixtures where the rule is VIOLATED and the check is shown RED (#3734).

setup() {
  CHECK="$BATS_TEST_DIRNAME/../scripts/commitment-check.sh"
  export COMMITMENT_PROBE_ROOT="$BATS_TEST_TMPDIR"
  printf '#!/bin/bash\nexit 0\n' > "$BATS_TEST_TMPDIR/green.sh"; chmod +x "$BATS_TEST_TMPDIR/green.sh"
  printf '#!/bin/bash\nexit 1\n' > "$BATS_TEST_TMPDIR/red.sh";   chmod +x "$BATS_TEST_TMPDIR/red.sh"
  PRE='@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:card-1 a chorus:Card .'
}

row() { # id status [card] [probe]
  local s="chorus:commitment-$1 a chorus:Commitment ; chorus:status \"$2\""
  [ -n "${3:-}" ] && s="$s ; chorus:card chorus:card-$3"
  [ -n "${4:-}" ] && s="$s ; chorus:probe \"$4\""
  echo "$s ."
}

@test "green world: carded open, deferred, closed with a green probe -> exit 0" {
  { echo "$PRE"; row a open 1; row b deferred; row c closed 1 green.sh; } > "$BATS_TEST_TMPDIR/w.ttl"
  run env COMMITMENT_TTL="$BATS_TEST_TMPDIR/w.ttl" "$CHECK"
  [ "$status" -eq 0 ] || { echo "$output"; false; }
  [[ "$output" == *"commitments=3 fail=0"* ]] || { echo "$output"; false; }
}

@test "NEGATIVE PROOF: an open commitment with no card turns the check RED" {
  { echo "$PRE"; row a open 1; row orphan open; } > "$BATS_TEST_TMPDIR/w.ttl"
  run env COMMITMENT_TTL="$BATS_TEST_TMPDIR/w.ttl" "$CHECK"
  [ "$status" -eq 1 ] || { echo "$output"; false; }
  [[ "$output" == *"FAIL open-no-card commitment-orphan"* ]] || { echo "$output"; false; }
}

@test "NEGATIVE PROOF: a closed commitment whose probe exits non-zero turns the check RED" {
  { echo "$PRE"; row c closed 1 red.sh; } > "$BATS_TEST_TMPDIR/w.ttl"
  run env COMMITMENT_TTL="$BATS_TEST_TMPDIR/w.ttl" "$CHECK"
  [ "$status" -eq 1 ] || { echo "$output"; false; }
  [[ "$output" == *"FAIL closed-probe-red commitment-c"* ]] || { echo "$output"; false; }
}

@test "a closed commitment with no probe is UNMEASURED, never a pass line" {
  { echo "$PRE"; row c closed 1; } > "$BATS_TEST_TMPDIR/w.ttl"
  run env COMMITMENT_TTL="$BATS_TEST_TMPDIR/w.ttl" "$CHECK"
  [ "$status" -eq 0 ] || { echo "$output"; false; }
  [[ "$output" == *"UNMEASURED closed-no-probe commitment-c"* ]] || { echo "$output"; false; }
  [[ "$output" != *"ok   closed"* ]] || { echo "$output"; false; }
}

@test "a file with zero commitment rows is a FAIL, not a vacuous pass (#3734 absence guard)" {
  echo "$PRE" > "$BATS_TEST_TMPDIR/w.ttl"
  run env COMMITMENT_TTL="$BATS_TEST_TMPDIR/w.ttl" "$CHECK"
  [ "$status" -eq 1 ] || { echo "$output"; false; }
  [[ "$output" == *"FAIL no-commitments"* ]] || { echo "$output"; false; }
}
