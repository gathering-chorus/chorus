#!/usr/bin/env bats
# @test-type: unit — a fake dagu records the enqueue and plays back statuses; no scheduler.
# @domain: pipelines — one test run at a time (#4675)
# platform/scripts/dagu-enqueue-wait: a pair's local proof must ENQUEUE cicd-test
# on its queue (a dag.run sub-run ignores its queue, #4494) and wait for it.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  SCRIPT="$ROOT/platform/scripts/dagu-enqueue-wait"
  FAKE="$BATS_TEST_TMPDIR/dagu"
  LOG="$BATS_TEST_TMPDIR/calls"
  STATES="$BATS_TEST_TMPDIR/states"
  cat > "$FAKE" <<FAKE
#!/usr/bin/env bash
echo "\$*" >> "$LOG"
case "\$*" in
  *" enqueue "*) echo '{"msg":"Enqueued dag-run"}' ;;
  *" status "*) s=\$(head -1 "$STATES"); [ \$(wc -l < "$STATES") -gt 1 ] && sed -i '' 1d "$STATES"; echo "\$s - 2026-10-21" ;;
esac
FAKE
  chmod +x "$FAKE"
  export DAGU="$FAKE" DAGU_CONFIG=/dev/null ENQUEUE_WAIT_POLL_S=0 ENQUEUE_WAIT_TIMEOUT_S=5
}

@test "enqueues the child on its queue (never a sub-run) and passes its params" {
  printf 'Queued\nRunning\nSucceeded\n' > "$STATES"
  run "$SCRIPT" cicd-test CARD=4675 ROLE=silas SCOPE=diff
  [ "$status" -eq 0 ]
  grep -q " enqueue --run-id cicd-test-.* cicd-test -- CARD=4675 ROLE=silas SCOPE=diff" "$LOG"
  [[ "$output" == *"result=succeeded"* ]] || return 1
}

@test "waits while queued: status is read until the child finishes" {
  printf 'Queued\nQueued\nQueued\nSucceeded\n' > "$STATES"
  run "$SCRIPT" cicd-test CARD=1
  [ "$status" -eq 0 ]
  [ "$(grep -c ' status ' "$LOG")" -eq 4 ]
}

# NEGATIVE PROOF: a failed child fails the step (a wait that always passed would hide reds)
@test "a failed child fails the step" {
  printf 'Running\nFailed\n' > "$STATES"
  run "$SCRIPT" cicd-test CARD=1
  [ "$status" -eq 1 ]
  [[ "$output" == *"result=failed"* ]] || return 1
}

@test "a child that never finishes fails at the timeout, never passes" {
  printf 'Queued\n' > "$STATES"
  ENQUEUE_WAIT_TIMEOUT_S=0 run "$SCRIPT" cicd-test CARD=1
  [ "$status" -eq 1 ]
}
