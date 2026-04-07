#!/usr/bin/env bash
# standards-surface-cron.sh — Nightly auto-regeneration of standards surface
# Card #2268: Runs via LaunchAgent, only regenerates when sources changed.
#
# Source change detection: compares sha256 of inputs against last-run checksums.
# If nothing changed, skips regeneration (idempotent, no wasted work).
set -euo pipefail
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects}"

export PATH="/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GEN_SCRIPT="$SCRIPT_DIR/generate-standards-surface.sh"
STATE_FILE="$HOME/.chorus/standards-surface-checksums.json"
LOG_TAG="standards-surface-cron"

# Sources to monitor for changes
DECISIONS_MD="${CHORUS_ROOT}/platform/roles/product-manager/decisions.md"
HOOKS_DIR="${CHORUS_ROOT}/platform/services/chorus-hooks/src/hooks"
PULSE_LOG="$HOME/Library/Logs/Gathering/hooks.log"
MEMORY_DIR="$HOME/.claude/projects/-Users-jeffbridwell-CascadeProjects/memory"

# Chorus log helper
CHORUS_LOG="${CHORUS_ROOT}/platform/scripts/chorus-log"

log() { echo "$(TZ=America/New_York date '+%Y-%m-%d %H:%M:%S') [$LOG_TAG] $*"; }

# --- Compute current source checksums ---
compute_checksums() {
  local decisions_hash="" hooks_hash="" pulse_hash="" memory_hash=""

  if [ -f "$DECISIONS_MD" ]; then
    decisions_hash=$(shasum -a 256 "$DECISIONS_MD" | cut -d' ' -f1)
  fi

  if [ -d "$HOOKS_DIR" ]; then
    hooks_hash=$(find "$HOOKS_DIR" -name '*.rs' -not -name 'mod.rs' -exec shasum -a 256 {} + 2>/dev/null | sort | shasum -a 256 | cut -d' ' -f1)
  fi

  # For pulse log, use size + mtime — hashing 62K lines is wasteful
  if [ -f "$PULSE_LOG" ]; then
    pulse_hash="$(stat -f '%z_%m' "$PULSE_LOG" 2>/dev/null)"
  fi

  # Memory files: count + newest mtime
  if [ -d "$MEMORY_DIR" ]; then
    memory_hash="$(ls "$MEMORY_DIR"/feedback_*.md "$MEMORY_DIR"/story_*.md 2>/dev/null | wc -l | tr -d ' ')_$(stat -f '%m' "$MEMORY_DIR"/feedback_*.md "$MEMORY_DIR"/story_*.md 2>/dev/null | sort -rn | head -1)"
  fi

  echo "{\"decisions\":\"$decisions_hash\",\"hooks\":\"$hooks_hash\",\"pulse\":\"$pulse_hash\",\"memory\":\"$memory_hash\"}"
}

# --- Check if sources changed ---
sources_changed() {
  local current="$1"

  if [ ! -f "$STATE_FILE" ]; then
    log "No previous checksums — first run"
    return 0
  fi

  local previous
  previous=$(cat "$STATE_FILE" 2>/dev/null || echo "{}")

  if [ "$current" = "$previous" ]; then
    return 1
  fi
  return 0
}

# --- Main ---
main() {
  local dry_run=false
  local force=false

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --dry-run) dry_run=true; shift ;;
      --force) force=true; shift ;;
      *) echo "Usage: standards-surface-cron.sh [--dry-run] [--force]" >&2; exit 1 ;;
    esac
  done

  log "Starting standards surface check"

  local current_checksums
  current_checksums=$(compute_checksums)

  if [ "$force" = true ]; then
    log "Forced regeneration"
  elif ! sources_changed "$current_checksums"; then
    log "No source changes detected — skipping regeneration"
    [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" standards.surface.skipped kade reason=no_changes 2>/dev/null &
    exit 0
  fi

  if [ "$dry_run" = true ]; then
    log "Dry run — would regenerate. Changes detected:"
    log "  Current: $current_checksums"
    [ -f "$STATE_FILE" ] && log "  Previous: $(cat "$STATE_FILE")"
    exit 0
  fi

  # Regenerate
  log "Sources changed — regenerating standards surface"
  if bash "$GEN_SCRIPT" 2>&1; then
    log "Regeneration complete"
    mkdir -p "$(dirname "$STATE_FILE")"
    echo "$current_checksums" > "$STATE_FILE"
    [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" standards.surface.regenerated kade 2>/dev/null &
  else
    log "ERROR: Regeneration failed"
    [ -x "$CHORUS_LOG" ] && "$CHORUS_LOG" standards.surface.failed kade 2>/dev/null &
    exit 1
  fi
}

main "$@"
