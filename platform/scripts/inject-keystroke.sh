#!/usr/bin/env bash
# inject-keystroke.sh — stable keystroke injection that survives Rust rebuilds
# TCC grants permission to Terminal.app, not to the binary calling osascript.
# This script calls osascript directly from bash — no compiled binary in the path.
set -euo pipefail

ROLE="$1"
MSG="$2"

# Map role to terminal tab name
case "$ROLE" in
  wren) TAB_PATTERN="wren\|product-manager" ;;
  silas) TAB_PATTERN="silas\|architect" ;;
  kade) TAB_PATTERN="kade\|engineer" ;;
  *) echo "Unknown role: $ROLE" >&2; exit 1 ;;
esac

# Escape message for AppleScript
ESCAPED=$(echo "$MSG" | sed "s/'/'\\\\''/g")

# Inject via osascript — sends keystroke to role's terminal tab
osascript -e "
tell application \"Terminal\"
  set targetWindow to missing value
  set targetTab to missing value
  repeat with w in windows
    repeat with t in tabs of w
      set tabName to custom title of t
      if tabName contains \"$ROLE\" then
        set targetWindow to w
        set targetTab to t
        exit repeat
      end if
    end repeat
    if targetTab is not missing value then exit repeat
  end repeat
  if targetTab is not missing value then
    set selected tab of targetWindow to targetTab
    set frontmost of targetWindow to true
    tell application \"System Events\"
      keystroke \"$ESCAPED\"
      keystroke return
    end tell
  end if
end tell
" 2>&1
