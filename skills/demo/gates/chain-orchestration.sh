#!/usr/bin/env bash
# Demo Chain-Orchestration — owned by /demo product (Wren). #2893.
#
# Invoked from accept_gate.rs at /acp time. Enforces Step 2 of /demo (gate
# chain) which was prose-only — model self-orchestrated. Failure modes
# covered:
#   1. Missing gate-pass comments on type:enhance / type:new / type:fix cards
#   2. Silent type:chore / type:swat bypass on cards with mistagged blast radius
#      (the failure mode #2893 itself surfaced — Wren tagged chore to skip
#      gates on a card that ultimately grew to multi-file substrate work)
#
# Outcome:
#   - All 5 gate comments present on a non-chore card → emit demo.chain.passed, exit 0
#   - Missing any gate comment on a non-chore card → emit demo.chain.failed, exit 1
#   - chore/swat with small blast radius → emit demo.chain.skipped reason=chore-tag, exit 0
#   - chore/swat with large blast radius → emit demo.chain.failed reason=mistagged-chore, exit 1
#
# Heuristic for "large blast radius" on chore/swat:
#   - files-changed > 3 OR total diff > 200 lines vs origin/main
#
# Contract: stderr = refusal message. Exit 0 = pass/skipped, exit 1 = fail.

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
CARDS="$CHORUS_ROOT/platform/scripts/cards"
CHORUS_LOG="$CHORUS_ROOT/platform/scripts/chorus-log"
HOME_DIR="${HOME:-/Users/jeffbridwell}"

CARD_ID="${1:-}"
ROLE="${2:-system}"
[ -z "$CARD_ID" ] && exit 0

emit_passed() {
  "$CHORUS_LOG" demo.chain.passed "$ROLE" card="$CARD_ID" 2>/dev/null || true
  exit 0
}

emit_skipped() {
  local reason="$1"
  "$CHORUS_LOG" demo.chain.skipped "$ROLE" card="$CARD_ID" reason="$reason" 2>/dev/null || true
  exit 0
}

emit_failed() {
  local reason="$1"
  "$CHORUS_LOG" demo.chain.failed "$ROLE" card="$CARD_ID" reason="$reason" 2>/dev/null || true
  echo "demo-chain: failed for #${CARD_ID} — $reason" >&2
  exit 1
}

CARD_VIEW=$("$CARDS" view "$CARD_ID" 2>&1) || true
if [ -z "$CARD_VIEW" ] || echo "$CARD_VIEW" | head -1 | grep -q "^ERROR"; then
  emit_skipped "card-not-found"
fi

# Extract card type from Domains line.
CARD_TYPE=$(echo "$CARD_VIEW" | grep -oE 'type:[a-z]+' | head -1 | sed 's/type://')
[ -z "$CARD_TYPE" ] && CARD_TYPE="unknown"

# chore / swat path: check blast radius before honoring the skip.
if [ "$CARD_TYPE" = "chore" ] || [ "$CARD_TYPE" = "swat" ]; then
  # Find the role's werk to inspect the branch diff.
  ROLE_WERK_VAR=$(echo "${ROLE}_WERK" | tr '[:lower:]' '[:upper:]')
  ROLE_WERK="${!ROLE_WERK_VAR:-$HOME_DIR/CascadeProjects/chorus-werk/${ROLE}}"

  files_changed=0
  lines_changed=0
  if [ -d "$ROLE_WERK/.git" ]; then
    # Count diff vs origin/main. Tolerate failure (return 0 counts).
    stats=$(cd "$ROLE_WERK" && git diff --shortstat origin/main...HEAD 2>/dev/null || echo "")
    files_changed=$(echo "$stats" | grep -oE '[0-9]+ file' | head -1 | grep -oE '[0-9]+' || echo 0)
    lines_changed=$(echo "$stats" | grep -oE '[0-9]+ insertion' | head -1 | grep -oE '[0-9]+' || echo 0)
    deletions=$(echo "$stats" | grep -oE '[0-9]+ deletion' | head -1 | grep -oE '[0-9]+' || echo 0)
    lines_changed=$((lines_changed + deletions))
  fi

  if [ "$files_changed" -gt 3 ] || [ "$lines_changed" -gt 200 ]; then
    emit_failed "mistagged-${CARD_TYPE}:files=${files_changed},lines=${lines_changed}-exceeds-threshold(3-files,200-lines);retag-enhance-or-new-or-fix-and-run-gates"
  fi

  emit_skipped "${CARD_TYPE}-tag:files=${files_changed},lines=${lines_changed}"
fi

# enhance / new / fix / other: require all 5 gate comments.
MISSING=""
for gate in "gate:product-pass" "gate:code-pass" "gate:quality-pass" "gate:arch-pass" "gate:ops-pass"; do
  if ! echo "$CARD_VIEW" | grep -q "$gate"; then
    MISSING="${MISSING}${gate} "
  fi
done

if [ -n "$MISSING" ]; then
  emit_failed "missing-gates:${MISSING% }"
fi

emit_passed
