#!/usr/bin/env bash
# look.sh — Screen capture for sensory bridge
# Uses chorus-capture (ScreenCaptureKit) which works from any context
# including Claude Code's Bash tool. No TCC issues.
# Usage: look.sh [--role <role>] [screen|chrome|terminal|<filepath>]
# With --role, captures only that role's named Chrome window (#1176).

set -euo pipefail

CAPTURE_DIR="/tmp/chorus-look"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CAPTURER="$SCRIPT_DIR/chorus-capture"
mkdir -p "$CAPTURE_DIR"

TIMESTAMP=$(date +%Y%m%dT%H%M%S)
ROLE=""

# Parse --role flag
if [ "${1:-}" = "--role" ]; then
  ROLE="${2:-}"
  shift 2
fi

TARGET="${1:-screen}"

case "$TARGET" in
    screen)
        OUTFILE="$CAPTURE_DIR/screen-$TIMESTAMP.png"
        "$CAPTURER" "$OUTFILE"
        ;;
    chrome)
        OUTFILE="$CAPTURE_DIR/chrome-$TIMESTAMP.png"
        if [ -n "$ROLE" ]; then
            # Capture the role's own window — focus it first
            CHROME_WINDOW="$SCRIPT_DIR/chrome-window.sh"
            if [ -x "$CHROME_WINDOW" ]; then
                bash "$CHROME_WINDOW" "$ROLE" --focus >/dev/null 2>&1
                sleep 0.5
            fi
        fi
        # Capture Chrome's frontmost window by CGWindowID
        CGWID=$(swift -e '
          import Cocoa
          let opts = CGWindowListOption(arrayLiteral: .optionOnScreenOnly, .excludeDesktopElements)
          guard let wl = CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as? [[String: Any]] else { exit(1) }
          for w in wl {
            let o = w[kCGWindowOwnerName as String] as? String ?? ""
            if o.contains("Chrome") && (w[kCGWindowLayer as String] as? Int ?? -1) == 0 {
              print(w[kCGWindowNumber as String] as? Int ?? 0)
              break
            }
          }
        ' 2>/dev/null)
        if [ -n "$CGWID" ] && [ "$CGWID" != "0" ]; then
            screencapture -o -x -l"$CGWID" "$OUTFILE"
        else
            "$CAPTURER" "$OUTFILE" chrome
        fi
        ;;
    terminal)
        OUTFILE="$CAPTURE_DIR/terminal-$TIMESTAMP.png"
        "$CAPTURER" "$OUTFILE" terminal
        ;;
    *)
        # Treat as a file path
        if [ -f "$TARGET" ]; then
            OUTFILE="$TARGET"
        else
            echo "ERROR: Unknown target '$TARGET'. Use: screen, chrome, terminal, or a file path." >&2
            exit 1
        fi
        ;;
esac

# Verify capture succeeded
if [ ! -f "$OUTFILE" ] || [ ! -s "$OUTFILE" ]; then
    echo "ERROR: Capture failed — no file produced" >&2
    exit 1
fi

# Symlink as latest for easy access
ln -sf "$OUTFILE" "$CAPTURE_DIR/latest.png"

# Clean up old captures (keep last 20)
ls -t "$CAPTURE_DIR"/screen-*.png "$CAPTURE_DIR"/chrome-*.png "$CAPTURE_DIR"/terminal-*.png 2>/dev/null | tail -n +21 | xargs rm -f 2>/dev/null || true

echo "$OUTFILE"
