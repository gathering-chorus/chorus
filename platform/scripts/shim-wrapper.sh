#!/usr/bin/env bash
# shim-wrapper.sh — resilient wrapper for chorus-hook-shim symlinks (#2034)
# Detects missing/broken binary and emits useful error instead of silent failure.
# Usage: symlink this from any script name (chorus-log, role-state, etc.)
#        The shim dispatches by first argument when invoked as subcommand.
#
# 17 scripts symlink here: chorus-log, role-state, wall-clock, heartbeat,
# context-cache-{5min,hourly,daily,weekly}, cruft-scan, log-rotate,
# session-{start,end,close}-thin, role-checkpoint, workflow, claudemd-gen,
# chorus-init-db.

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
SHIM="${CHORUS_ROOT}/platform/services/chorus-hooks/target/release/chorus-hook-shim"
CMD="$(basename "$0")"
LOGFILE="${HOME}/Library/Logs/Chorus/shim-wrapper.log"

log() {
  echo "$(date '+%Y-%m-%d %H:%M:%S') [${CMD}] $1" >> "$LOGFILE" 2>/dev/null
}

if [ ! -f "$SHIM" ]; then
  log "FATAL: shim binary not found at ${SHIM}"
  echo "${CMD}: chorus-hook-shim not found" >&2
  echo "  Path: ${SHIM}" >&2
  echo "  Fix: cd ${CHORUS_ROOT}/platform/services/chorus-hooks && cargo build --release --bin chorus-hook-shim" >&2
  exit 1
fi

if [ ! -x "$SHIM" ]; then
  log "FATAL: shim binary not executable at ${SHIM}"
  echo "${CMD}: chorus-hook-shim not executable" >&2
  echo "  Fix: chmod +x ${SHIM}" >&2
  exit 1
fi

exec "$SHIM" "$CMD" "$@"
