#!/usr/bin/env bats
# @test-type: unit — hermetic: NIGHTLY_LOCKDIR under BATS_TEST_TMPDIR, the process table stubbed through NIGHTLY_PS; no live runner, no launchd, no network
# #4008 — the single-flight lock guards the RUNNER, not just the wrapper. On 2026-08-25
# a killed wrapper left `werk-test --nightly` alive for 1h52m; its pid was dead, so the
# next wrapper stole the lock and ran a second lane beside the orphan. Now a dead holder
# with a live runner is a typed refusal that names the runner's pid and age; a dead
# holder with no runner is a clean steal. Both states are shown (#3734).

setup() {
  NS="$BATS_TEST_DIRNAME/../scripts/nightly-suites.sh"
  export NIGHTLY_LOCKDIR="$BATS_TEST_TMPDIR/lock.d"
  export OPS_NUDGE="$BATS_TEST_TMPDIR/ops-nudge"   # capture, never page anyone
  printf '#!/bin/bash\nprintf "%%s\\n" "$*" >> "%s/nudges"\n' "$BATS_TEST_TMPDIR" > "$OPS_NUDGE"; chmod +x "$OPS_NUDGE"
  # a stub process table; the proof writes the lines it wants seen
  export NIGHTLY_PS="cat $BATS_TEST_TMPDIR/ps.txt"
  printf '  PID  PPID     ELAPSED COMMAND\n' > "$BATS_TEST_TMPDIR/ps.txt"
  # a holder pid that is certainly dead
  DEAD_PID=4194000
  while kill -0 "$DEAD_PID" 2>/dev/null; do DEAD_PID=$((DEAD_PID - 7)); done
}

stale_lock() { mkdir -p "$NIGHTLY_LOCKDIR"; echo "$DEAD_PID" > "$NIGHTLY_LOCKDIR/pid"; }

@test "no lock at all: the probe ACQUIRES" {
  run bash "$NS" --lock-probe
  [ "$status" -eq 0 ]
  [ "$output" = "ACQUIRED" ]
}

@test "dead holder, no runner alive: the stale lock is stolen — ACQUIRED" {
  stale_lock
  run bash "$NS" --lock-probe
  [ "$status" -eq 0 ]
  [ "$output" = "ACQUIRED" ]
}

@test "NEGATIVE PROOF: dead holder but a runner still alive — REFUSED, naming the runner pid and age" {
  stale_lock
  printf '4242 1 01:52:03 /Users/x/.chorus/bin/werk-test --nightly\n' >> "$BATS_TEST_TMPDIR/ps.txt"
  run bash "$NS" --lock-probe
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "^REFUSED holder pid $DEAD_PID is dead but runner pid 4242 is alive (age 01:52:03)" || { echo "$output"; false; }
  # the lock was NOT stolen
  [ "$(cat "$NIGHTLY_LOCKDIR/pid")" = "$DEAD_PID" ]
}

@test "a live holder is still a refusal, and says so" {
  mkdir -p "$NIGHTLY_LOCKDIR"; echo "$$" > "$NIGHTLY_LOCKDIR/pid"
  run bash "$NS" --lock-probe
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "^REFUSED holder pid $$ is alive" || { echo "$output"; false; }
}

@test "the runner check ignores unrelated processes and this wrapper itself" {
  stale_lock
  printf '5151 1 00:03:10 /bin/bash /some/other/script --run-all-the-things\n' >> "$BATS_TEST_TMPDIR/ps.txt"
  printf '5252 1 00:00:02 bash %s --lock-probe\n' "$NS" >> "$BATS_TEST_TMPDIR/ps.txt"
  run bash "$NS" --lock-probe
  [ "$output" = "ACQUIRED" ] || { echo "$output"; false; }
}

@test "--run-all under a live runner refuses LOUDLY: typed line on stderr, ops-nudge fired, exit 0, nothing run" {
  stale_lock
  printf '4242 1 00:41:00 /Users/x/.chorus/bin/werk-test --nightly\n' >> "$BATS_TEST_TMPDIR/ps.txt"
  export NIGHTLY_LOG_PATH="$BATS_TEST_TMPDIR/nightly.log"
  run bash "$NS" --run-all
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "REFUSED — holder pid $DEAD_PID is dead but runner pid 4242 is alive (age 00:41:00)" || { echo "$output"; false; }
  grep -q "runner pid 4242 is alive" "$BATS_TEST_TMPDIR/nudges"
  ! [ -f "$NIGHTLY_LOG_PATH" ] || ! grep -q "^RUN|start" "$NIGHTLY_LOG_PATH"
}
