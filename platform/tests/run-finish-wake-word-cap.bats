#!/usr/bin/env bats
# @test-type: unit — run-finished's failure wake through the real nudge word cap, pulse caged
# @domain: pipelines
# @card: #4889 · owner: silas
# What Jeff sees: a failed run always wakes its owner. #4889 run 5 (07:11) failed and
# nobody woke: platform/scripts/run-finished built a 52-word wake (the reason carries
# retry commands), the nudge word cap (50) refused it, and run.wake.failed said no why.
# The reason now rides in a fence, which the cap does not count; a refusal's first line
# goes on the spine.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
RF="$ROOT/platform/scripts/run-finished"

setup() {
  H="$BATS_TEST_TMPDIR/home"; mkdir -p "$H/ops/active-pairs"
  # run 5's own reason, verbatim (chorus-werk-status --why cicd 034sIjDspy8e2VcCHIxIjy)
  LONG="cicd failed at test-pipeline: test-verdict: werk-test: UNMEASURED — no test covers package.json (exit 2) · retry skill-werk-test-verdict alone: dagu --config $ROOT/platform/config/dagu.yaml retry --run-id=034sIjDspy8e2VcCHIxIjy --sub-run-id=GVwch3UFCSt4wCNWPBzsHHsC1pToMSgRh7Gs88h7tXVq --step=skill-werk-test-verdict --downstream cicd · then: dagu --config $ROOT/platform/config/dagu.yaml retry --run-id=034sIjDspy8e2VcCHIxIjy --step=skill-werk-deploy-demo --downstream cicd · or the whole step: dagu --config $ROOT/platform/config/dagu.yaml retry --run-id=034sIjDspy8e2VcCHIxIjy --step=skill-werk-test-pipeline --downstream cicd"
  printf '#!/bin/sh\nprintf "%%s\\n" "%s"\n' "$LONG" > "$BATS_TEST_TMPDIR/why"; chmod +x "$BATS_TEST_TMPDIR/why"
  printf '#!/bin/sh\necho "$@" >> %s/spine\n' "$BATS_TEST_TMPDIR" > "$BATS_TEST_TMPDIR/log"; chmod +x "$BATS_TEST_TMPDIR/log"
  SHIM="$(command -v chorus-hook-shim || echo "$HOME/.chorus/bin/chorus-hook-shim")"
  [ -x "$SHIM" ] || { echo "no chorus-hook-shim to count words with"; return 1; }
}

# run-finished with the real shim; pulse is a dead port, so a wake that passes the cap
# fails only at transport ("pulse did not answer"), never at "word-cap"
finish() {
  env -u CHORUS_ROLE -u CHORUS_NUDGE_SIGNS -u BUZZ_RELAY_URL CHORUS_LOG_FILE="$BATS_TEST_TMPDIR/chorus.log" CHORUS_HOME="$H" CHORUS_HOOK_SHIM="$SHIM" CHORUS_PULSE_URL=http://127.0.0.1:9/api/nudge \
    RUN_FINISHED_WHY="$BATS_TEST_TMPDIR/why" RUN_FINISHED_LOG="$BATS_TEST_TMPDIR/log" \
    DAG_RUN_STATUS=failed DAG_RUN_STEP_NAME=onExit bash "$RF" 4889 silas cicd run-5
}

@test "a failed run's long reason passes the word cap: the only refusal left is the caged transport" {
  run finish
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" != *"word-cap"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"pulse did not answer"* ]] || { echo "$output"; return 1; }
}

@test "the refusal's first line is on the spine with the failed wake" {
  run finish
  grep -q 'run.wake.failed system card=4889 dag=cicd to=silas status=failed run=run-5 reason=ops-nudge: transport error' "$BATS_TEST_TMPDIR/spine" \
    || { cat "$BATS_TEST_TMPDIR/spine"; return 1; }
}

@test "negative proof: the same reason outside a fence is refused by the word cap" {
  run env -u CHORUS_ROLE -u CHORUS_NUDGE_SIGNS -u BUZZ_RELAY_URL CHORUS_LOG_FILE="$BATS_TEST_TMPDIR/chorus.log" CHORUS_PULSE_URL=http://127.0.0.1:9/api/nudge "$SHIM" nudge-send system silas "#4889 cicd $LONG · dagu 034sIjDspy8e2VcCHIxIjy · /cws 4889"
  [[ "$output" == *"word-cap"* ]] || { echo "$output"; return 1; }
}
