#!/usr/bin/env bash
# demo-scroll.sh — Programmatic Chrome scrolling for /demo walkthroughs
# Card #1108 (original), #1176 (named window support). Silas owns.
#
# Usage:
#   demo-scroll.sh [--role <role>] down [N]        Scroll down N sections (default 1)
#   demo-scroll.sh [--role <role>] up [N]          Scroll up N sections (default 1)
#   demo-scroll.sh [--role <role>] top             Scroll to top of page
#   demo-scroll.sh [--role <role>] bottom          Scroll to bottom of page
#   demo-scroll.sh [--role <role>] section "text"  Scroll to element containing text
#   demo-scroll.sh [--role <role>] smooth [px]     Smooth scroll by pixel amount (default 500)
#
# --role targets the named Chrome window for that role (via chrome-window.sh).
# Without --role, falls back to front window (legacy behavior).
#
# Uses System Events key codes — no Chrome JS permission needed for basic scrolling.
# Named section scroll requires: Chrome > View > Developer > Allow JavaScript from Apple Events.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROLE=""

# Parse --role flag
if [ "${1:-}" = "--role" ]; then
  ROLE="${2:-}"
  shift 2
fi

CMD="${1:-down}"
ARG="${2:-1}"

# Window targeting: find the role's named window or fall back to front window
if [ -n "$ROLE" ]; then
  FRAGMENT="#role=${ROLE}"
  WINDOW_REF_SCRIPT="
    set targetWindow to missing value
    set wList to every window
    repeat with w in wList
      set tabList to every tab of w
      repeat with i from 1 to count of tabList
        try
          set u to URL of item i of tabList
          if u contains \"$FRAGMENT\" then
            set active tab index of w to i
            set targetWindow to w
            exit repeat
          end if
        end try
      end repeat
      if targetWindow is not missing value then exit repeat
    end repeat
    if targetWindow is missing value then
      set targetWindow to front window
    end if
  "
else
  WINDOW_REF_SCRIPT="set targetWindow to front window"
fi

# Focus the role's window (or Chrome generally)
focus_chrome() {
  if [ -n "$ROLE" ]; then
    "$SCRIPT_DIR/chrome-window.sh" "$ROLE" --focus >/dev/null 2>&1 || \
      osascript -e 'tell application "Google Chrome" to activate' 2>/dev/null
  else
    osascript -e 'tell application "Google Chrome" to activate' 2>/dev/null
  fi
  sleep 0.2
}

scroll_down() {
  local count="${1:-1}"
  focus_chrome
  for ((i=0; i<count; i++)); do
    osascript -e 'tell application "System Events" to key code 125 using {option down}' 2>/dev/null
    sleep 0.4
  done
}

scroll_up() {
  local count="${1:-1}"
  focus_chrome
  for ((i=0; i<count; i++)); do
    osascript -e 'tell application "System Events" to key code 126 using {option down}' 2>/dev/null
    sleep 0.4
  done
}

scroll_top() {
  focus_chrome
  osascript -e 'tell application "System Events" to key code 126 using {command down}' 2>/dev/null
}

scroll_bottom() {
  focus_chrome
  osascript -e 'tell application "System Events" to key code 125 using {command down}' 2>/dev/null
}

scroll_to_section() {
  local text="$1"
  osascript -e "
    tell application \"Google Chrome\"
      $WINDOW_REF_SCRIPT
      set activeTab to active tab of targetWindow
      execute activeTab javascript \"
        (function() {
          var els = document.querySelectorAll('h1,h2,h3,h4,summary,th,td,a,p,span');
          var target = '${text}'.toLowerCase();
          for (var i = 0; i < els.length; i++) {
            if (els[i].textContent.toLowerCase().includes(target)) {
              els[i].scrollIntoView({behavior: 'smooth', block: 'start'});
              return 'found';
            }
          }
          return 'not found';
        })()
      \"
    end tell
  " 2>/dev/null
}

scroll_smooth() {
  local px="${1:-500}"
  osascript -e "
    tell application \"Google Chrome\"
      $WINDOW_REF_SCRIPT
      set activeTab to active tab of targetWindow
      execute activeTab javascript \"window.scrollBy({top: ${px}, behavior: 'smooth'})\"
    end tell
  " 2>/dev/null
}

case "$CMD" in
  down)    scroll_down "$ARG" ;;
  up)      scroll_up "$ARG" ;;
  top)     scroll_top ;;
  bottom)  scroll_bottom ;;
  section) scroll_to_section "$ARG" ;;
  smooth)  scroll_smooth "$ARG" ;;
  help|*)
    echo "demo-scroll.sh — Chrome scrolling for /demo walkthroughs"
    echo ""
    echo "Options:"
    echo "  --role <role>     Target named Chrome window (wren, silas, kade)"
    echo ""
    echo "Commands (no JS permission needed):"
    echo "  down [N]          Scroll down N sections (default 1)"
    echo "  up [N]            Scroll up N sections (default 1)"
    echo "  top               Scroll to top"
    echo "  bottom            Scroll to bottom"
    echo ""
    echo "Commands (requires Chrome JS permission):"
    echo "  section \"text\"    Scroll to element containing text"
    echo "  smooth [px]       Smooth scroll by pixels (default 500)"
    ;;
esac
