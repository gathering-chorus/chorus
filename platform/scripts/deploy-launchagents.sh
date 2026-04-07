#!/bin/bash
# deploy-launchagents.sh — Copy canonical plists to ~/Library/LaunchAgents/
# Source of truth: chorus/proving/config/launchagents/
# Run after editing plists in the repo.

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects}"

CANONICAL="${CHORUS_ROOT}/proving/config/launchagents"
TARGET="$HOME/Library/LaunchAgents"

if [ ! -d "$CANONICAL" ]; then
  echo "ERROR: Canonical source not found: $CANONICAL" >&2
  exit 1
fi

changed=0
skipped=0

for src in "$CANONICAL"/*.plist; do
  name="$(basename "$src")"
  dest="$TARGET/$name"

  if [ -f "$dest" ] && diff -q "$src" "$dest" >/dev/null 2>&1; then
    skipped=$((skipped + 1))
    continue
  fi

  cp "$src" "$dest"
  echo "  deployed: $name"
  changed=$((changed + 1))
done

echo "Deploy complete: $changed updated, $skipped unchanged"

if [ "$changed" -gt 0 ]; then
  echo "NOTE: Run 'launchctl kickstart -k gui/\$(id -u)/<label>' to restart affected agents"
fi
