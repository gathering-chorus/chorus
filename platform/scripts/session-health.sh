#!/usr/bin/env bash
# session-health.sh — Session degradation detection (#2271)
# Reports: prompt count, session age, tool call count, queue removes (compaction proxy)
# Alerts when session exceeds thresholds — gas gauge, not check engine light.
set -euo pipefail

SESSIONS_DIR="$HOME/.claude/projects"
HOOKS_LOG="$HOME/Library/Logs/Gathering/hooks.log"
NUDGE="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/nudge"
CHORUS_LOG="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log"

# Defaults
ROLE=""
PROMPT_THRESHOLD=400
HOUR_THRESHOLD=4
ALERT=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --role) ROLE="$2"; shift 2 ;;
    --threshold) PROMPT_THRESHOLD="$2"; shift 2 ;;
    --hour-threshold) HOUR_THRESHOLD="$2"; shift 2 ;;
    *) echo "Usage: session-health.sh --role <role> [--threshold N] [--hour-threshold H]" >&2; exit 1 ;;
  esac
done

if [ -z "$ROLE" ]; then
  echo "Error: --role required" >&2; exit 1
fi

# Map role to project directory
case "$ROLE" in
  silas) PROJECT_SUFFIX="chorus-architect" ;;
  wren) PROJECT_SUFFIX="chorus-product-manager" ;;
  kade) PROJECT_SUFFIX="chorus-engineer" ;;
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

# Count queue remove operations (compaction proxy)
QUEUE_REMOVES=$(grep -c '"queue-operation".*"remove"' "$LATEST_SESSION" 2>/dev/null || \
                grep -c 'queue-operation' "$LATEST_SESSION" 2>/dev/null || echo 0)

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
  TOOL_COUNT=$(grep "$SESSION_ID" "$HOOKS_LOG" 2>/dev/null | grep -c "allow" || echo 0)
fi

# Tool call rate (calls per minute)
if [ "$AGE_MIN" -gt 0 ]; then
  TOOL_RATE=$((TOOL_COUNT / AGE_MIN))
else
  TOOL_RATE=0
fi

# Output metrics
echo "session=$SESSION_ID role=$ROLE"
echo "prompts=$PROMPT_COUNT responses=$RESPONSE_COUNT tools=$TOOL_COUNT"
echo "age_min=$AGE_MIN age_hours=$AGE_HOURS tool_rate=${TOOL_RATE}/min"
echo "queue_removes=$QUEUE_REMOVES compaction=not_emitted_by_claude_code"

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

if [ "$ALERT" = true ]; then
  echo "WARN: ${ALERTS}Consider /reboot for fresh context."
  # Nudge the role and Wren
  "$NUDGE" "$ROLE" "session-health: ${ALERTS}Consider /reboot." 2>/dev/null || true
  "$NUDGE" wren "session-health: ${ROLE} session at ${PROMPT_COUNT} prompts, ${AGE_HOURS}h. May need reboot." 2>/dev/null || true
  "$CHORUS_LOG" session.health.warning "$ROLE" prompts="$PROMPT_COUNT" age_hours="$AGE_HOURS" tools="$TOOL_COUNT" 2>/dev/null || true
else
  echo "OK: session healthy"
fi
