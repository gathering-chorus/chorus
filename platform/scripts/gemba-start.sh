#!/usr/bin/env bash
# gemba-start.sh — Deterministic gemba entry point
# #2176: Reads pulse as single source of truth. Previously had a 5-fallback
# chain (dead /tmp/role-state path, cards CLI, session JSONL, observer JSONL,
# chorus SQLite) — classic add-without-retire entropy. Pulse is the assembler;
# everything here just reads pulse.roles.<role> + pulse.board.wip_cards.
# Recent activity still reads the tiles API for lastAction (real-time) with
# session JSONL as a narrow detail fallback. No more chain.
set -euo pipefail

ROLE="${1:?Usage: gemba-start.sh <role>}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PULSE_FILE="${PULSE_FILE:-/tmp/pulse-latest.json}"
TILES_API="${TILES_API:-http://localhost:3470/api/tiles}"

echo "$(date +%s)" > "/tmp/gemba-start-epoch-${ROLE}"

echo "=== GEMBA: $ROLE ==="
echo "--- $(TZ=America/New_York date '+%Y-%m-%d %H:%M') Boston ---"
echo ""

# 1. Role state + active card — read from pulse (the assembler).
#    pulse.roles.<role> has state, card, card_declared, card_inferred,
#    divergent, inferred_stale — everything the previous 5-source chain
#    was trying to reconstruct.
if [ -f "$PULSE_FILE" ]; then
  python3 - "$PULSE_FILE" "$ROLE" <<'PY'
import json, sys
pulse_file, role = sys.argv[1], sys.argv[2]
try:
    pulse = json.load(open(pulse_file))
except Exception as e:
    print(f"## State\nunknown (pulse unreadable: {e})\n")
    sys.exit(0)
r = pulse.get('roles', {}).get(role, {})
print("## State")
state = r.get('state', 'unknown')
card = r.get('card', '')
print(f"  state: {state}")
if card: print(f"  card:  {card}")
if r.get('divergent'):
    print(f"  divergent: declared={r.get('card_declared')} inferred={r.get('card_inferred')}")
if r.get('inferred_stale'):
    print(f"  inferred: STALE (>5min since tool-call observation)")
print()
print("## WIP Cards (owned by " + role + ")")
owned = [c for c in pulse.get('board', {}).get('wip_cards', [])
         if (c.get('owner') or '').lower() == role.lower()]
if owned:
    for c in owned:
        print(f"  #{c.get('id')}  {c.get('title','')[:80]}")
else:
    print("  (none)")
print()
PY
else
  echo "## State"
  echo "unknown (no pulse at $PULSE_FILE)"
  echo ""
fi

# 2. Last action — Clearing tiles API is the live "what are they doing right now"
#    surface. One HTTP call, no fallback. If it's unreachable, say so.
echo "## Last Action (live from tiles)"
TILE=$(curl -sf --max-time 2 "$TILES_API" 2>/dev/null | python3 -c "
import sys, json
try:
    tiles = json.load(sys.stdin)
    for t in tiles:
        if t.get('role') == '$ROLE':
            age = t.get('lastActionAge','')
            action = t.get('lastAction','')[:120]
            print(f'  {age}: {action}' if action else f'  {age}: (no recent action)')
            break
    else:
        print('  (role not in tiles)')
except: print('  (tiles API unreachable)')
" 2>/dev/null || echo "  (tiles API unreachable)")
echo "$TILE"
echo ""

# 3. Recent briefs — filesystem read, not a data-source duplication.
BRIEF_DIR=""
case "$ROLE" in
  silas) BRIEF_DIR="$SCRIPT_DIR/../../roles/silas/briefs" ;;
  kade)  BRIEF_DIR="$SCRIPT_DIR/../../roles/kade/briefs" ;;
  wren)  BRIEF_DIR="$SCRIPT_DIR/../../roles/wren/briefs" ;;
esac

if [ -n "$BRIEF_DIR" ] && [ -d "$BRIEF_DIR" ]; then
  echo "## Recent Briefs"
  ls -t "$BRIEF_DIR"/*.md 2>/dev/null | head -5 | while read -r f; do
    echo "  - $(basename "$f")"
  done
  echo ""
fi

echo "=== END GEMBA START ==="
