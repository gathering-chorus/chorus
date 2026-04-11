#!/bin/bash
# bedroom-heartbeat.sh — ping Bedroom Mac, notify Jeff if unreachable
# Runs via LaunchAgent every 5 minutes.
# Silas owns. 2026-03-23.

BEDROOM_IP="192.168.86.242"
STATE_FILE="/tmp/bedroom-heartbeat-state"
LOG="/tmp/bedroom-heartbeat.log"

# Ping with 3-second timeout
if ping -c 1 -t 3 "$BEDROOM_IP" >/dev/null 2>&1; then
  # Bedroom is up
  if [ -f "$STATE_FILE" ] && [ "$(cat "$STATE_FILE")" = "down" ]; then
    # Was down, now recovered
    osascript -e "display notification \"Bedroom Mac is back online\" with title \"Gathering\" sound name \"Glass\"" 2>/dev/null
    echo "$(TZ=America/New_York date '+%H:%M') RECOVERED" >> "$LOG"
  fi
  echo "up" > "$STATE_FILE"
else
  # Bedroom is down
  if [ ! -f "$STATE_FILE" ] || [ "$(cat "$STATE_FILE")" != "down" ]; then
    # First detection of outage
    osascript -e "display notification \"Bedroom Mac is unreachable — check power/network\" with title \"Gathering\" subtitle \"192.168.86.242 not responding\" sound name \"Basso\"" 2>/dev/null
    echo "$(TZ=America/New_York date '+%H:%M') DOWN" >> "$LOG"
  fi
  echo "down" > "$STATE_FILE"
fi
