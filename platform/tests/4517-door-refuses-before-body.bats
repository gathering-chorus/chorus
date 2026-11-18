#!/usr/bin/env bats
# @test-type: unit — the model door's write order, through athena-make's hermetic serve
# (Fuseki stub + stub DAL + real serve loop); no live services
# @domain: security
# @card: #4517
#
# #4517 AC1 — a caller the door would refuse gets 403 whatever the body. Before this card a
# non-owner PUT with a door stamp (writeCount) got 422 from the stamp check, ahead of the
# row-owner verdict, so a refused caller learned the body's rules.
#
# Covers: platform/services/athena-make/src/lib.rs

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
}

@test "AC1 NEGATIVE PROOF: a non-owner's stamped body is 403, the owner's same body is 422" {
  run bash -c "cd '$ROOT/platform/services/athena-make' && cargo test --release --quiet --test coverage_3701_hermetic a_refused_caller_gets_403_before_the_body_checks_4517 2>&1"
  echo "$output" | tail -3
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE "test result: ok\. 1 passed"
}

@test "AC1 create: a caller with no Write row gets 403 before any body check (the same body is 422 with one)" {
  run bash -c "cd '$ROOT/platform/services/athena-make' && cargo test --release --quiet --test create_needs_write_row_4517 a_refused_caller_gets_403_before_any_body_check 2>&1"
  echo "$output" | tail -3
  [ "$status" -eq 0 ] || return 1
  echo "$output" | grep -qE "test result: ok\. 1 passed" || return 1
}

@test "AC1 ownerless rows: a claim from a caller with no Write is 403 before its body is judged" {
  run bash -c "cd '$ROOT/platform/services/athena-make' && cargo test --release --quiet --test coverage_3701_hermetic a_claim_on_an_ownerless_row_reaches_the_dal_and_nothing_else_does_4517 2>&1"
  echo "$output" | tail -3
  [ "$status" -eq 0 ] || return 1
  echo "$output" | grep -qE "test result: ok\. 1 passed" || return 1
}
