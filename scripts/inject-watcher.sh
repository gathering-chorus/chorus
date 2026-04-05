#!/usr/bin/env bash
# inject-watcher.sh — LaunchAgent that delivers queued nudges via osascript
#
# Polls /tmp/voice-inbox/*/pending-inject.txt every 2 seconds.
# When a file exists, delivers each line via chorus-inject, then removes the file.
# Runs in its own process chain (launchd → this script → chorus-inject → osascript)
# so TCC checks chorus-inject as the responsible process, not Claude Code.

set -uo pipefail

INJECT="/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-inject/target/release/chorus-inject"
INBOX_ROOT="/tmp/voice-inbox"
LOG="/Users/jeffbridwell/Library/Logs/Chorus/inject-watcher.log"

log() { echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" >> "$LOG"; }

while true; do
  for role in wren silas kade; do
    INBOX="$INBOX_ROOT/$role/pending-inject.txt"
    if [ -f "$INBOX" ]; then
      # Atomic rename to prevent race with writers
      DRAIN="$INBOX_ROOT/$role/draining-watcher-$$.txt"
      if mv "$INBOX" "$DRAIN" 2>/dev/null; then
        while IFS= read -r line; do
          [ -z "$line" ] && continue
          if "$INJECT" "$role" "$line" 2>/dev/null; then
            log "INJECTED to $role: ${line:0:80}"
          else
            log "INJECT FAILED for $role: ${line:0:80}"
          fi
        done < "$DRAIN"
        rm -f "$DRAIN"
      fi
    fi
  done
  sleep 2
done
