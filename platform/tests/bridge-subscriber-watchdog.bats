#!/usr/bin/env bats
# @test-type: integration — operational; live services, skip-if-absent in CI
load test_helper
# bridge-subscriber-watchdog.bats — #1964
# What Jeff sees: bridge-subscribers stay alive across crashes. When one dies,
# watchdog kickstarts within a cycle, spine event records the restart.
# Hermetic: LAUNCHCTL_BIN + CHORUS_LOG_BIN env vars are test seams so bats
# doesn't actually kickstart live services or pollute chorus.log.

WATCHDOG="${CHORUS_ROOT}/platform/scripts/bridge-subscriber-watchdog.sh"

setup() {
  export TEST_TMP="$(mktemp -d)"
  export LAUNCHCTL_BIN="$TEST_TMP/launchctl-mock"
  export CHORUS_LOG_BIN="$TEST_TMP/chorus-log-mock"
  export CHORUS_LOG_FILE="$TEST_TMP/events.log"
  export CHORUS_WATCHDOG_STATE_DIR="$TEST_TMP/state"
  mkdir -p "$CHORUS_WATCHDOG_STATE_DIR"

  # Mock chorus-log: just append args to file
  cat > "$CHORUS_LOG_BIN" <<'EOF'
#!/bin/bash
echo "$@" >> "$CHORUS_LOG_FILE"
EOF
  chmod +x "$CHORUS_LOG_BIN"
}

teardown() {
  rm -rf "$TEST_TMP"
}

# Helper: mock launchctl that returns specified output per role
mock_launchctl_alive() {
  # All 3 roles alive
  cat > "$LAUNCHCTL_BIN" <<'EOF'
#!/bin/bash
case "$1 $2" in
  "list com.chorus.bridge-subscriber-silas") echo '{"PID" = 1001; "LastExitStatus" = 0;}' ;;
  "list com.chorus.bridge-subscriber-kade")  echo '{"PID" = 1002; "LastExitStatus" = 0;}' ;;
  "list com.chorus.bridge-subscriber-wren")  echo '{"PID" = 1003; "LastExitStatus" = 0;}' ;;
  "kickstart"*) echo "kickstart $3"; exit 0 ;;
  *) exit 0 ;;
esac
EOF
  chmod +x "$LAUNCHCTL_BIN"
}

mock_launchctl_one_dead() {
  local dead_role="$1"
  cat > "$LAUNCHCTL_BIN" <<EOF
#!/bin/bash
case "\$1 \$2" in
  "list com.chorus.bridge-subscriber-${dead_role}") exit 113 ;;  # not running
  "list com.chorus.bridge-subscriber-silas") echo '{"PID" = 1001;}' ;;
  "list com.chorus.bridge-subscriber-kade")  echo '{"PID" = 1002;}' ;;
  "list com.chorus.bridge-subscriber-wren")  echo '{"PID" = 1003;}' ;;
  "kickstart"*) echo "kickstart \$3"; exit 0 ;;
  *) exit 0 ;;
esac
EOF
  chmod +x "$LAUNCHCTL_BIN"
}

mock_launchctl_kickstart_fail() {
  local dead_role="$1"
  cat > "$LAUNCHCTL_BIN" <<EOF
#!/bin/bash
case "\$1 \$2" in
  "list com.chorus.bridge-subscriber-${dead_role}") exit 113 ;;
  "list com.chorus.bridge-subscriber-silas") echo '{"PID" = 1001;}' ;;
  "list com.chorus.bridge-subscriber-kade")  echo '{"PID" = 1002;}' ;;
  "list com.chorus.bridge-subscriber-wren")  echo '{"PID" = 1003;}' ;;
  "kickstart"*) echo "kickstart failed"; exit 1 ;;
  *) exit 0 ;;
esac
EOF
  chmod +x "$LAUNCHCTL_BIN"
}

# --- AC: watchdog exists + invocable ---

@test "watchdog script exists and is executable" {
  [ -x "$WATCHDOG" ]
}

@test "watchdog rejects unknown flag" {
  run "$WATCHDOG" --nonsense-flag
  [ "$status" -ne 0 ]
}

# --- AC: all alive → 3 healthy events (when hourly rate-limiter allows) ---

@test "all subscribers alive emits 3 healthy events on first run" {
  mock_launchctl_alive
  run "$WATCHDOG"
  [ "$status" -eq 0 ]
  # First run — no prior state, all 3 healthy events fire
  healthy_count=$(grep -c "bridge.subscriber.healthy" "$CHORUS_LOG_FILE" || true)
  [ "$healthy_count" -eq 3 ]
}

@test "rate-limit: second run within the hour does NOT re-emit healthy events" {
  mock_launchctl_alive
  run "$WATCHDOG"
  [ "$status" -eq 0 ]
  healthy1=$(grep -c "bridge.subscriber.healthy" "$CHORUS_LOG_FILE" || true)
  # Immediate re-run — should rate-limit
  run "$WATCHDOG"
  healthy2=$(grep -c "bridge.subscriber.healthy" "$CHORUS_LOG_FILE" || true)
  [ "$healthy1" -eq "$healthy2" ]
}

# --- AC: one dead → 1 restart event + kickstart called ---

@test "one subscriber dead emits exactly 1 restart event + kickstarts that role" {
  mock_launchctl_one_dead "kade"
  run "$WATCHDOG"
  [ "$status" -eq 0 ]
  restart_count=$(grep -c "bridge.subscriber.restart" "$CHORUS_LOG_FILE" || true)
  [ "$restart_count" -eq 1 ]
  # The restart event names the kade role
  grep -q "role=kade" "$CHORUS_LOG_FILE"
}

# --- AC: kickstart failure surfaces in spine event ---

@test "kickstart failure emits restart event with error field" {
  mock_launchctl_kickstart_fail "wren"
  run "$WATCHDOG"
  [ "$status" -eq 0 ]
  # restart event fired, with error marker
  grep "bridge.subscriber.restart" "$CHORUS_LOG_FILE" | grep -q "error="
}

# --- AC: pinned 3 roles (silas/kade/wren) ---

@test "watchdog checks all 3 known roles" {
  mock_launchctl_alive
  run "$WATCHDOG"
  [ "$status" -eq 0 ]
  # Healthy events name each role at least once
  grep "bridge.subscriber.healthy" "$CHORUS_LOG_FILE" | grep -q "role=silas"
  grep "bridge.subscriber.healthy" "$CHORUS_LOG_FILE" | grep -q "role=kade"
  grep "bridge.subscriber.healthy" "$CHORUS_LOG_FILE" | grep -q "role=wren"
}
