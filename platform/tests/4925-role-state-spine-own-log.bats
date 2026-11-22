#!/usr/bin/env bats
# @test-type: unit — runs test-role-state-spine.sh against a stubbed role-state and a busy log
# @domain: roles
# @card: #4925 · owner: kade
# Wren 11-22: test-role-state-spine.sh used the caller's CHORUS_LOG_FILE (the nightly sets one)
# and counted every new line in it, so any suite writing that log in the same seconds failed it.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
SPEC="$ROOT/platform/scripts/test-role-state-spine.sh"

@test "a busy shared log no longer fails it: it measures only its own spine" {
  busy="$BATS_TEST_TMPDIR/shared.log"; : > "$busy"
  ( for i in $(seq 1 200); do echo '{"event":"other.suite"}' >> "$busy"; sleep 0.01; done ) &
  run env CHORUS_LOG_FILE="$busy" CHORUS_ROOT="$ROOT" bash "$SPEC"
  wait
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "NEGATIVE PROOF: a role-state that writes a line on 'building' still fails Test 2" {
  stub="$BATS_TEST_TMPDIR/role-state"
  cat > "$stub" <<'SH'
#!/bin/bash
[ "$2" = blocked ] && [[ "$3" == card=* ]] && exit 2
if [ "$2" = blocked ]; then printf '{"event":"role.blocked","role":"%s","detail":"waiting on the DAL cred"}\n' "$1" >> "$CHORUS_LOG_FILE"; exit 0; fi
echo '{"event":"role.state.changed"}' >> "$CHORUS_LOG_FILE"
SH
  chmod +x "$stub"
  run env ROLE_STATE_BIN="$stub" CHORUS_ROOT="$ROOT" bash "$SPEC"
  [ "$status" -ne 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"Test 2"*"FAIL"* ]] || { echo "$output"; return 1; }
}
