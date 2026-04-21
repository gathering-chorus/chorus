#!/usr/bin/env bash
# gate-spine-vikunja-bridge.sh — #2324
# Emits a gate.<name>.passed spine event AND keeps the Vikunja board coherent:
#   - applies label gate:<name>-passed to the card
#   - if <name>==product, transitions the card to Done
# This is the single surface that prevents gate-board divergence.
#
# Usage: gate-spine-vikunja-bridge.sh <card_id> <gate_name> <role> [key=value ...]
#   gate_name: code | quality | arch | ops | product
#   extra kv pairs are forwarded to chorus-log

set -euo pipefail

CARD_ID="${1:-}"
GATE="${2:-}"
ROLE="${3:-}"
shift 3 2>/dev/null || true

if [[ -z "$CARD_ID" || -z "$GATE" || -z "$ROLE" ]]; then
  echo "Usage: $0 <card_id> <gate_name> <role> [key=value ...]" >&2
  echo "  gate_name: code | quality | arch | ops | product" >&2
  exit 2
fi

case "$GATE" in
  code|quality|arch|ops|product) ;;
  *)
    echo "ERROR: unknown gate '$GATE'. Expected: code, quality, arch, ops, product" >&2
    exit 2
    ;;
esac

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
CHORUS_LOG_BIN="${CHORUS_ROOT}/platform/scripts/chorus-log"
CARDS_BIN="${CHORUS_ROOT}/platform/scripts/cards"

EVENT="gate.${GATE}.passed"
LABEL="gate:${GATE}-passed"

# 1. Emit spine event (the thing that already worked)
"$CHORUS_LOG_BIN" "$EVENT" "$ROLE" "card=${CARD_ID}" "$@" >/dev/null

# 2. Apply the Vikunja label (the new writer path)
if ! bash "$CARDS_BIN" label add "$CARD_ID" "$LABEL" >/dev/null 2>&1; then
  echo "warn: label write failed for #${CARD_ID} / ${LABEL}" >&2
  # Not exit-1 — the spine event already landed; surface the drift
fi

# 3. For gate:product, move the card to Done (enforcement-at-surface)
if [[ "$GATE" == "product" ]]; then
  if ! bash "$CARDS_BIN" done "$CARD_ID" >/dev/null 2>&1; then
    echo "warn: status transition to Done failed for #${CARD_ID}" >&2
  fi
fi

echo "bridge: ${EVENT} emitted + ${LABEL} applied for #${CARD_ID}"
