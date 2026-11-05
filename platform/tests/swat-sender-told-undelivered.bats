#!/usr/bin/env bats
# @test-type: unit — nudge-send's answer to the sender, and the ops path that carries it
# @domain: messages
# @card: #4889 · owner: silas
# What Jeff sees: nobody is told "nudge sent" for a nudge that never arrived. pulse
# (platform/pulse/src/service.ts deliveryVerdict) answers delivered, queued or
# not-delivered with the reason; nudge-send (platform/services/chorus-hooks/src/nudge_send.rs)
# prints it and fails on not-delivered; platform/scripts/ops-nudge passes the refusal on,
# and platform/scripts/run-finished records it as run.wake.failed with its reason.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
CH="$ROOT/platform/services/chorus-hooks"

@test "nudge-send: not-delivered is an error with pulse's reason; queued says not seen yet" {
  run env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$CH/target/$(id -un)" nice cargo test -q --manifest-path "$CH/Cargo.toml" --test nudge_send_4515 sender_told
  [ "$status" -eq 0 ] || { echo "$output" | tail -20; return 1; }
  [[ "$output" == *"3 passed"* ]] || { echo "$output" | tail -5; return 1; }
}

# a shim that answers like nudge-send does when pulse says not-delivered
not_delivered_shim() {
  printf '#!/bin/sh\necho "nudge NOT delivered: system → abby-normal: dead-pane %%12 (trace=t)"; exit 1\n' > "$1"; chmod +x "$1"
}

@test "ops-nudge refuses (exit 3) with the reason, never success" {
  not_delivered_shim "$BATS_TEST_TMPDIR/shim"
  run env -u CHORUS_ROLE CHORUS_HOOK_SHIM="$BATS_TEST_TMPDIR/shim" bash "$ROOT/platform/scripts/ops-nudge" abby-normal "hello"
  [ "$status" -eq 3 ] || { echo "status $status: $output"; return 1; }
  [[ "$output" == *"nudge NOT delivered"*"dead-pane %12"* ]] || { echo "$output"; return 1; }
}

@test "a run-finish wake that was not delivered lands as run.wake.failed with its reason" {
  H="$BATS_TEST_TMPDIR/home"; mkdir -p "$H/ops/active-pairs"
  not_delivered_shim "$BATS_TEST_TMPDIR/shim"
  printf '#!/bin/sh\necho "failed at test-pipeline: werk-test: RED (exit 1)"\n' > "$BATS_TEST_TMPDIR/why"; chmod +x "$BATS_TEST_TMPDIR/why"
  printf '#!/bin/sh\necho "$@" >> %s/spine\n' "$BATS_TEST_TMPDIR" > "$BATS_TEST_TMPDIR/log"; chmod +x "$BATS_TEST_TMPDIR/log"
  run env -u CHORUS_ROLE CHORUS_HOME="$H" CHORUS_HOOK_SHIM="$BATS_TEST_TMPDIR/shim" RUN_FINISHED_WHY="$BATS_TEST_TMPDIR/why" \
    RUN_FINISHED_LOG="$BATS_TEST_TMPDIR/log" DAG_RUN_STATUS=failed DAG_RUN_STEP_NAME=onExit bash "$ROOT/platform/scripts/run-finished" 4889 abby-normal cicd run-9
  grep -q 'run.wake.failed system card=4889 dag=cicd to=abby-normal status=failed run=run-9 reason=ops-nudge: refused — nudge NOT delivered' "$BATS_TEST_TMPDIR/spine" \
    || { cat "$BATS_TEST_TMPDIR/spine"; return 1; }
}

@test "negative proof: a delivered answer is success, so the checks above can tell the two apart" {
  printf '#!/bin/sh\necho "nudge delivered: system → kade (trace=t)"\n' > "$BATS_TEST_TMPDIR/ok"; chmod +x "$BATS_TEST_TMPDIR/ok"
  run env -u CHORUS_ROLE CHORUS_HOOK_SHIM="$BATS_TEST_TMPDIR/ok" bash "$ROOT/platform/scripts/ops-nudge" kade "hello"
  [ "$status" -eq 0 ] || { echo "status $status: $output"; return 1; }
}
