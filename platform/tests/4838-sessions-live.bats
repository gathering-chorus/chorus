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

# AC5 — the sweep, run: it reads only open rows at the door, closes an expired
# one, and exits 0; NEGATIVE PROOF: a door that refuses the close makes it exit
# non-zero with failed>0, never a quiet 0.
sweep_world() {
  cd "$ROOT/platform/services/chorus-principal"
  CARGO_BUILD_JOBS=2 nice cargo build -q 2>&1 | tail -5
  PRINCIPAL="$ROOT/platform/services/chorus-principal/target/debug/chorus-principal"
  [ -x "$PRINCIPAL" ] || { echo "chorus-principal did not build"; return 1; }
  T="$BATS_TEST_TMPDIR"
  source "$ROOT/platform/tests/lib/login-harness.bash"
  login_harness
  printf '{"data":[{"name":"session-kade-dead","actsAs":"","sessionState":"open","expiresAt":"2026-09-01T00:10:00Z","tokenId":"j","ownedBy":"principal-kade"}]}' > "$T/row.json"
}

@test "AC5: sweep asks the door for open rows only, closes the expired one, and exits 0" {
  sweep_world || return 1
  run "$PRINCIPAL" sweep
  echo "$output"
  [ "$status" -eq 0 ] || return 1
  printf '%s' "$output" | grep -qF "1 expired session(s) closed" || return 1
  grep -F "/v1/identity/sessions?" "$T/curl.log" | grep -qF "endedAt.absent=true" || { cat "$T/curl.log"; return 1; }
}

@test "AC5 NEGATIVE PROOF: a door that refuses the close makes sweep fail loudly" {
  sweep_world || return 1
  echo 500 > "$T/curl.status"
  run "$PRINCIPAL" sweep
  echo "$output"
  [ "$status" -ne 0 ] || return 1
  printf '%s' "$output" | grep -qE "[1-9][0-9]* failed" || return 1
}
