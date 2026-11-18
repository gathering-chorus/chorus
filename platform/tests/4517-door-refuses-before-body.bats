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
