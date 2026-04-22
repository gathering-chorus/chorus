#!/usr/bin/env bash
# install-tick-poller-plists.sh — #2435 §2
#
# Writes per-role LaunchAgent plists for spine-tick-poller to
# ~/Library/LaunchAgents/com.chorus.spine-tick-poller.<role>.plist.
# Plists ship with NUDGE_TICK_POLLER_ACTIVE=0 (gated off). Operator
# activates per-role by editing the plist env var to =1 and reloading:
#
#   launchctl unload ~/Library/LaunchAgents/com.chorus.spine-tick-poller.<role>.plist
#   launchctl load   ~/Library/LaunchAgents/com.chorus.spine-tick-poller.<role>.plist
#
# Activation is intentionally manual during the #2435 SLO proof period —
# parallel-primary must never happen (Wren 06:50). Inject stays canonical
# floor; tick-poller runs gated-off collecting zero load until operator
# flips for SLO measurement, then flag-ffips + inject retires in §5 cutover.

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
SCRIPT="${CHORUS_ROOT}/platform/scripts/spine-tick-poller"
AGENTS_DIR="${HOME}/Library/LaunchAgents"
LOG_DIR="${HOME}/Library/Logs/Chorus"

mkdir -p "$AGENTS_DIR" "$LOG_DIR"

if [ ! -x "$SCRIPT" ]; then
  echo "install-tick-poller-plists: spine-tick-poller not executable at $SCRIPT" >&2
  exit 1
fi

for role in wren silas kade; do
  plist="${AGENTS_DIR}/com.chorus.spine-tick-poller.${role}.plist"
  cat > "$plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.chorus.spine-tick-poller.${role}</string>
    <key>ProgramArguments</key>
    <array>
        <string>/bin/bash</string>
        <string>${SCRIPT}</string>
        <string>${role}</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>NUDGE_TICK_POLLER_ACTIVE</key>
        <string>0</string>
        <key>CHORUS_ROOT</key>
        <string>${CHORUS_ROOT}</string>
    </dict>
    <key>KeepAlive</key>
    <true/>
    <key>RunAtLoad</key>
    <true/>
    <key>StandardErrorPath</key>
    <string>${LOG_DIR}/spine-tick-poller.${role}.log</string>
    <key>StandardOutPath</key>
    <string>${LOG_DIR}/spine-tick-poller.${role}.log</string>
</dict>
</plist>
PLIST
  echo "wrote $plist"
done

cat <<EOF

Plists installed (gated off). To activate one role for SLO measurement:

  1. Edit the NUDGE_TICK_POLLER_ACTIVE value from "0" to "1" in:
       ${AGENTS_DIR}/com.chorus.spine-tick-poller.<role>.plist
  2. Reload:
       launchctl unload ${AGENTS_DIR}/com.chorus.spine-tick-poller.<role>.plist
       launchctl load   ${AGENTS_DIR}/com.chorus.spine-tick-poller.<role>.plist

Live telemetry then appears as nudge.surfaced events on spine. Pulse
'nudges' section surfaces lag_ms when the new telemetry lands (§4 AC).
EOF
