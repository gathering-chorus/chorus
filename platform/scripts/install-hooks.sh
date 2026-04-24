#!/usr/bin/env bash
# install-hooks.sh — symlink tracked hooks from platform/hooks/ into .git/hooks/ (#2465)
# Idempotent: safe to re-run; backs up any pre-existing non-symlink hook once.
# Auto-invoked by git-queue.sh if the installed hook is missing or stale.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SRC_DIR="$REPO_ROOT/platform/hooks"
DEST_DIR="$REPO_ROOT/.git/hooks"

if [ ! -d "$SRC_DIR" ]; then
  echo "install-hooks: no $SRC_DIR — nothing to install" >&2
  exit 0
fi

if [ ! -d "$DEST_DIR" ]; then
  echo "install-hooks: $DEST_DIR missing (not a git repo?)" >&2
  exit 1
fi

installed=0
skipped=0

for src in "$SRC_DIR"/*; do
  [ -f "$src" ] || continue
  name=$(basename "$src")
  dest="$DEST_DIR/$name"

  # Already a symlink pointing at our source? Skip.
  if [ -L "$dest" ] && [ "$(readlink "$dest")" = "$src" ]; then
    skipped=$((skipped + 1))
    continue
  fi

  # Existing non-symlink file? Back it up once.
  if [ -e "$dest" ] && [ ! -L "$dest" ]; then
    backup="$dest.preinstall-$(date +%Y%m%d-%H%M%S)"
    mv "$dest" "$backup"
    echo "install-hooks: backed up existing $name to $(basename "$backup")" >&2
  elif [ -L "$dest" ]; then
    # Stale symlink — remove
    rm "$dest"
  fi

  ln -s "$src" "$dest"
  chmod +x "$src"
  installed=$((installed + 1))
done

echo "install-hooks: $installed installed, $skipped already current"
