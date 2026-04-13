#!/usr/bin/env bash
# Demo Preflight Gate — owned by /demo product (Wren)
# Contract: exit 0 = allow, exit 1 = deny (stderr = message)
# Called by chorus-hooks shim — no logic in the hook, just dispatch.
#
# Gates:
#   1. Card must be in WIP
#   2. AC must exist in description
#   3. Smoke check must pass
#   4. ICD render check for domain cards

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-$(cd "$(dirname "$0")/../../../.." && pwd)}"
CARDS="$CHORUS_ROOT/platform/scripts/cards"
SMOKE="$CHORUS_ROOT/platform/scripts/smoke-check.sh"

CARD_ID="${1:-}"
CHORUS_LOG="$CHORUS_ROOT/platform/scripts/chorus-log"

if [ -z "$CARD_ID" ]; then
  exit 0  # No card ID — let the skill handle it
fi

"$CHORUS_LOG" demo.preflight.started system card="$CARD_ID" 2>/dev/null || true

# Gate 1: Card must be in WIP
CARD_VIEW=$("$CARDS" view "$CARD_ID" 2>&1) || true
if echo "$CARD_VIEW" | head -1 | grep -q "^ERROR.*No task"; then
  echo "Demo blocked: #${CARD_ID} not found on board." >&2
  "$CHORUS_LOG" demo.preflight.failed system card="$CARD_ID" reason="not_found" 2>/dev/null || true
  exit 1
fi
if [ -z "$CARD_VIEW" ]; then
  exit 0  # cards returned nothing — don't block
fi

STATUS=$(echo "$CARD_VIEW" | grep "Status:" | sed 's/.*Status:\s*//' | tr -d ' ')
if [ "$STATUS" != "WIP" ] && [ "$STATUS" != "SWAT" ]; then
  echo "Demo blocked: #${CARD_ID} is in ${STATUS} — must be in WIP to demo. Move it first." >&2
  "$CHORUS_LOG" demo.preflight.failed system card="$CARD_ID" reason="wrong_status" status="$STATUS" 2>/dev/null || true
  exit 1
fi

# Gate 2: AC must exist in description
if ! echo "$CARD_VIEW" | grep -qi "## ac\|acceptance criteria\|- \[ \]"; then
  echo "Demo blocked: #${CARD_ID} has no acceptance criteria. Add ## AC to the card description before demoing." >&2
  "$CHORUS_LOG" demo.preflight.failed system card="$CARD_ID" reason="no_ac" 2>/dev/null || true
  exit 1
fi

# Gate 3: Smoke check
if [ -x "$SMOKE" ]; then
  SMOKE_OUT=$(bash "$SMOKE" --all 2>&1) || {
    FAILURES=$(echo "$SMOKE_OUT" | grep "FAIL" || true)
    echo "Demo blocked: smoke check failed. Fix before demoing." >&2
    [ -n "$FAILURES" ] && echo "$FAILURES" >&2
    "$CHORUS_LOG" demo.preflight.failed system card="$CARD_ID" reason="smoke_check" 2>/dev/null || true
    exit 1
  }
fi

# Gate 4: ICD render check for domain cards
DOMAINS=$(echo "$CARD_VIEW" | grep -oE "domain:[a-z]+" | sed 's/domain://' | grep -v "infrastructure" || true)
for DOMAIN in $DOMAINS; do
  TTL="$CHORUS_ROOT/architect/icd-instance-${DOMAIN}.ttl"
  if [ -f "$TTL" ]; then
    SIZE=$(wc -c < "$TTL")
    if [ "$SIZE" -lt 100 ]; then
      echo "Demo blocked: ICD instance file for ${DOMAIN} is empty or stub." >&2
      "$CHORUS_LOG" demo.preflight.failed system card="$CARD_ID" reason="icd_empty" domain="$DOMAIN" 2>/dev/null || true
      exit 1
    fi
    if ! grep -q "icd:provider\|Provider" "$TTL"; then
      echo "Demo blocked: ICD for ${DOMAIN} missing provider section." >&2
      "$CHORUS_LOG" demo.preflight.failed system card="$CARD_ID" reason="icd_no_provider" domain="$DOMAIN" 2>/dev/null || true
      exit 1
    fi
    if ! grep -q "icd:field\|icd:maps" "$TTL"; then
      echo "Demo blocked: ICD for ${DOMAIN} missing field mappings." >&2
      "$CHORUS_LOG" demo.preflight.failed system card="$CARD_ID" reason="icd_no_fields" domain="$DOMAIN" 2>/dev/null || true
      exit 1
    fi
  fi
done

# All gates passed
"$CHORUS_LOG" demo.preflight.passed system card="$CARD_ID" 2>/dev/null || true
exit 0
