#!/usr/bin/env bash
# gemba-tick.sh — Deterministic gemba tick
# Outputs: new activity since last check for observed role
# Usage: gemba-tick.sh <role> [start_epoch]
set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects}"

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

# Direct session JSONL — fastest path, no indexer dependency (#2021)
SESSION_DIR=""
case "$ROLE" in
  silas) SESSION_DIR="$HOME/.claude/projects/-Users-jeffbridwell-CascadeProjects-chorus-architect" ;;
  kade)  SESSION_DIR="$HOME/.claude/projects/-Users-jeffbridwell-CascadeProjects-chorus-engineer" ;;
  wren)  SESSION_DIR="$HOME/.claude/projects/-Users-jeffbridwell-CascadeProjects-chorus-roles-wren" ;;
esac

if [ -n "$SESSION_DIR" ] && [ -d "$SESSION_DIR" ]; then
  LATEST_JSONL=$(ls -t "$SESSION_DIR"/*.jsonl 2>/dev/null | head -1)
  if [ -n "$LATEST_JSONL" ]; then
    RESULTS=$(tail -20 "$LATEST_JSONL" | python3 -c "
import sys, json
for line in sys.stdin:
    try:
        d = json.loads(line.strip())
        msg_type = d.get('type','')
        ts = d.get('timestamp','')[:16] or '?'
        if msg_type == 'assistant':
            for block in d.get('message',{}).get('content',[]):
                if block.get('type') == 'tool_use':
                    name = block.get('name','?')
                    inp = block.get('input',{})
                    if name == 'Bash':
                        cmd = inp.get('command','')[:100]
                        desc = inp.get('description','')
                        label = desc if desc else f'bash: {cmd}'
                        # Detect board ops
                        if 'cards ' in cmd or 'board-ts ' in cmd:
                            label = f'board op: {cmd[:90]}'
                        print(f'{ts}|{label}')
                    elif name == 'Read':
                        path = inp.get('file_path','')
                        print(f'{ts}|reading {path.split("/")[-1]}')
                    elif name == 'Edit' or name == 'Write':
                        path = inp.get('file_path','')
                        print(f'{ts}|editing {path.split("/")[-1]}')
                    elif name == 'Grep':
                        print(f'{ts}|searching: {inp.get("pattern","")}')
                    else:
                        print(f'{ts}|{name}')
        elif msg_type == 'human':
            content = d.get('message',{}).get('content','')
            if isinstance(content, str) and len(content) > 5 and not content.startswith('<'):
                print(f'{ts}|user: {content[:100]}')
    except: pass
" 2>/dev/null || true)
  fi
fi

# Fallback 1: observer JSONL (written by PostToolUse hook)
OBS_FILE="/tmp/claude-team-scan/${ROLE}-observations.jsonl"
if [ -z "$RESULTS" ] && [ -f "$OBS_FILE" ]; then
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
  silas) ROLE_DIR="${CHORUS_ROOT}/roles/silas" ;;
  kade)  ROLE_DIR="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site" ;;
  wren)  ROLE_DIR="${CHORUS_ROOT}/chorus/roles/wren" ;;
esac

# Check git for uncommitted changes in role's working dirs
for DIR in "$ROLE_DIR" ${CHORUS_ROOT}/chorus; do
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
