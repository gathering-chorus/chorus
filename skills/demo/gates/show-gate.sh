#!/usr/bin/env bash
# /demo show-gate — owned by /demo product (Wren). #2864.
#
# Invoked by accept_gate.rs at /acp time (PreToolUse on chorus_acp).
# Validates that the *show* happened: card.demo.started fired, smoke passed,
# AND a Jeff-attention-signal landed (jeff.input.delivered spine event in
# the window AFTER card.demo.started).
#
# Outcome:
#   - All preconditions present → emit demo.show.completed, exit 0 (allow /acp)
#   - Any missing                → emit demo.show.failed, exit 1 (block /acp)
#
# Contract: stderr = refusal message. Exit 0 = allow, exit 1 = deny.
# Architecture note: previously registered as PostToolUse on /demo skill
# but that fires before card.demo.started lands on spine. Moved to /acp time.

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
CHORUS_LOG="$CHORUS_ROOT/platform/scripts/chorus-log"
LOKI_URL="${LOKI_URL:-http://localhost:3102}"

CARD_ID="${1:-}"
ROLE="${2:-system}"
[ -z "$CARD_ID" ] && exit 0
# #3030: card 99998 is the synthetic test card — it never has a real
# card.demo.started, so it generated ~70% of the demo.show.failed/no_demo_started
# noise that topped the pain board (a phantom: non-blocking, consumed by nothing).
# Keep test traffic out of prod observability; allow without emitting.
[ "$CARD_ID" = "99998" ] && exit 0

"$CHORUS_LOG" demo.show.started "$ROLE" card="$CARD_ID" 2>/dev/null || true

emit_completed() {
  "$CHORUS_LOG" demo.show.completed "$ROLE" card="$CARD_ID" jeff_input_count="$1" 2>/dev/null || true
  exit 0  # PreToolUse: allow /acp
}

emit_failed() {
  "$CHORUS_LOG" demo.show.failed "$ROLE" card="$CARD_ID" reason="$1" 2>/dev/null || true
  echo "demo-show: failed for #${CARD_ID} — $1" >&2
  exit 1  # PreToolUse contract — block /acp on missing precondition
}

# Step 1: find the most recent card.demo.started for this card_id (last 1h).
DEMO_STARTED_TS=$(curl -sf -G "$LOKI_URL/loki/api/v1/query_range" \
  --data-urlencode "query={job=\"chorus-api\"} |~ \"card.demo.started\" |~ \"\\\"card_id\\\":${CARD_ID}\"" \
  --data-urlencode "start=$(( $(date +%s) - 3600 ))000000000" \
  --data-urlencode "limit=10" 2>/dev/null | python3 -c "
import json, sys
try:
    d = json.load(sys.stdin)
    timestamps = []
    for s in d.get('data', {}).get('result', []):
        for ts, _ in s.get('values', []):
            timestamps.append(int(ts))
    print(max(timestamps) if timestamps else 0)
except Exception:
    print(0)
" 2>/dev/null || echo 0)

if [ -z "$DEMO_STARTED_TS" ] || [ "$DEMO_STARTED_TS" = "0" ]; then
  emit_failed "no_demo_started"
fi

# Step 2: Jeff-attention-signal — jeff.input.delivered in 30s..10min window
# AFTER card.demo.started. Window starts +30s to rule out same-instant noise.
WINDOW_START=$(( DEMO_STARTED_TS + 30000000000 ))    # +30s in ns
WINDOW_END=$(( DEMO_STARTED_TS + 600000000000 ))     # +10min in ns

JEFF_INPUT_COUNT=$(curl -sf -G "$LOKI_URL/loki/api/v1/query_range" \
  --data-urlencode 'query={job=~".+"} |~ "jeff.input.delivered"' \
  --data-urlencode "start=${WINDOW_START}" \
  --data-urlencode "end=${WINDOW_END}" \
  --data-urlencode "limit=50" 2>/dev/null | python3 -c "
import json, sys
try:
    d = json.load(sys.stdin)
    n = sum(len(s.get('values', [])) for s in d.get('data', {}).get('result', []))
    print(n)
except Exception:
    print(0)
" 2>/dev/null || echo 0)

if [ -z "$JEFF_INPUT_COUNT" ] || [ "$JEFF_INPUT_COUNT" = "0" ]; then
  emit_failed "jeff_not_watching"
fi

# Step 3: smoke check evidence — demo.preflight.passed event for this card.
SMOKE_OK=$(curl -sf -G "$LOKI_URL/loki/api/v1/query_range" \
  --data-urlencode "query={job=\"chorus-api\"} |~ \"demo.preflight.passed\" |~ \"card=${CARD_ID}\"" \
  --data-urlencode "start=$(( $(date +%s) - 3600 ))000000000" \
  --data-urlencode "limit=5" 2>/dev/null | python3 -c "
import json, sys
try:
    d = json.load(sys.stdin)
    n = sum(len(s.get('values', [])) for s in d.get('data', {}).get('result', []))
    print(n)
except Exception:
    print(0)
" 2>/dev/null || echo 0)

if [ -z "$SMOKE_OK" ] || [ "$SMOKE_OK" = "0" ]; then
  emit_failed "no_preflight_passed"
fi

emit_completed "$JEFF_INPUT_COUNT"
