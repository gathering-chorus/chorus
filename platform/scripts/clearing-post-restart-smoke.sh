#!/bin/bash
# clearing-post-restart-smoke.sh — Post-restart shape assertion for /api/flow (#2333).
#
# Locks the shape promise from #2325: `sequences: string[]` on every flow card.
# Fires once on Clearing LaunchAgent kickstart (RunAtLoad). Fails loudly if the
# shape regresses so Jeff doesn't open the Clearing to an empty board.
#
# Validator logic lives in clearing-flow-shape-validator.py — this script is
# the curl-and-pipe wrapper plus the /health gate.
set -euo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/chorus-env-setup.sh"

CLEARING="${CLEARING_URL:-http://localhost:3470}"
CHORUS_LOG="${CHORUS_LOG_BIN:-${CHORUS_ROOT}/platform/scripts/chorus-log}"
SMOKE_LOG="${SMOKE_LOG:-/Users/jeffbridwell/Library/Logs/Chorus/clearing-post-restart-smoke.log}"
HEALTH_TIMEOUT="${HEALTH_TIMEOUT:-10}"
ROLE="system"
VALIDATOR="$(dirname "${BASH_SOURCE[0]}")/clearing-flow-shape-validator.py"

log() {
  echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" >> "$SMOKE_LOG" 2>/dev/null || true
}

fail() {
  local stage="$1" detail="$2"
  log "FAIL: stage=$stage $detail"
  "$CHORUS_LOG" clearing.smoke.failed "$ROLE" "stage=$stage" "$detail" 2>/dev/null || true
  exit 1
}

HEALTH=000
for i in $(seq 1 "$HEALTH_TIMEOUT"); do
  HEALTH=$(curl -s -o /dev/null -w '%{http_code}' "$CLEARING/health" --connect-timeout 2 --max-time 3 2>/dev/null || echo 000)
  if [ "$HEALTH" = "200" ]; then
    break
  fi
  sleep 1
done
[ "$HEALTH" = "200" ] || fail "health" "http=$HEALTH timeout=${HEALTH_TIMEOUT}s"

FLOW=$(curl -s -w $'\n%{http_code}' "$CLEARING/api/flow" --connect-timeout 3 --max-time 5 2>/dev/null || printf '\n000')
FLOW_CODE=$(printf '%s' "$FLOW" | tail -1)
FLOW_BODY=$(printf '%s' "$FLOW" | sed '$d')
[ "$FLOW_CODE" = "200" ] || fail "flow_http" "http=$FLOW_CODE"

printf '%s' "$FLOW_BODY" | CHORUS_LOG="$CHORUS_LOG" ROLE="$ROLE" python3 "$VALIDATOR" || exit 1

log "PASS"
