#!/usr/bin/env bats
# @test-type: integration — spawns its own sleep fixture + fake launchctl; seamed spine; no live services touched
load test_helper
# chorus-scratch-kill.bats — #3750: the owner-sanctioned scratch-process exit.
# NEGATIVE PROOFS (#3734) are the point: the states this tool exists to refuse
# (a managed-service pid, a dead pid, init) are each driven and shown REFUSED.

SCRIPT="$CHORUS_ROOT/platform/scripts/chorus-scratch-kill"

setup() {
  membrane_world
  # Spine witness sink for the script (CHORUS_LOG_BIN honors CHORUS_LOG_FILE).
  export CHORUS_LOG_FILE="$BATS_TEST_TMPDIR/spine.log"
  export CHORUS_LOG_BIN="$HOME/.chorus/bin/chorus-hook-shim"
  # Fake launchctl: reports pid 99999 as a managed chorus service.
  mkdir -p "$BATS_TEST_TMPDIR/bin"
  cat > "$BATS_TEST_TMPDIR/bin/fake-launchctl" <<'FAKE'
#!/bin/sh
printf '99999\t0\tcom.chorus.fake-managed\n'
FAKE
  chmod +x "$BATS_TEST_TMPDIR/bin/fake-launchctl"
  export LAUNCHCTL="$BATS_TEST_TMPDIR/bin/fake-launchctl"
}

@test "kills an unmanaged scratch process and witnesses it on the (seamed) spine" {
  sleep 300 &
  local pid=$!
  run bash "$SCRIPT" "$pid" "bats fixture cleanup"
  echo "output: $output"
  [ "$status" -eq 0 ]
  # process is gone (allow a beat for signal delivery)
  sleep 1
  ! kill -0 "$pid" 2>/dev/null
  grep -q '"event":"process.scratch.killed"' "$CHORUS_LOG_FILE"
  grep -q "\"pid\":\"$pid\"" "$CHORUS_LOG_FILE" || grep -q "pid=$pid" "$CHORUS_LOG_FILE"
}

@test "NEGATIVE PROOF: a managed-service pid is REFUSED, typed" {
  # 99999 is 'managed' per the fake launchctl. It must refuse BEFORE any
  # existence check outcome matters — but the script checks existence first,
  # so give the fake pid a real process: spawn one and teach the fake
  # launchctl to claim it.
  sleep 300 &
  local pid=$!
  cat > "$BATS_TEST_TMPDIR/bin/fake-launchctl" <<FAKE
#!/bin/sh
printf '$pid\t0\tcom.chorus.fake-managed\n'
FAKE
  chmod +x "$BATS_TEST_TMPDIR/bin/fake-launchctl"
  run bash "$SCRIPT" "$pid" "should never happen"
  echo "output: $output"
  [ "$status" -eq 2 ]
  echo "$output" | grep -q "MANAGED service"
  echo "$output" | grep -q "agent-state.sh"
  # and the process is UNHARMED
  kill -0 "$pid" 2>/dev/null
  kill -TERM "$pid" 2>/dev/null || true   # fixture cleanup (direct kill fine inside bats' own child)
}

@test "NEGATIVE PROOF: nonexistent pid refused" {
  run bash "$SCRIPT" 4009999 "no such pid"
  [ "$status" -eq 2 ]
  echo "$output" | grep -q "does not exist"
}

@test "NEGATIVE PROOF: pid 1 refused" {
  run bash "$SCRIPT" 1 "absolutely not"
  [ "$status" -eq 2 ]
  echo "$output" | grep -q "init/kernel"
}
