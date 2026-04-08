#!/usr/bin/env bash
# Demo Done Gate (DEC-048) — owned by /demo product (Wren)
# Contract: exit 0 = allow, exit 1 = deny (stderr = message)
# Logic: card can't be marked Done without demo evidence.
# Skip for chore/swat cards.

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-$(cd "$(dirname "$0")/../../../.." && pwd)}"
CARDS="$CHORUS_ROOT/platform/scripts/cards"

CARD_ID="${1:-}"

if [ -z "$CARD_ID" ]; then
  exit 0
fi

# Get card details
CARD_VIEW=$("$CARDS" view "$CARD_ID" 2>&1) || true
if echo "$CARD_VIEW" | head -1 | grep -q "^ERROR.*No task"; then
  exit 0  # Card not found — let other gates handle
fi

# Skip for chore and swat cards
if echo "$CARD_VIEW" | grep -qi "type:chore\|type:swat\|\[swat\]"; then
  exit 0
fi

# Check for demo evidence

# Evidence 1: Demo brief in wren/briefs/
BRIEF_DIR="$CHORUS_ROOT/roles/wren/briefs"
if ls "$BRIEF_DIR"/*demo*"$CARD_ID"* >/dev/null 2>&1; then
  exit 0
fi

# Evidence 2: Demo spine event via Chorus search API
if curl -sf "http://localhost:3340/api/chorus/search?q=card.demo.started+card%3D${CARD_ID}" 2>/dev/null | grep -q "card.demo.started"; then
  exit 0
fi

# Evidence 3: cards demo was called (recorded in card comments, not description)
COMMENTS=$(echo "$CARD_VIEW" | sed -n '/Comments/,$p')
if echo "$COMMENTS" | grep -qi "Demo started"; then
  exit 0
fi

# No evidence found
echo "Demo gate: #${CARD_ID} has no demo evidence. Run /demo ${CARD_ID} first." >&2
exit 1
