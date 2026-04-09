#!/usr/bin/env bash
# inject-watcher.sh — LaunchAgent that delivers queued nudges via osascript
#
# Polls /tmp/voice-inbox/*/pending-inject.txt every 2 seconds.
# When a file exists, delivers each line via chorus-inject, then removes the file.
# Runs in its own process chain (launchd → this script → chorus-inject → osascript)
# so TCC checks chorus-inject as the responsible process, not Claude Code.

set -uo pipefail

INBOX_ROOT="/tmp/voice-inbox"
LOG="/Users/jeffbridwell/Library/Logs/Chorus/inject-watcher.log"

log() { echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" >> "$LOG"; }

# Role directory patterns for Terminal window matching
role_pattern() {
  case "$1" in
    wren) echo "product-manager" ;;
    silas) echo "architect" ;;
    kade) echo "engineer" ;;
    *) echo "" ;;
  esac
}

# Inject text into a role's Terminal window via osascript directly.
# No external binary — osascript inherits this process's TCC grant. (#2100)
inject_to_role() {
  local role="$1"
  local text="$2"
  local pattern
  pattern=$(role_pattern "$role")
  [ -z "$pattern" ] && return 1

  # Escape for AppleScript double-quoted strings
  local escaped
  escaped=$(printf '%s' "$text" | sed 's/\\/\\\\/g; s/"/\\"/g')

  osascript -e "
tell application \"System Events\"
    set originalApp to name of first application process whose frontmost is true
end tell
tell application \"Terminal\"
    set winCount to count of windows
    repeat with i from 1 to winCount
        set w to window i
        set winName to name of w
        if winName contains \"${pattern}\" and winName contains \"claude\" then
            activate
            set frontmost of w to true
            delay 0.15
            tell application \"System Events\"
                tell process \"Terminal\"
                    keystroke \"${escaped}\"
                    delay 0.05
                    key code 36
                end tell
            end tell
            delay 0.05
            tell application originalApp to activate
            return \"ok\"
        end if
    end repeat
    return \"no window\"
end tell" 2>/dev/null
}

while true; do
  for role in wren silas kade; do
    INBOX="$INBOX_ROOT/$role/pending-inject.txt"
    if [ -f "$INBOX" ]; then
      # Atomic rename to prevent race with writers
      DRAIN="$INBOX_ROOT/$role/draining-watcher-$$.txt"
      if mv "$INBOX" "$DRAIN" 2>/dev/null; then
        while IFS= read -r line; do
          [ -z "$line" ] && continue
          result=$(inject_to_role "$role" "$line")
          if [ "$result" = "ok" ]; then
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
