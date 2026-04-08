#!/usr/bin/env bash
# gemba-start.sh — Deterministic gemba entry point
# Outputs: card context, recent session activity, role state
# Usage: gemba-start.sh <role>
set -euo pipefail

ROLE="${1:?Usage: gemba-start.sh <role>}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CARDS="$SCRIPT_DIR/cards"

echo "=== GEMBA: $ROLE ==="
echo "--- $(TZ=America/New_York date '+%Y-%m-%d %H:%M') Boston ---"
echo ""

# 1. Role state from andon
STATE_FILE="/tmp/role-state-${ROLE}"
if [ -f "$STATE_FILE" ]; then
  echo "## State"
  cat "$STATE_FILE"
  echo ""
else
  echo "## State"
  echo "unknown (no state file)"
  echo ""
fi

# 2. WIP cards for this role
echo "## WIP Cards"
$CARDS mine "$ROLE" 2>/dev/null | grep "\[WIP\]" || echo "(none)"
echo ""

# 3. Active card detail (first WIP card)
CARD_ID=$($CARDS mine "$ROLE" 2>/dev/null | grep "\[WIP\]" | head -1 | grep -oE '[0-9]+' | head -1 || true)
if [ -n "$CARD_ID" ]; then
  echo "## Active Card #$CARD_ID"
  $CARDS view "$CARD_ID" 2>/dev/null || echo "(could not load card)"
  echo ""
fi

# 4. Recent session activity — same sources as gemba-tick.sh
echo "## Recent Activity (last 20 session lines)"

# Primary: direct session JSONL (no indexer dependency)
# Find the most recently modified project dir matching the role name
SESSION_DIR=$(ls -dt "$HOME/.claude/projects/"*chorus*"$ROLE"* 2>/dev/null | head -1)

RESULTS=""
if [ -n "$SESSION_DIR" ] && [ -d "$SESSION_DIR" ]; then
  LATEST_JSONL=$(ls -t "$SESSION_DIR"/*.jsonl 2>/dev/null | head -1)
  if [ -n "$LATEST_JSONL" ]; then
    RESULTS=$(tail -40 "$LATEST_JSONL" | python3 -c "
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
                        if 'cards ' in cmd or 'board-ts ' in cmd:
                            label = f'board op: {cmd[:90]}'
                        print(f'{ts}|{label}')
                    elif name == 'Read':
                        path = inp.get('file_path','')
                        print(f'{ts}|reading {path.split(\"/\")[-1]}')
                    elif name == 'Edit' or name == 'Write':
                        path = inp.get('file_path','')
                        print(f'{ts}|editing {path.split(\"/\")[-1]}')
                    elif name == 'Grep':
                        print(f'{ts}|searching: {inp.get(\"pattern\",\"\")}')
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

# Fallback: observer JSONL
OBS_FILE="/tmp/claude-team-scan/${ROLE}-observations.jsonl"
if [ -z "$RESULTS" ] && [ -f "$OBS_FILE" ]; then
  RESULTS=$(tail -20 "$OBS_FILE" | python3 -c "
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

# Last resort: chorus SQLite index
if [ -z "$RESULTS" ]; then
  DB_PATH="${CHORUS_DB:-$HOME/.chorus/index.db}"
  if [ -f "$DB_PATH" ]; then
    RESULTS=$(sqlite3 "$DB_PATH" "
      SELECT datetime(timestamp, 'localtime') as ts, substr(content, 1, 120) as line
      FROM messages
      WHERE role = '${ROLE}'
      ORDER BY timestamp DESC
      LIMIT 20;
    " 2>/dev/null || true)
  fi
fi

if [ -n "$RESULTS" ]; then
  echo "$RESULTS"
else
  echo "(no activity from session, observer, or index)"
fi
echo ""

# 5. Recent briefs for this role
BRIEF_DIR=""
case "$ROLE" in
  silas) BRIEF_DIR="$SCRIPT_DIR/../roles/silas/briefs" ;;
  kade)  BRIEF_DIR="$SCRIPT_DIR/../roles/kade/briefs" ;;
  wren)  BRIEF_DIR="$SCRIPT_DIR/../roles/wren/briefs" ;;
esac

if [ -n "$BRIEF_DIR" ] && [ -d "$BRIEF_DIR" ]; then
  echo "## Recent Briefs"
  ls -t "$BRIEF_DIR"/*.md 2>/dev/null | head -5 | while read -r f; do
    echo "  - $(basename "$f")"
  done
  echo ""
fi

echo "=== END GEMBA START ==="
