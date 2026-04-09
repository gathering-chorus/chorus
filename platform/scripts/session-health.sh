#!/usr/bin/env bash
# session-health.sh — Session degradation detection (#2271)
# Reports: prompt count, session age, tool call count, queue removes (compaction proxy)
# Alerts when session exceeds thresholds — gas gauge, not check engine light.
set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects}"

SESSIONS_DIR="$HOME/.claude/projects"
HOOKS_LOG="$HOME/Library/Logs/Gathering/hooks.log"
NUDGE="${CHORUS_ROOT}/platform/scripts/nudge"
CHORUS_LOG="${CHORUS_ROOT}/platform/scripts/chorus-log"

# Defaults
ROLE=""
PROMPT_THRESHOLD=400
HOUR_THRESHOLD=4
REMOVE_RATE_THRESHOLD=10  # removes per 50-prompt bucket — raised from 5, boot-heavy sessions inflate early
ALERT=false
TEST_MODE="${BATS_TEST_RUNNING:-${SESSION_HEALTH_TEST:-}}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --role) ROLE="$2"; shift 2 ;;
    --threshold) PROMPT_THRESHOLD="$2"; shift 2 ;;
    --hour-threshold) HOUR_THRESHOLD="$2"; shift 2 ;;
    --remove-rate-threshold) REMOVE_RATE_THRESHOLD="$2"; shift 2 ;;
    *) echo "Usage: session-health.sh --role <role> [--threshold N] [--hour-threshold H]" >&2; exit 1 ;;
  esac
done

if [ -z "$ROLE" ]; then
  echo "Error: --role required" >&2; exit 1
fi

# Map role to project directory
case "$ROLE" in
  silas) PROJECT_SUFFIX="chorus-silas" ;;
  wren) PROJECT_SUFFIX="chorus-wren" ;;
  kade) PROJECT_SUFFIX="chorus-kade" ;;
  *) echo "Unknown role: $ROLE" >&2; exit 1 ;;
esac

PROJECT_DIR=$(find "$SESSIONS_DIR" -maxdepth 1 -name "*$PROJECT_SUFFIX" -type d 2>/dev/null | head -1)
if [ -z "$PROJECT_DIR" ] || [ ! -d "$PROJECT_DIR" ]; then
  echo "No project directory found for $ROLE" >&2; exit 1
fi

# Find the most recent session JSONL
LATEST_SESSION=$(find "$PROJECT_DIR" -name "*.jsonl" -type f 2>/dev/null | xargs ls -t 2>/dev/null | head -1)
if [ -z "$LATEST_SESSION" ]; then
  echo "No session JSONL found for $ROLE" >&2; exit 1
fi

SESSION_ID=$(basename "$(dirname "$LATEST_SESSION")")
now=$(date +%s)

# Count prompts (user messages)
PROMPT_COUNT=$(grep -c '"type":"user"' "$LATEST_SESSION" 2>/dev/null || echo 0)

# Count assistant responses
RESPONSE_COUNT=$(grep -c '"type":"assistant"' "$LATEST_SESSION" 2>/dev/null || echo 0)

# Count queue-operation remove events (compaction = context window trimming)
QUEUE_REMOVES=$(python3 -c "
import json, sys
count = 0
for line in open(sys.argv[1]):
    try:
        d = json.loads(line)
        if d.get('type') == 'queue-operation' and d.get('operation') == 'remove':
            count += 1
    except: pass
print(count)
" "$LATEST_SESSION" 2>/dev/null || echo 0)

# Session age from first timestamp
FIRST_TS=$(head -1 "$LATEST_SESSION" | python3 -c "import sys,json; print(json.load(sys.stdin).get('timestamp',''))" 2>/dev/null || echo "")
if [ -n "$FIRST_TS" ]; then
  first_epoch=$(date -j -f "%Y-%m-%dT%H:%M:%S" "${FIRST_TS%%.*}" +%s 2>/dev/null || echo "$now")
  AGE_SEC=$((now - first_epoch))
  AGE_MIN=$((AGE_SEC / 60))
  AGE_HOURS=$((AGE_SEC / 3600))
else
  AGE_SEC=0; AGE_MIN=0; AGE_HOURS=0
fi

# Tool call count from hooks log (this session only)
TOOL_COUNT=0
if [ -f "$HOOKS_LOG" ]; then
  TOOL_COUNT=$(grep -c "$SESSION_ID.*allow" "$HOOKS_LOG" 2>/dev/null || true)
  TOOL_COUNT="${TOOL_COUNT:-0}"
fi

# Tool call rate (calls per minute)
if [ "$AGE_MIN" -gt 0 ]; then
  TOOL_RATE=$((TOOL_COUNT / AGE_MIN))
else
  TOOL_RATE=0
fi

# Compaction rate: removes in latest 50-prompt bucket vs first bucket
# Higher rate in recent buckets = context window under pressure
if [ "$PROMPT_COUNT" -gt 0 ]; then
  REMOVE_RATE=$(python3 -c "
import json, sys
prompts = 0; recent_removes = 0; total = int(sys.argv[2])
for line in open(sys.argv[1]):
    try:
        d = json.loads(line)
        if d.get('type') == 'user': prompts += 1
        if d.get('type') == 'queue-operation' and d.get('operation') == 'remove':
            bucket_start = max(0, total - 50)
            if prompts >= bucket_start:
                recent_removes += 1
    except: pass
print(recent_removes)
" "$LATEST_SESSION" "$PROMPT_COUNT" 2>/dev/null || echo 0)
else
  REMOVE_RATE=0
fi

# Output metrics
echo "session=$SESSION_ID role=$ROLE"
echo "prompts=$PROMPT_COUNT responses=$RESPONSE_COUNT tools=$TOOL_COUNT"
echo "age_min=$AGE_MIN age_hours=$AGE_HOURS tool_rate=${TOOL_RATE}/min"
echo "queue_removes=$QUEUE_REMOVES remove_rate=$REMOVE_RATE"

# Alert evaluation
ALERTS=""
if [ "$PROMPT_COUNT" -gt "$PROMPT_THRESHOLD" ]; then
  ALERTS="${ALERTS}Session is long (${PROMPT_COUNT} prompts, threshold ${PROMPT_THRESHOLD}). "
  ALERT=true
fi
if [ "$AGE_HOURS" -gt "$HOUR_THRESHOLD" ]; then
  ALERTS="${ALERTS}Session is old (${AGE_HOURS}h, threshold ${HOUR_THRESHOLD}h). "
  ALERT=true
fi
if [ "$REMOVE_RATE" -gt "$REMOVE_RATE_THRESHOLD" ]; then
  ALERTS="${ALERTS}Compaction accelerating (${REMOVE_RATE} removes in last 50 prompts, threshold ${REMOVE_RATE_THRESHOLD}). "
  ALERT=true
fi

# Dedup: only alert once per session — write marker file
ALERT_MARKER="/tmp/session-health-alerted-${ROLE}-${SESSION_ID}"

if [ "$ALERT" = true ]; then
  echo "WARN: ${ALERTS}Context pressure rising."
  # Suppress nudges during test runs or if already alerted this session
  if [ -z "$TEST_MODE" ] && [ ! -f "$ALERT_MARKER" ]; then
    "$NUDGE" "$ROLE" "session-health: ${ALERTS}Context pressure rising." --from system 2>/dev/null || true
    "$NUDGE" wren "session-health: ${ROLE} at ${PROMPT_COUNT} prompts, ${REMOVE_RATE} removes/50. Context pressure rising." --from system 2>/dev/null || true
    touch "$ALERT_MARKER"
  fi
  "$CHORUS_LOG" session.health.warning "$ROLE" prompts="$PROMPT_COUNT" age_hours="$AGE_HOURS" tools="$TOOL_COUNT" removes="$QUEUE_REMOVES" remove_rate="$REMOVE_RATE" 2>/dev/null || true
else
  echo "OK: session healthy"
  # Clear marker if session is healthy (new session)
  rm -f "$ALERT_MARKER" 2>/dev/null
fi
