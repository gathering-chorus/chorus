#!/usr/bin/env bats
# @test-type: unit — temp werk fixtures; never reaches a relay or a pulse.
# @domain: messages
# @card: #4629
# @owner: wren
# platform/scripts/demo-4629-deliver-from-buzz.sh — the AC2 demo refuses to prove
# anything it cannot honestly prove: no werk-built shim (the installed one would
# pass vacuously via the fallback, Silas 06:51), no staging room in the config,
# or a room that is the real one.

SCRIPT="$BATS_TEST_DIRNAME/../scripts/demo-4629-deliver-from-buzz.sh"

werk() {
  local w="$BATS_TEST_TMPDIR/werk"
  mkdir -p "$w/directing/clearing-v2" "$w/platform/services/chorus-hooks/target/release" "$w/.chorus-demo"
  printf '%s\n' "$1" > "$w/directing/clearing-v2/clearing-v2.config.json"
  if [ "${2:-shim}" = shim ]; then
    printf '#!/bin/sh\necho "nudge sent: wren → silas (trace=00000000-0000-0000-0000-000000000000) · Buzz: queued"\n' > "$w/platform/services/chorus-hooks/target/release/chorus-hook-shim"
    chmod +x "$w/platform/services/chorus-hooks/target/release/chorus-hook-shim"
  fi
  echo "$w"
}

@test "NEGATIVE PROOF: no werk-built shim, no demo" {
  w=$(werk '{"buzzRelayHost":"relay.lan:3000","stagingRelayPort":3010,"stagingTeamChannel":"c"}' none)
  run bash "$SCRIPT" "$w" http://127.0.0.1:9 http://127.0.0.1:9 http://127.0.0.1:9
  [ "$status" -ne 0 ]
  [[ "$output" == *"no werk-built chorus-hook-shim"* ]] || return 1
}

@test "NEGATIVE PROOF: a config with no staging room is refused" {
  w=$(werk '{"buzzRelayHost":"relay.lan:3000"}')
  run bash "$SCRIPT" "$w" http://127.0.0.1:9 http://127.0.0.1:9 http://127.0.0.1:9
  [ "$status" -ne 0 ]
  [[ "$output" == *"no staging room"* ]] || return 1
}

@test "NEGATIVE PROOF: the real room (the prod tunnel port) is refused" {
  w=$(werk '{"buzzRelayHost":"127.0.0.1:13000","stagingRelayPort":13000,"stagingTeamChannel":"c"}')
  run bash "$SCRIPT" "$w" http://127.0.0.1:9 http://127.0.0.1:9 http://127.0.0.1:9
  [ "$status" -ne 0 ]
  [[ "$output" == *"refusing the real room"* ]] || return 1
}

@test "NEGATIVE PROOF: the real room direct on its own port (:3000) is refused too" {
  w=$(werk '{"buzzRelayHost":"relay.lan:3000","stagingRelayPort":3000,"stagingTeamChannel":"c"}')
  run bash "$SCRIPT" "$w" http://127.0.0.1:9 http://127.0.0.1:9 http://127.0.0.1:9
  [ "$status" -ne 0 ] || return 1
  [[ "$output" == *"refusing the real room"* ]] || return 1
}

@test "NEGATIVE PROOF: a note that did not reach the staging room fails AC2 (never passes via the fallback)" {
  w=$(werk '{"buzzRelayHost":"relay.lan:3000","stagingRelayPort":3010,"stagingTeamChannel":"c"}')
  run bash "$SCRIPT" "$w" http://127.0.0.1:9 http://127.0.0.1:9 http://127.0.0.1:9
  [ "$status" -ne 0 ]
  [[ "$output" == *"did not reach the staging room"* ]] || return 1
}
