#!/usr/bin/env bats
# @test-type: unit — a stand-in dagu records its arguments; no live scheduler
# @domain: cicd
# @card: 4474 · owner: kade
# What Jeff sees: a v2 run keeps going after the session that started it exits
# (#4474 run 10 died with the shell that ran `dagu start`).

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  export CHORUS_HOME="$BATS_TEST_TMPDIR/home" CHORUS_WERK_BASE="$BATS_TEST_TMPDIR/werks"
  mkdir -p "$CHORUS_HOME" "$CHORUS_WERK_BASE/kade-4474/platform/pipelines"
  touch "$CHORUS_WERK_BASE/kade-4474/platform/pipelines/cicd.yaml"
  export DAGU="$BATS_TEST_TMPDIR/dagu"
  printf '#!/bin/sh\necho "$@" > "%s/args"\n' "$BATS_TEST_TMPDIR" > "$DAGU"; chmod +x "$DAGU"
}

@test "a v2 run is handed to the scheduler, never run from this shell" {
  run "$ROOT/platform/scripts/werk-v2" 4474 kade
  [ "$status" -eq 0 ]
  args="$(cat "$BATS_TEST_TMPDIR/args")"
  [[ "$args" == *" enqueue $CHORUS_WERK_BASE/kade-4474/platform/pipelines/cicd.yaml -- CARD=4474 ROLE=kade" ]] || return 1
  [[ "$args" != *" start "* ]] || return 1
}

@test "negative proof: no card, a bad role or no workflow is refused before dagu" {
  run "$ROOT/platform/scripts/werk-v2" "" kade
  [ "$status" -ne 0 ]
  run "$ROOT/platform/scripts/werk-v2" 4474 'Kade;rm'
  [ "$status" -ne 0 ]
  run "$ROOT/platform/scripts/werk-v2" 9999 kade
  [ "$status" -ne 0 ]
  [[ "$output" == *"no workflow"* ]] || return 1
  [ ! -f "$BATS_TEST_TMPDIR/args" ]
}
