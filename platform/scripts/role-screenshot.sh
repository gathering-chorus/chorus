#!/bin/bash
# Capture a role's terminal screen without stealing focus
# Usage: role-screenshot.sh <role>
# Output: path to screenshot PNG

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

ROLE="${1:?Usage: role-screenshot.sh <role>}"
OUTDIR="${CHORUS_ROOT}/proving/screenshots"
mkdir -p "$OUTDIR"

# Map role to working directory
case "$ROLE" in
  wren)  CWD_MATCH="roles/wren" ;;
  silas) CWD_MATCH="roles/silas" ;;
  kade)  CWD_MATCH="roles/kade" ;;
  *)     echo "Unknown role: $ROLE" >&2; exit 1 ;;
esac

# Find the claude process for this role by cwd
TTY=""
while IFS= read -r line; do
  PID=$(echo "$line" | awk '{print $1}')
  T=$(echo "$line" | awk '{print $2}')
  [ "$T" = "??" ] && continue
  CWD=$(/usr/sbin/lsof -p "$PID" -a -d cwd -Fn 2>/dev/null | grep ^n | cut -c2- || true)
  if echo "$CWD" | grep -q "$CWD_MATCH"; then
    TTY="/dev/$T"
    break
  fi
done < <(ps -eo pid,tty,comm 2>/dev/null | grep claude | grep -v grep)

if [ -z "$TTY" ]; then
  echo "No active session for $ROLE" >&2
  exit 1
fi

# Find the Terminal window containing this TTY
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
OUTFILE="$OUTDIR/${ROLE}-${TIMESTAMP}.png"

# Use osascript to get the window ID for this TTY, then screencapture by window ID
WINDOW_ID=$(osascript -e "
tell application \"Terminal\"
  repeat with w in windows
    repeat with t in tabs of w
      if tty of t is \"$TTY\" then
        return id of w
      end if
    end repeat
  end repeat
  return \"\"
end tell
" 2>/dev/null)

if [ -z "$WINDOW_ID" ]; then
  echo "Could not find Terminal window for $ROLE (TTY: $TTY)" >&2
  exit 1
fi

# Capture the window without bringing it to front
screencapture -l "$WINDOW_ID" -x "$OUTFILE" 2>/dev/null

if [ -f "$OUTFILE" ]; then
  echo "$OUTFILE"
else
  echo "Screenshot failed" >&2
  exit 1
fi
