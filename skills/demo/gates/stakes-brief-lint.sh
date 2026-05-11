#!/usr/bin/env bash
# Demo Stakes-Brief Lint — owned by /demo product (Wren). #2893.
#
# Invoked by demo_stakes_lint.rs at PostToolUse on Skill(demo).
# Validates that the printed stakes brief meets the editorial gate:
#   1. Contains "Why this matters" line (case-insensitive)
#   2. Does NOT open with a mechanics-first anti-pattern
#      ("I built", "The API now", "Here's what changed", "I created",
#       "I added", "I wrote", "The function", "The script")
#
# Brief source: Bridge GET /api/messages — the most recent message from this
# role with text starting "[demo] #<card_id>". The brief is POSTed to Bridge
# by /demo Step 5c, so by PostToolUse time it is queryable.
#
# Outcome:
#   - Both checks pass → emit demo.stakes.passed, exit 0
#   - Any check fails  → emit demo.stakes.failed reason=<…>, exit 1 (stderr = refusal)
#
# Contract: stderr = refusal message. Exit 0 = pass, exit 1 = fail.

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
CHORUS_LOG="$CHORUS_ROOT/platform/scripts/chorus-log"
BRIDGE_URL="${BRIDGE_URL:-http://localhost:3470}"

CARD_ID="${1:-}"
ROLE="${2:-system}"
[ -z "$CARD_ID" ] && exit 0

emit_passed() {
  "$CHORUS_LOG" demo.stakes.passed "$ROLE" card="$CARD_ID" 2>/dev/null || true
  exit 0
}

emit_skipped() {
  local reason="$1"
  "$CHORUS_LOG" demo.stakes.skipped "$ROLE" card="$CARD_ID" reason="$reason" 2>/dev/null || true
  exit 0
}

emit_failed() {
  local reason="$1"
  "$CHORUS_LOG" demo.stakes.failed "$ROLE" card="$CARD_ID" reason="$reason" 2>/dev/null || true
  echo "demo-stakes-lint: failed for #${CARD_ID} — $reason" >&2
  exit 1
}

# Fetch the most recent [demo] message for this card from this role.
# Bridge returns most-recent-first. Wider limit accommodates a busy session.
BRIEF=$(curl -sf -G "$BRIDGE_URL/api/messages" \
  --data-urlencode "from=$ROLE" \
  --data-urlencode "limit=500" 2>/dev/null \
  | CARD_ID="$CARD_ID" python3 -c '
import json, os, sys
try:
    msgs = json.load(sys.stdin)
except Exception:
    sys.exit(2)
card_id = os.environ.get("CARD_ID", "")
needle = f"[demo] #{card_id}"
for m in msgs:
    text = m.get("text", "")
    if text.startswith(needle):
        print(text)
        sys.exit(0)
sys.exit(1)
' 2>/dev/null) || BRIEF=""

if [ -z "$BRIEF" ]; then
  # Brief not in Bridge — likely aged out of the message window. Skip rather
  # than refuse, so legitimate old-card /acp doesn't break on Bridge retention.
  # Spine event marks the skip so audits can find them.
  emit_skipped "no-brief-found-in-bridge"
fi

# Check 1: "Why this matters" must be present (case-insensitive).
if ! echo "$BRIEF" | grep -qi "why this matters"; then
  emit_failed "missing-why-this-matters"
fi

# Check 2: body must NOT open with mechanics-first patterns.
# Body = lines after the "[demo] #NNNN — <title>" header line.
# Skip blank lines and markdown bold markers to find the first content line.
BODY=$(echo "$BRIEF" | sed -E "1d" | sed -E '/^[[:space:]]*$/d')
OPENER=$(echo "$BODY" | sed -E 's/^[[:space:]]*\*+[[:space:]]*//' | head -1)

MECHANICS_PATTERNS=(
  "^I built"
  "^The API now"
  "^Here's what changed"
  "^I created"
  "^I added"
  "^I wrote"
  "^The function"
  "^The script"
)

for pat in "${MECHANICS_PATTERNS[@]}"; do
  if echo "$OPENER" | grep -qE "$pat"; then
    emit_failed "mechanics-first-opening"
  fi
done

emit_passed
