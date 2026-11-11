#!/usr/bin/env bats
# @test-type: integration — runs the suites that hold #4838's AC (a scratch Fuseki for the store proof)
# @domain: identity
# @card: #4838 · owner: silas
# What Jeff sees: Clearing tiles show every role that's running and none that
# isn't. The roles door asks the store for live sessions only (no endedAt,
# expiresAt not past), so it no longer pulls 2,600 rows against a 3s abort.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

@test "AC1/AC4: the store returns only live sessions; dropping either half lets ended or expired ones in (real Jena)" {
  cd "$ROOT/platform/services/athena-make"
  run env CARGO_BUILD_JOBS=2 nice cargo test --test live_sessions_4838
  echo "$output" | grep -E 'test result|panicked' || true
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"1 passed"* ]] || return 1
}

@test "AC1: the door's absent/after filters and their refusals (unit)" {
  cd "$ROOT/platform/services/athena-make"
  run env CARGO_BUILD_JOBS=2 nice cargo test --lib -- absent_and_after a_bad_operator_or_value_is_refused
  [ "$status" -eq 0 ] || { echo "$output" | tail -20; return 1; }
  [[ "$output" == *"2 passed"* ]] || return 1
}

@test "AC3: the roles door asks for live sessions; a role with no live session has no tile" {
  cd "$ROOT/platform/api"
  run npx jest tests/handlers/context-roles.test.ts
  [ "$status" -eq 0 ] || { echo "$output" | tail -20; return 1; }
}

# #4950 — AC5's two sweep cases ("closes the expired one" and its NEGATIVE PROOF) are
# deleted: #4948 removed expiry as a close (a session ends only at logout). The sweep's
# loud failure on a refused close is proven in sweep_closes_superseded_4950.rs.
