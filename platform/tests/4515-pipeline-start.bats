#!/usr/bin/env bats
# @test-type: unit — a stand-in dagu records its arguments; no live scheduler
# @domain: pipelines
# @card: 4515 · owner: wren
# What Jeff sees (2026-10-14): `chorus-<product> [stream]` starts that
# product's pipeline for a card. chorus-werk, chorus-athena and chorus-borg
# all run pipeline-start; a pipeline with no generated workflow is refused by
# name and nothing is enqueued, so nothing pretends to run.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  export CHORUS_HOME="$BATS_TEST_TMPDIR/home" CHORUS_WERK_BASE="$BATS_TEST_TMPDIR/werks"
  W="$CHORUS_WERK_BASE/wren-4515"
  mkdir -p "$CHORUS_HOME" "$W/platform/pipelines"
  for p in cicd athena borg-prove; do touch "$W/platform/pipelines/$p.yaml"; done
  # #4579 — cicd reads the card's diff (model changes start as athena), so the werk is a repo
  git -C "$W" init -q && git -C "$W" config user.email t@t && git -C "$W" config user.name t && git -C "$W" add -A && git -C "$W" commit -qm base && git -C "$W" update-ref refs/remotes/origin/main HEAD
  export DAGU="$BATS_TEST_TMPDIR/dagu"
  printf '#!/bin/sh\necho "$@" > "%s/args"\n' "$BATS_TEST_TMPDIR" > "$DAGU"; chmod +x "$DAGU"
}

@test "each product's pipeline is enqueued from the card's werk, by workflow name" {
  for p in cicd athena borg-prove; do
    rm -f "$BATS_TEST_TMPDIR/args"
    run "$ROOT/platform/scripts/pipeline-start" "$p" 4515 wren
    [ "$status" -eq 0 ] || { echo "$p: $output"; return 1; }
    args="$(cat "$BATS_TEST_TMPDIR/args")"
    [[ "$args" == *" enqueue $W/platform/pipelines/$p.yaml -- CARD=4515 ROLE=wren" ]] || { echo "$p: $args"; return 1; }
    [[ "$args" != *" start "* ]] || return 1
  done
}

@test "werk-v2 is the cicd starter, unchanged" {
  run "$ROOT/platform/scripts/werk-v2" 4515 wren
  [ "$status" -eq 0 ]
  [[ "$(cat "$BATS_TEST_TMPDIR/args")" == *" enqueue $W/platform/pipelines/cicd.yaml -- CARD=4515 ROLE=wren" ]] || return 1
}

@test "negative proof: an unknown pipeline is refused by name and never enqueued" {
  run "$ROOT/platform/scripts/pipeline-start" no-such-pipeline 4515 wren
  [ "$status" -ne 0 ]
  [[ "$output" == *"no-such-pipeline has no generated pipeline yet"* ]] || return 1
  [ ! -f "$BATS_TEST_TMPDIR/args" ]
}

@test "negative proof: borg operate has no generated pipeline, refused by name" {
  run "$ROOT/platform/scripts/pipeline-start" borg-operate 4515 wren
  [ "$status" -ne 0 ]
  [[ "$output" == *"borg-operate has no generated pipeline yet"* ]] || return 1
  [ ! -f "$BATS_TEST_TMPDIR/args" ]
}

@test "negative proof: a starter never enqueues when its workflow file is missing" {
  rm "$W/platform/pipelines/athena.yaml"
  run "$ROOT/platform/scripts/pipeline-start" athena 4515 wren
  [ "$status" -ne 0 ]
  [[ "$output" == *"no workflow at $W/platform/pipelines/athena.yaml"* ]] || return 1
  [ ! -f "$BATS_TEST_TMPDIR/args" ]
}

@test "negative proof: a bad pipeline name, card or role is refused before dagu" {
  run "$ROOT/platform/scripts/pipeline-start" '../cicd' 4515 wren
  [ "$status" -ne 0 ]
  run "$ROOT/platform/scripts/pipeline-start" cicd '' wren
  [ "$status" -ne 0 ]
  run "$ROOT/platform/scripts/pipeline-start" cicd 4515 'Wren;rm'
  [ "$status" -ne 0 ]
  run "$ROOT/platform/scripts/pipeline-start" '' 4515 wren
  [ "$status" -ne 0 ]
  [ ! -f "$BATS_TEST_TMPDIR/args" ]
}

# #4637 AC4 — the run names the Jeff line whose turn started it
@test "a run started in a turn a Jeff line woke carries that line; any other run says none" {
  printf '#!/bin/sh\necho "$@" > "%s/args"\necho '"'"'{"msg":"Enqueued dag-run","run-id":"034abc"}'"'"'\n' "$BATS_TEST_TMPDIR" > "$DAGU"; chmod +x "$DAGU"
  export CHORUS_TURN_DIR="$BATS_TEST_TMPDIR/run" CHORUS_LOG_BIN="$BATS_TEST_TMPDIR/log"
  mkdir -p "$CHORUS_TURN_DIR"
  printf '#!/bin/sh\necho "$@" >> "%s/logged"\n' "$BATS_TEST_TMPDIR" > "$CHORUS_LOG_BIN"; chmod +x "$CHORUS_LOG_BIN"
  printf '{"turn_id":"t1","woken_by_line":"57629","at":%s}' "$(date +%s)" > "$CHORUS_TURN_DIR/turn-wren.json"
  run "$ROOT/platform/scripts/pipeline-start" cicd 4515 wren
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  grep -q "pipeline.run.started wren card=4515 pipeline=cicd run_id=034abc started_by=line started_by_line=57629" "$BATS_TEST_TMPDIR/logged" || { cat "$BATS_TEST_TMPDIR/logged"; return 1; }
  # NEGATIVE PROOF: a turn no line woke (a nudge) gives the run no line
  printf '{"turn_id":"t2","woken_by_line":null,"at":%s}' "$(date +%s)" > "$CHORUS_TURN_DIR/turn-wren.json"
  run "$ROOT/platform/scripts/pipeline-start" cicd 4515 wren
  tail -1 "$BATS_TEST_TMPDIR/logged" | grep -q "started_by=none" || { cat "$BATS_TEST_TMPDIR/logged"; return 1; }
  ! tail -1 "$BATS_TEST_TMPDIR/logged" | grep -q "started_by_line" || return 1
}

# #4753 — one go per card: cicd supersedes the card's older waiting run before
# it enqueues. NEGATIVE PROOF: athena never supersedes.
@test "cicd supersedes the card's older run first; athena does not" {
  export CICD_SUPERSEDE_BIN="$BATS_TEST_TMPDIR/sup"
  printf '#!/bin/sh\necho "sup $@" >> "%s/order"\n' "$BATS_TEST_TMPDIR" > "$CICD_SUPERSEDE_BIN"; chmod +x "$CICD_SUPERSEDE_BIN"
  printf '#!/bin/sh\necho "enqueue" >> "%s/order"\n' "$BATS_TEST_TMPDIR" > "$DAGU"
  run "$ROOT/platform/scripts/pipeline-start" cicd 4515 wren
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ "$(cat "$BATS_TEST_TMPDIR/order")" = "$(printf 'sup 4515 wren\nenqueue')" ] || { cat "$BATS_TEST_TMPDIR/order"; return 1; }
  rm -f "$BATS_TEST_TMPDIR/order"
  run "$ROOT/platform/scripts/pipeline-start" athena 4515 wren
  [ "$(cat "$BATS_TEST_TMPDIR/order")" = "enqueue" ] || { cat "$BATS_TEST_TMPDIR/order"; return 1; }
}
