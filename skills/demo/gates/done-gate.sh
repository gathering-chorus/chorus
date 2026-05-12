#!/usr/bin/env bash
# Demo Done Gate (DEC-048) — owned by /demo product (Wren)
# Contract: exit 0 = allow, exit 1 = deny (stderr = message)
# Logic: card can't be marked Done without demo evidence.
# Skip for chore/swat cards.

set -euo pipefail

# #1815: fail-open on EXPLICIT empty CHORUS_ROOT (set but empty) — env issue,
# let other gates handle. `${VAR-default}` substitutes only when unset, so
# explicit empty stays empty. The fail-open guard catches it before the
# CARDS path becomes broken.
CHORUS_ROOT="${CHORUS_ROOT-$(cd "$(dirname "$0")/../../.." && pwd)}"
if [ -z "$CHORUS_ROOT" ]; then
  exit 0
fi
CARDS="$CHORUS_ROOT/platform/scripts/cards"

CARD_ID="${1:-}"
CHORUS_LOG="$CHORUS_ROOT/platform/scripts/chorus-log"

ROLE="${2:-system}"

if [ -z "$CARD_ID" ]; then
  exit 0
fi

# #1916: --proven bypass for retroactive closure
# Usage: done-gate.sh <card-id> <role> --proven "1815 1898 1894"
if echo "$@" | grep -q -- '--proven'; then
  EVIDENCE=$(echo "$@" | sed 's/.*--proven *//')
  "$CHORUS_LOG" card.accepted.proven "$ROLE" card="$CARD_ID" evidence="$EVIDENCE" 2>/dev/null || true
  echo "Proven: #${CARD_ID} — evidence from ${EVIDENCE}"
  exit 0
fi

"$CHORUS_LOG" demo.done_gate.started system card="$CARD_ID" 2>/dev/null || true

# Get card details
CARD_VIEW=$("$CARDS" view "$CARD_ID" 2>&1) || true
if echo "$CARD_VIEW" | head -1 | grep -q "^ERROR.*No task"; then
  exit 0  # Card not found — let other gates handle
fi

# Skip for chore and swat cards
if echo "$CARD_VIEW" | grep -qi "type:chore\|type:swat\|\[swat\]"; then
  exit 0
fi

# Check for demo evidence — CANONICAL FORM ONLY (#2910, Jeff direct 2026-05-12).
# The single gate-evidence is the demo:preflight-pass card comment that /demo
# Step 1.5 writes for every demo. Brief files, spine events, and "Demo started"
# comments still get emitted by /demo for observability, but they are no longer
# load-bearing for the gate. One reader, one form, no drift.
#
# Why: tonight (2026-05-12) chorus_acp refused on a card that had the comment
# evidence; manual done-gate.sh accepted the same card. Root cause: cards SDK
# checkDemoEvidence and this script had drifted to different evidence sets and
# the SDK's spine-event check used a stale field name. Moving to one canonical
# form means SDK and script reduce to identical greps.
COMMENTS=$(echo "$CARD_VIEW" | sed -n '/Comments/,$p')
if echo "$COMMENTS" | grep -qi "demo:preflight-pass"; then
  "$CHORUS_LOG" demo.done_gate.passed system card="$CARD_ID" evidence="preflight-comment" 2>/dev/null || true
  exit 0
fi

# No canonical evidence found
echo "Demo gate: #${CARD_ID} has no demo evidence. Run /demo ${CARD_ID} first." >&2
"$CHORUS_LOG" demo.done_gate.failed system card="$CARD_ID" reason="no_evidence" 2>/dev/null || true
exit 1
