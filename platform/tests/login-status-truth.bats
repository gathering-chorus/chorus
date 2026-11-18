#!/usr/bin/env bats
# @test-type: unit — chorus-principal's status-line rules and the Teammates tile, each through
# its own test target; no live services
# @domain: identity
# @card: #4997
#
# #4997 AC1 — "logged in" only with the role's own successful login AND an open Session row;
# a refused login reads refused with the reason. The Teammates tile says logged in only with
# an open, live row. 11-09: every role read green while their logins were being refused.
#
# Covers: platform/services/chorus-principal/src/lifecycle.rs
# Covers: directing/clearing-v2/src/views.ts

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
}

@test "AC1 the status line never says logged in without an open Session row, and says refused with why" {
  run bash -c "cd '$ROOT/platform/services/chorus-principal' && cargo test --release --quiet --test status_truth_4997 2>&1"
  echo "$output" | tail -3
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE "test result: ok\. 9 passed"
}

@test "AC1 the Teammates tile: open live row reads logged in, else not logged in, unread reads unverified" {
  run bash -c "cd '$ROOT/directing/clearing-v2' && npx jest tests/teammates-login-4997.test.ts 2>&1"
  echo "$output" | tail -3
  [ "$status" -eq 0 ]
  echo "$output" | grep -qE "Tests: +4 passed, 4 total"
}
