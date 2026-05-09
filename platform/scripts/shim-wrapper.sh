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
# Resolve chorus-hook-shim via PATH (#2734) — ~/.chorus/bin/ is the canonical
# deploy location; target/release/ is the build artifact and a fallback for
# pre-#2734 systems where the install path hasn't run yet.
SHIM="$(command -v chorus-hook-shim 2>/dev/null || true)"
if [ -z "$SHIM" ] || [ ! -x "$SHIM" ]; then
  SHIM="$HOME/.chorus/bin/chorus-hook-shim"
fi
if [ ! -x "$SHIM" ]; then
  SHIM="${CHORUS_ROOT}/platform/services/chorus-hooks/target/release/chorus-hook-shim"
fi
CMD="$(basename "$0")"
LOGFILE="${HOME}/Library/Logs/Chorus/shim-wrapper.log"

log() {
  echo "$(date '+%Y-%m-%d %H:%M:%S') [${CMD}] $1" >> "$LOGFILE" 2>/dev/null
}

if [ ! -f "$SHIM" ]; then
  log "FATAL: shim binary not found at ${SHIM}"
  echo "${CMD}: chorus-hook-shim not found" >&2
  echo "  Path: ${SHIM}" >&2
  echo "  Fix: bash ${CHORUS_ROOT}/platform/scripts/build-signed.sh chorus-hooks" >&2
  exit 1
fi

if [ ! -x "$SHIM" ]; then
  log "FATAL: shim binary not executable at ${SHIM}"
  echo "${CMD}: chorus-hook-shim not executable" >&2
  echo "  Fix: chmod +x ${SHIM}" >&2
  exit 1
fi

# Trace hop bridge for role-state transitions (#2105, ADR-024)
if [ "$CMD" = "role-state" ] && [ -n "$1" ] && [ -n "$2" ]; then
  ROLE="$1"
  STATE="$2"
  CARD_ID=$(echo "$*" | grep -oE 'card=[0-9]+' | head -1 | sed 's/card=//')
  TRACE_ID="state-${ROLE}-${CARD_ID:-none}-$(date +%s)"
  curl -s -X POST http://localhost:3340/api/chorus/trace \
    -H 'Content-Type: application/json' \
    -d "{\"correlationId\":\"${TRACE_ID}\",\"hop\":1,\"callStack\":\"integration\",\"source\":{\"domain\":\"chorus\",\"service\":\"role-state\",\"instance\":\"${ROLE}\"},\"destination\":{\"domain\":\"chorus\",\"service\":\"${STATE}\"}}" \
    --max-time 3 > /dev/null 2>&1 &
fi

# #2857 — trace_id + card_id env-bridge for chorus-log. When invoked as
# chorus-log AND CHORUS_TRACE_ID / CHORUS_CARD_ID are set in env, prepend
# them to argv as kv pairs so the Rust shim emits them on the spine event.
# card_id is conditional on the event prefix per #2838 MUST-carry list:
# card.*, gate.*, demo.*, build.*, deploy.*, chorus_*, flow.*. Other events
# (library.health.*, canonical.sync.*, nudge.*, etc.) leave card_id unset
# even if env present — MUST-NOT contract enforced structurally at injection.
if [ "$CMD" = "chorus-log" ] && [ -n "${1:-}" ]; then
  EVENT_NAME="$1"; shift
  ROLE_ARG="${1:-}"; [ -n "$ROLE_ARG" ] && shift
  EXTRA_KV=()
  if [ -n "${CHORUS_TRACE_ID:-}" ]; then
    EXTRA_KV+=("trace_id=${CHORUS_TRACE_ID}")
  fi
  if [ -n "${CHORUS_CARD_ID:-}" ]; then
    case "$EVENT_NAME" in
      card.*|gate.*|demo.*|build.*|deploy.*|chorus_*|flow.*)
        EXTRA_KV+=("card_id=${CHORUS_CARD_ID}")
        ;;
    esac
  fi
  if [ -n "$ROLE_ARG" ]; then
    exec "$SHIM" "$CMD" "$EVENT_NAME" "$ROLE_ARG" "${EXTRA_KV[@]}" "$@"
  else
    exec "$SHIM" "$CMD" "$EVENT_NAME" "${EXTRA_KV[@]}" "$@"
  fi
fi

exec "$SHIM" "$CMD" "$@"
