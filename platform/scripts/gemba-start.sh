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

# 4. Recent session activity from chorus log
echo "## Recent Activity (last 20 session lines)"
DB_PATH="${CHORUS_DB:-$HOME/.chorus/index.db}"
if [ -f "$DB_PATH" ]; then
  sqlite3 "$DB_PATH" "
    SELECT datetime(timestamp, 'localtime') as ts, substr(content, 1, 120) as line
    FROM messages
    WHERE role = '${ROLE}'
    ORDER BY timestamp DESC
    LIMIT 20;
  " 2>/dev/null || echo "(index query failed)"
else
  echo "(no chorus index)"
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
