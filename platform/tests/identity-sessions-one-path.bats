#!/usr/bin/env bats
# @test-type: unit — source guard plus the login tool's and browser sign-in's own suites; no live services
# @domain: security
# @card: #4517
#
# #4517 AC6 — one path: chorus-principal and person_session.py write no Session row (login,
# renewal, logout, supersede and the expired close all go through the door), the sweep closes
# no Session, and the door-only rule is on in the model while no role-side write remains.
#
# Covers: platform/services/chorus-principal/src/lib.rs
# Covers: platform/scripts/lib/person_session.py
# Covers: roles/silas/ontology/session-model-4302.ttl

setup() { ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"; }

@test "AC6 no role-side Session write remains while the shape says door (guard, with its negative proof)" {
  run bats "$ROOT/platform/tests/4517-session-door-flip-guard.bats"
  echo "$output" | tail -4
  [ "$status" -eq 0 ] || return 1
  grep -qE 'writeAuthority[[:space:]]+"door"' "$ROOT/roles/silas/ontology/session-model-4302.ttl" || return 1
}

@test "AC6 the sweep writes no Session row, even with an older pane session open" {
  run bash -c "cd '$ROOT/platform/services/chorus-principal' && cargo test --release --quiet --test sweep_closes_superseded_4950 2>&1"
  echo "$output" | tail -3
  [ "$status" -eq 0 ] || return 1
  echo "$output" | grep -qE "test result: ok\. [1-9][0-9]* passed" || return 1
}

@test "AC6 his browser sign-in, renewal and sign-out ask the door and never write the row" {
  run python3 "$ROOT/platform/tests/fixtures/person-session-4412.py"
  [ "$status" -eq 0 ] || { echo "$output" | grep FAIL; return 1; }
  grep -qF "PASS NEGATIVE: it never writes a Session row itself" <<<"$output" || return 1
  run python3 "$ROOT/platform/tests/fixtures/share-renew-4500.py"
  [ "$status" -eq 0 ] || { echo "$output" | grep FAIL; return 1; }
  grep -qF "PASS NEGATIVE: renewal never writes the Session row itself" <<<"$output" || return 1
}
