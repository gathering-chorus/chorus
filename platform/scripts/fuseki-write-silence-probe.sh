#!/usr/bin/env bash
# #3560 — Fuseki write-silence probe. READ-ONLY go/no-go before a quiesced backup.
#
# Watches the Fuseki request log (ground truth: every SPARQL update is a logged
# `POST .../pods/update`) for write activity over a sample window. Silence means the
# node table is stable and `$/backup` can read it without hitting an uncommitted
# boundary ("Unrecognized type 0"). Any residual writes are named so the missed
# writer is identified empirically, not from memory (Kade's #3559 nav point).
#
# Exit 0 = write-silent (GO).  Exit 1 = writes detected (NO-GO; lists signatures).
# Usage: fuseki-write-silence-probe.sh [window_seconds]   (default 30)
set -uo pipefail

WINDOW="${1:-30}"
FLOG="${FUSEKI_LOG:-$HOME/Library/Logs/Gathering/fuseki.log}"
[ -r "$FLOG" ] || { echo "probe: cannot read $FLOG" >&2; exit 2; }

# verify-idle precheck: no in-flight hydrate run (disable only stops the NEXT launch)
if pgrep -f "graph-hydrate-tag.sh" >/dev/null 2>&1; then
  echo "NO-GO: graph-hydrate-tag.sh still running (PID $(pgrep -f graph-hydrate-tag.sh | tr '\n' ' '))— wait for it to finish"
  exit 1
fi

start=$(wc -l < "$FLOG")
echo "probing ${WINDOW}s for pods writes (from line ${start})..."
sleep "$WINDOW"

new=$(tail -n +$((start + 1)) "$FLOG")
writes=$(printf '%s\n' "$new" | grep -cE "POST .*/pods/update" || true)
echo "update POSTs in window: ${writes}"

if [ "${writes}" -gt 0 ]; then
  echo "NO-GO: not write-silent. Residual update sources (request ids — correlate to writer):"
  printf '%s\n' "$new" | grep -E "POST .*/pods/update" | sed -E 's/^([0-9:]+).*\[([0-9]+)\].*/  \1  req \2/' | tail -10
  exit 1
fi

echo "GO: write-silent for ${WINDOW}s — safe to run \$/backup/pods"
exit 0
