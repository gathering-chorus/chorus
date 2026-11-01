#!/usr/bin/env bats
# @test-type: integration — runs the chorus-runner suites that hold #4851's ACs
# @domain: services
# @card: 4851 · owner: silas
# What Jeff sees: Borg Queen's and Abby's tool calls are answered by the shared
# runner as themselves, so a mob counts their verdicts (#4789). `chorus-runner
# stdio` forwards to :3342 and runs nothing itself; a runner that is down is an
# error, never a local run.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

@test "AC1/AC2: stdio forwards to the HTTP runner — SSE unwrapped, re-mint once on 401, down = error, no local record (cargo)" {
  cd "$ROOT/platform/services/chorus-runner"
  run env CARGO_BUILD_JOBS=4 nice cargo test --test forward_4851 --test stdio_4515
  echo "$output" | grep -E 'test result|panicked' || true
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"6 passed"* ]] || return 1
}
