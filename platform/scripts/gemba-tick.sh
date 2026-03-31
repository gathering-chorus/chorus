#!/usr/bin/env bash
# gemba-tick.sh — Deterministic gemba tick
# Outputs: new activity since last check for observed role
# Usage: gemba-tick.sh <role> [start_epoch]
set -euo pipefail

ROLE="${1:?Usage: gemba-tick.sh <role> [start_epoch]}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
START_EPOCH="${2:-0}"
NOW=$(date +%s)
ELAPSED=$(( NOW - START_EPOCH ))
LAST_CHECK_FILE="/tmp/gemba-last-check-${ROLE}"

echo "=== GEMBA TICK: $ROLE ==="
echo "--- $(TZ=America/New_York date '+%Y-%m-%d %H:%M') Boston | ${ELAPSED}s elapsed ---"
echo ""

# 1. Role state (new JSON format, fallback to old)
STATE_JSON="/tmp/claude-team-scan/${ROLE}-declared.json"
STATE_FILE="/tmp/role-state-${ROLE}"
echo "## State"
if [ -f "$STATE_JSON" ]; then
  STATE=$(python3 -c "
import json, sys
d = json.load(open('$STATE_JSON'))
card = d.get('card', '')
state = d.get('state', 'unknown')
ts = d.get('last_emit', '?')
print(f'{state} card=#{card} (declared {ts})')
" 2>/dev/null || cat "$STATE_JSON")
  echo "$STATE"
elif [ -f "$STATE_FILE" ]; then
  cat "$STATE_FILE"
else
  echo "unknown (no state file)"
fi
echo ""

# 2. New activity since last check
DB_PATH="${CHORUS_DB:-$HOME/.chorus/index.db}"
LAST_CHECK="1970-01-01"
if [ -f "$LAST_CHECK_FILE" ]; then
  LAST_CHECK=$(cat "$LAST_CHECK_FILE")
fi

echo "## New Activity"
RESULTS=""

# Primary: observer JSONL (written by PostToolUse hook — always current)
OBS_FILE="/tmp/claude-team-scan/${ROLE}-observations.jsonl"
if [ -f "$OBS_FILE" ]; then
  RESULTS=$(tail -10 "$OBS_FILE" | python3 -c "
import sys, json
for line in sys.stdin:
    try:
        d = json.loads(line.strip())
        ts = d.get('ts','?')[:16]
        digest = d.get('digest','')[:120]
        if digest:
            print(f'{ts}|{digest}')
    except: pass
" 2>/dev/null || true)
fi

# Fallback: Chorus SQLite index
if [ -z "$RESULTS" ] && [ -f "$DB_PATH" ]; then
  RESULTS=$(sqlite3 "$DB_PATH" "
    SELECT datetime(timestamp, 'localtime') as ts, substr(content, 1, 150) as line
    FROM messages
    WHERE role = '${ROLE}'
      AND timestamp > '${LAST_CHECK}'
    ORDER BY timestamp DESC
    LIMIT 10;
  " 2>/dev/null || true)
fi

if [ -n "$RESULTS" ]; then
  echo "$RESULTS"
else
  echo "(no new activity from observer or index)"
fi
echo ""

# 3. Recent file changes by this role (live signal)
echo "## Recent Files Changed"
ROLE_DIR=""
case "$ROLE" in
  silas) ROLE_DIR="/Users/jeffbridwell/CascadeProjects/chorus/architect" ;;
  kade)  ROLE_DIR="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site" ;;
  wren)  ROLE_DIR="/Users/jeffbridwell/CascadeProjects/chorus/product-manager" ;;
esac

# Check git for uncommitted changes in role's working dirs
for DIR in "$ROLE_DIR" /Users/jeffbridwell/CascadeProjects/chorus; do
  if [ -n "$DIR" ] && [ -d "$DIR/.git" ]; then
    CHANGES=$(cd "$DIR" && git diff --name-only --diff-filter=M 2>/dev/null | head -5)
    if [ -n "$CHANGES" ]; then
      echo "  Modified in $(basename "$DIR"):"
      echo "$CHANGES" | sed 's/^/    /'
    fi
  fi
done

# Check for recently modified files (last 2 min) in role's session artifacts
find /tmp -maxdepth 1 -name "*${ROLE}*" -newer "$LAST_CHECK_FILE" 2>/dev/null | head -5 | while read -r f; do
  echo "  $(basename "$f") ($(stat -f '%Sm' -t '%H:%M' "$f" 2>/dev/null))"
done
echo ""

# 4. Role screen capture (live view)
echo "## Role Screen"
SCREENSHOT=$("$SCRIPT_DIR/role-screenshot.sh" "$ROLE" 2>/dev/null || true)
if [ -n "$SCREENSHOT" ] && [ -f "$SCREENSHOT" ]; then
  echo "  Captured: $SCREENSHOT"
else
  echo "  (screenshot failed or role not active)"
fi
echo ""

# 5. Save checkpoint
TZ=America/New_York date '+%Y-%m-%dT%H:%M:%S' > "$LAST_CHECK_FILE"

# 4. TTL check
if [ "$START_EPOCH" -gt 0 ] && [ "$ELAPSED" -gt 600 ]; then
  echo "## TTL EXPIRED"
  echo "Observation window: ${ELAPSED}s (limit: 600s)"
fi

echo "=== END TICK ==="
