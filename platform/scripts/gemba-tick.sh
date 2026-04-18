#!/usr/bin/env bash
# gemba-tick.sh — Deterministic gemba tick
# #2176: Reads pulse as single source of truth. Previously had the same
# 5-fallback chain as gemba-start.sh; retired.
set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
ROLE="${1:?Usage: gemba-tick.sh <role>}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PULSE_FILE="${PULSE_FILE:-/tmp/pulse-latest.json}"
TILES_API="${TILES_API:-http://localhost:3470/api/tiles}"
EPOCH_FILE="/tmp/gemba-start-epoch-${ROLE}"
LAST_CHECK_FILE="/tmp/gemba-last-check-${ROLE}"
NOW=$(date +%s)
if [ ! -f "$EPOCH_FILE" ]; then
  echo "$NOW" > "$EPOCH_FILE"
fi
START_EPOCH=$(cat "$EPOCH_FILE")
ELAPSED=$(( NOW - START_EPOCH ))

echo "=== GEMBA TICK: $ROLE ==="
echo "--- $(TZ=America/New_York date '+%Y-%m-%d %H:%M') Boston | ${ELAPSED}s elapsed ---"
echo ""

# 1. Role state + last action — pulse + tiles API, no chain.
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
print(f"  state: {r.get('state','unknown')}")
card = r.get('card', '')
if card: print(f"  card:  {card}")
if r.get('divergent'):
    print(f"  divergent: declared={r.get('card_declared')} inferred={r.get('card_inferred')}")
print()
PY
else
  echo "## State"
  echo "unknown (no pulse at $PULSE_FILE)"
  echo ""
fi

# 2. Last action — Clearing tiles API is the live surface.
echo "## Last Action"
curl -sf --max-time 2 "$TILES_API" 2>/dev/null | python3 -c "
import sys, json
try:
    tiles = json.load(sys.stdin)
    for t in tiles:
        if t.get('role') == '$ROLE':
            age = t.get('lastActionAge','')
            action = t.get('lastAction','')[:140]
            print(f'  {age}: {action}' if action else f'  {age}: (no recent action)')
            break
    else:
        print('  (role not in tiles)')
except: print('  (tiles API unreachable)')
" 2>/dev/null || echo "  (tiles API unreachable)"
echo ""

# 3. Uncommitted changes in role's working dirs (git is its own source of truth — not overlapping pulse).
echo "## Uncommitted Changes"
ROLE_DIR=""
case "$ROLE" in
  silas) ROLE_DIR="${CHORUS_ROOT}/roles/silas" ;;
  kade)  ROLE_DIR="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site" ;;
  wren)  ROLE_DIR="${CHORUS_ROOT}/roles/wren" ;;
esac
for DIR in "$ROLE_DIR" "${CHORUS_ROOT}"; do
  if [ -n "$DIR" ] && [ -d "$DIR/.git" ]; then
    CHANGES=$(cd "$DIR" && git diff --name-only --diff-filter=M 2>/dev/null | head -5)
    if [ -n "$CHANGES" ]; then
      echo "  Modified in $(basename "$DIR"):"
      echo "$CHANGES" | sed 's/^/    /'
    fi
  fi
done
echo ""

# 4. Role screen capture (live visual).
echo "## Role Screen"
SCREENSHOT=$("$SCRIPT_DIR/role-screenshot.sh" "$ROLE" 2>/dev/null || true)
if [ -n "$SCREENSHOT" ] && [ -f "$SCREENSHOT" ]; then
  echo "  Captured: $SCREENSHOT"
else
  echo "  (screenshot failed or role not active)"
fi
echo ""

# 5. Checkpoint
TZ=America/New_York date '+%Y-%m-%dT%H:%M:%S' > "$LAST_CHECK_FILE"

# 6. TTL check
if [ "$START_EPOCH" -gt 0 ] && [ "$ELAPSED" -gt 600 ]; then
  echo "## TTL EXPIRED"
  echo "Observation window: ${ELAPSED}s (limit: 600s)"
fi

echo "=== END TICK ==="
