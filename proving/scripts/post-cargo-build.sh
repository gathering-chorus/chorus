#!/usr/bin/env bash
# post-cargo-build.sh — verify Accessibility permission after chorus-hooks rebuild
#
# macOS invalidates Accessibility permission when a binary's hash changes.
# This script tests osascript inject and warns if it fails.
# Run after: cargo build --release (in chorus-hooks)

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

SHIM="${CHORUS_ROOT}/platform/services/chorus-hooks/target/release/chorus-hook-shim"

if [[ ! -x "$SHIM" ]]; then
  echo "ERROR: chorus-hook-shim not found at $SHIM"
  exit 1
fi

# Test osascript keystroke permission (empty keystroke to self)
if osascript -e 'tell application "System Events" to keystroke ""' 2>/dev/null; then
  echo "✓ Accessibility permission OK — osascript can send keystrokes"
else
  echo ""
  echo "⚠  Accessibility permission BROKEN after rebuild"
  echo ""
  echo "   cargo build changed the binary hash → macOS revoked permission."
  echo ""
  echo "   Fix: System Settings → Privacy & Security → Accessibility"
  echo "         Toggle chorus-hook-shim OFF then ON"
  echo ""
  echo "   Opening the pane now..."
  open "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
  exit 1
fi
