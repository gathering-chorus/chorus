#!/usr/bin/env bash
# #3753 AC3 — shared load-aware failure reclassifier.
# Reads one failure per line on stdin. Prints each back prefixed:
#   FAIL|<line>   — stays a failure
#   WARN|unmeasurable under load (<load=X max=Y>): <line>
# Timeout-class lines (no-response markers) downgrade ONLY when the box is
# loaded right now (nightly-suites.sh --load-gate, one shared predicate +
# nightly-load.conf thresholds). Response-code failures and quiet-box
# timeouts pass through as FAIL. Consumers: deep-health.sh; probe scripts
# use the same gate directly. Hermetic tests stub load via NIGHTLY_LOAD_STUB.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LG_BIN="${LOAD_GATE_BIN:-$SCRIPT_DIR/nightly-suites.sh}"

lg_out=""
loaded=0
if [ -x "$LG_BIN" ]; then
  if ! lg_out=$("$LG_BIN" --load-gate 2>/dev/null); then loaded=1; fi
fi

while IFS= read -r line; do
  [ -n "$line" ] || continue
  if [ "$loaded" = "1" ]; then
    case "$line" in
      *"code=000"*|*"returned 000"*|*"exit=28"*|*timeout*|*unreachable*|*"exit=7"*)
        printf 'WARN|unmeasurable under load (%s): %s\n' "$lg_out" "$line"
        continue ;;
    esac
  fi
  printf 'FAIL|%s\n' "$line"
done
