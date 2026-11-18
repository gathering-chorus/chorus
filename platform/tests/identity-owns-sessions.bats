#!/usr/bin/env bats
# @test-type: unit — the session decision crate, the door's login/logout routes and the DAL's
# owner rule, each through its own test target; no live services
# @domain: security
# @card: #4517
#
# #4517 AC5 — the door is the only writer of Session rows: it creates the row when it accepts a
# verified login, records a refused login (401/403 with the reason) itself, and the DAL refuses
# a Session owned by anyone but the login writing it. Negative proofs live in each target:
# a role's direct Session write is refused (door_written), a foreign actsAs or wrong account is
# 403, a login with no id is 401, another principal's Session is refused at the DAL.
#
# Covers: platform/services/identity-login/src/lib.rs
# Covers: platform/services/athena-make/src/logins.rs
# Covers: platform/services/athena-model/src/lib.rs

setup() { ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"; }

cargo_ok() { # $1 crate dir, rest: cargo test args — passes when it ran tests and all passed
  local dir="$1"; shift
  run bash -c "cd '$ROOT/platform/services/$dir' && cargo test --release --quiet $* 2>&1"
  echo "$output" | tail -3
  [ "$status" -eq 0 ] || return 1
  echo "$output" | grep -qE "test result: ok\. [1-9][0-9]* passed" || return 1
}

@test "AC5 the session decision: one row per verified login, the door's stamps, refusals named" {
  cargo_ok identity-login
}

@test "AC5 the door's login and logout routes, and a role's direct Session write refused" {
  cargo_ok athena-make --test logins_4517
  cargo_ok athena-make --test session_door_written_4517
}

@test "AC5 the DAL: a Session is owned by the login that writes it, never another" {
  cargo_ok athena-model --lib session_owner_4517
}
