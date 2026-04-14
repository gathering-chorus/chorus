#!/usr/bin/env bash
# chrome-window.sh — Named Chrome windows per role (#1176)
#
# Simple design: one window per role, tracked by saved window ID.
# The WID file is the single source of truth. No URL fragments,
# no title injection, no multi-pass guessing.
#
# Usage:
#   chrome-window.sh <role> --focus       Bring role's window to front (don't create)
#   chrome-window.sh <role> [url]         Open URL in role's window (create if needed)
#   chrome-window.sh <role> --id          Print window ID
#   chrome-window.sh setup                Create windows for all roles

set -euo pipefail

ROLE="${1:-}"
ACTION="${2:-}"
ROLES=(wren silas kade)
DEFAULT_URL="http://localhost:3000"

die() { echo "ERROR: $*" >&2; exit 1; }

# --- Window ID persistence ---
WID_DIR="/tmp/chorus-chrome-windows"
mkdir -p "$WID_DIR"

save_wid() { printf '%s' "$2" > "$WID_DIR/$1.wid"; }
load_wid() { [ -f "$WID_DIR/$1.wid" ] && tr -d '[:space:]' < "$WID_DIR/$1.wid" || true; }

# Check if a window ID still exists in Chrome
wid_exists() {
  local wid="$1"
  [ -z "$wid" ] && return 1
  osascript -e "
    tell application \"Google Chrome\"
      repeat with w in every window
        if (id of w as text) is \"$wid\" then return true
      end repeat
      return false
    end tell
  " 2>/dev/null | grep -q "true"
}

# --- Core operations ---

# Find the role's window. Returns window ID or empty string.
# Only checks saved WID file — that's the source of truth.
find_window() {
  local role="$1"
  local wid
  wid=$(load_wid "$role")
  if [ -n "$wid" ] && wid_exists "$wid"; then
    echo "$wid"
  fi
}

# Create a new window for the role. Saves the ID.
create_window() {
  local role="$1"
  local url="${2:-$DEFAULT_URL}"
  local wid
  wid=$(osascript -e "
    tell application \"Google Chrome\"
      set newWindow to make new window
      set URL of active tab of newWindow to \"$url\"
      delay 0.5
      return id of newWindow
    end tell
  " 2>/dev/null)
  save_wid "$role" "$wid"
  echo "$wid"
}

# Focus (bring to front) a role's existing window. Never creates.
focus_window() {
  local role="$1"
  local wid
  wid=$(find_window "$role")
  [ -z "$wid" ] && return 0
  osascript -e "
    tell application \"Google Chrome\"
      repeat with w in every window
        if (id of w as text) is \"$wid\" then
          set index of w to 1
          activate
          return id of w
        end if
      end repeat
    end tell
  " 2>/dev/null
  echo "$wid"
}

# Navigate a role's window to a URL. Opens a new tab in the role's window.
# Creates window if none exists.
navigate_window() {
  local role="$1"
  local url="$2"
  local wid
  wid=$(find_window "$role")
  if [ -z "$wid" ]; then
    create_window "$role" "$url"
    return
  fi
  # Check if active tab is the default blank page — reuse it instead of opening new tab
  # Save and restore frontmost app to prevent focus theft (#2045)
  osascript -e "
    tell application \"System Events\"
      set frontApp to name of first application process whose frontmost is true
    end tell
    tell application \"Google Chrome\"
      repeat with w in every window
        if (id of w as text) is \"$wid\" then
          set currentURL to URL of active tab of w
          if currentURL is \"chrome://newtab/\" or currentURL is \"$url\" then
            set URL of active tab of w to \"$url\"
          else
            tell w to make new tab with properties {URL:\"$url\"}
          end if
        end if
      end repeat
    end tell
    delay 0.1
    tell application frontApp to activate
  " 2>/dev/null
  echo "$wid"
}

# --- Main ---

case "$ROLE" in
  setup)
    for r in "${ROLES[@]}"; do
      existing=$(find_window "$r")
      if [ -z "$existing" ]; then
        create_window "$r" "$DEFAULT_URL" >/dev/null
        echo "  $r: created"
      else
        echo "  $r: exists ($existing)"
      fi
    done
    ;;
  wren|silas|kade)
    case "$ACTION" in
      --focus)
        focus_window "$ROLE"
        ;;
      --id)
        wid=$(find_window "$ROLE")
        if [ -z "$wid" ]; then
          wid=$(create_window "$ROLE" "$DEFAULT_URL")
          echo "Created window for $ROLE" >&2
        fi
        echo "$wid"
        ;;
      ""|http://*|https://*|file://*)
        url="${ACTION:-$DEFAULT_URL}"
        navigate_window "$ROLE" "$url"
        ;;
      *)
        die "Unknown action: $ACTION. Use --focus, --id, or a URL."
        ;;
    esac
    ;;
  help|--help|-h)
    echo "Usage: chrome-window.sh <role> [--focus|--id|url]"
    echo "Roles: wren, silas, kade, setup"
    ;;
  *)
    die "Unknown role: $ROLE. Valid: wren, silas, kade, setup, help"
    ;;
esac
