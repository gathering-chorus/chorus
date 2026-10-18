#!/usr/bin/env bats
# @test-type: integration — runs the generated cicd-test workflow on a scratch dagu (own dags/data/log dirs); a stand-in werk-test, no live scheduler
# @domain: tests
# @card: 4541 · owner: kade
# What Jeff sees: when a werk-test step breaks, the run stops at that step and
# /cws names it; the steps after it never run. Negative proof: the same
# workflow with nothing broken runs to the end.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  S="$BATS_TEST_TMPDIR"
  mkdir -p "$S/dags" "$S/werks/kade-9999/platform/services/werk-test/target/release" "$S/werks/kade-bin"
  cp "$ROOT/platform/pipelines/cicd-test.yaml" "$S/dags/"
  printf '#!/bin/sh\n[ -f "%s/break-$1" ] && { echo "werk-test: $1 broke (planted)"; exit 1; }\necho "werk-test $1 ok"\n' "$S" \
    > "$S/werks/kade-9999/platform/services/werk-test/target/release/werk-test"
  chmod +x "$S/werks/kade-9999/platform/services/werk-test/target/release/werk-test"
  printf 'dags_dir: %s/dags\ndata_dir: %s/data\nlog_dir: %s/logs\n' "$S" "$S" "$S" > "$S/config.yaml"
  DAGU="${DAGU:-/opt/homebrew/bin/dagu}"
}

run_test_dag() { # RUN_ID
  CHORUS_WERK_BASE="$S/werks" CHORUS_HOME="$S" "$DAGU" --config "$S/config.yaml" start --run-id "$1" cicd-test -- CARD=9999 ROLE=kade SCOPE=diff TRACE=t >/dev/null 2>&1
  "$DAGU" --config "$S/config.yaml" status --run-id "$1" cicd-test 2>&1
}

@test "a broken step stops the run at that step, by name, and nothing after it runs" {
  touch "$S/break-prepare"
  out="$(run_test_dag broke)"
  [[ "$out" == *"skill-werk-test-prepare"*"[failed]"* ]] || { echo "$out"; return 1; }
  for later in run report verdict; do
    [[ "$out" == *"skill-werk-test-$later [aborted]"* ]] || { echo "$later: $out"; return 1; }
  done
  [[ "$out" == *"Result: Failed"* ]] || return 1
}

@test "negative proof: with nothing broken the same workflow runs to the end" {
  out="$(run_test_dag clean)"
  [[ "$out" == *"Result: Succeeded"* ]] || { echo "$out"; return 1; }
  [[ "$out" != *"[failed]"* && "$out" != *"[aborted]"* ]] || { echo "$out"; return 1; }
}
