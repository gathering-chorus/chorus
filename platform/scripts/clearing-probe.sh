#!/bin/bash
# clearing-probe.sh — Synthetic monitor for Clearing message channel (#1933)
# Runs every 60s via LaunchAgent. Proves the full round-trip:
#   POST probe → GET messages → verify probe appears → emit spine event
# Alerts on failure so Jeff knows the channel is dead before he discovers it.
set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

CLEARING="http://localhost:3470"
CHORUS_LOG="${CHORUS_ROOT}/platform/scripts/chorus-log"
PROBE_LOG="/Users/jeffbridwell/Library/Logs/Chorus/clearing-probe.log"
MARKER="probe-$(date +%s)-$$"
MAX_WAIT=5
ROLE="system"

log() {
  echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" >> "$PROBE_LOG"
}

# Step 1: POST a probe message
HTTP_CODE=$(curl -s -o /dev/null -w '%{http_code}' \
  -X POST "$CLEARING/api/message" \
  -H 'Content-Type: application/json' \
  -d "{\"from\":\"probe\",\"text\":\"$MARKER\",\"type\":\"probe\"}" \
  --connect-timeout 3 --max-time 5 2>/dev/null || echo "000")

if [ "$HTTP_CODE" != "200" ]; then
  log "FAIL: POST returned $HTTP_CODE"
  "$CHORUS_LOG" clearing.probe.failed "$ROLE" "stage=post" "http=$HTTP_CODE" 2>/dev/null || true
  exit 1
fi

# Step 2: Verify the probe appears in message history
FOUND=false
for i in $(seq 1 $MAX_WAIT); do
  MESSAGES=$(curl -s "$CLEARING/api/messages?includeHidden=1" --connect-timeout 3 --max-time 5 2>/dev/null || echo "[]")
  if echo "$MESSAGES" | grep -q "$MARKER"; then
    FOUND=true
    break
  fi
  sleep 1
done

if $FOUND; then
  log "PASS: round-trip verified ($i s)"
  "$CHORUS_LOG" clearing.probe.passed "$ROLE" "latency=${i}s" 2>/dev/null || true
else
  log "FAIL: probe not found in messages after ${MAX_WAIT}s"
  "$CHORUS_LOG" clearing.probe.failed "$ROLE" "stage=verify" "timeout=${MAX_WAIT}s" 2>/dev/null || true
  exit 1
fi
