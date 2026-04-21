#!/usr/bin/env bash
# bridge-subscriber-watchdog.sh — #1964
# Checks the 3 bridge-subscriber LaunchAgents; kickstarts any that aren't
# running. Emits spine events so silent-deaths become observable.
#
# Scheduled by com.chorus.bridge-subscriber-watchdog.plist every 5 minutes.
# Hermetic test seams via env vars:
#   LAUNCHCTL_BIN              — default: launchctl (tests override with mock)
#   CHORUS_LOG_BIN             — default: chorus-log symlink
#   CHORUS_WATCHDOG_STATE_DIR  — default: /tmp/bridge-subscriber-watchdog
#   CHORUS_WATCHDOG_HEALTHY_INTERVAL — seconds between healthy-events per role (default 3600)

set -u

# Reject unknown flags so tests can assert invalid invocation fails
for arg in "$@"; do
  case "$arg" in
    --help|-h) echo "Usage: $0  (no flags; scheduled by LaunchAgent)"; exit 0 ;;
    -*|--*)    echo "Unknown flag: $arg" >&2; exit 2 ;;
  esac
done

ROLES=(silas kade wren)
LAUNCHCTL_BIN="${LAUNCHCTL_BIN:-launchctl}"
CHORUS_LOG_BIN="${CHORUS_LOG_BIN:-/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log}"
STATE_DIR="${CHORUS_WATCHDOG_STATE_DIR:-/tmp/bridge-subscriber-watchdog}"
HEALTHY_INTERVAL="${CHORUS_WATCHDOG_HEALTHY_INTERVAL:-3600}"
mkdir -p "$STATE_DIR"

emit() {
  local event="$1"; shift
  local role="$1"; shift
  "$CHORUS_LOG_BIN" "$event" system "role=${role}" "$@" >/dev/null 2>&1 || true
}

should_emit_healthy() {
  local role="$1"
  local marker="$STATE_DIR/last-healthy-${role}"
  local now=$(date +%s)
  if [ ! -f "$marker" ]; then
    echo "$now" > "$marker"
    return 0
  fi
  local last=$(cat "$marker" 2>/dev/null || echo 0)
  local elapsed=$((now - last))
  if [ "$elapsed" -ge "$HEALTHY_INTERVAL" ]; then
    echo "$now" > "$marker"
    return 0
  fi
  return 1
}

for role in "${ROLES[@]}"; do
  label="com.chorus.bridge-subscriber-${role}"
  if "$LAUNCHCTL_BIN" list "$label" >/dev/null 2>&1; then
    if should_emit_healthy "$role"; then
      emit bridge.subscriber.healthy "$role" "label=${label}"
    fi
  else
    kickstart_out=$("$LAUNCHCTL_BIN" kickstart "gui/$(id -u)/${label}" 2>&1)
    kickstart_rc=$?
    if [ "$kickstart_rc" -eq 0 ]; then
      emit bridge.subscriber.restart "$role" "label=${label}" "reason=missing"
    else
      emit bridge.subscriber.restart "$role" "label=${label}" "reason=missing" "error=${kickstart_out}"
    fi
  fi
done
