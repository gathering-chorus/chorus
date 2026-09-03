#!/usr/bin/env bash
# log-harvest.sh — #4084. Hourly: every launchd unit's log as chorus:LogSource rows
# in urn:chorus:domains:logs. gen (pure) -> check (unmapped = red, never a guess)
# -> load (PUT only when changed, via service-harvest-load.sh). Emits one spine
# event per run with the counts, so "did the harvest run" is a query.
set -euo pipefail
CHORUS_ROOT="${CHORUS_HOME:-${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}}"
S="$CHORUS_ROOT/platform/scripts"
CHORUS_LOG="${CHORUS_LOG:-$S/chorus-log}"
OUT="${LOG_HARVEST_OUT:-$HOME/.chorus/log-harvest/library.ttl}"
mkdir -p "$(dirname "$OUT")"
export CHORUS_HARVEST_GRAPH="${CHORUS_HARVEST_GRAPH:-urn:chorus:domains:logs}"

if ! python3 "$S/log-harvest-gen.py" "$@" > "$OUT.tmp" 2> "$OUT.stderr"; then
  cat "$OUT.stderr" >&2
  "$CHORUS_LOG" log.harvest.failed silas reason=gen 2>/dev/null || true
  exit 1
fi
cat "$OUT.stderr" >&2
UNMAPPED=$(python3 "$S/log-harvest-gen.py" "$@" --check 2>/dev/null | grep -c '^UNMAPPED' || true)
mv "$OUT.tmp" "$OUT"
ROWS=$(grep -c 'a chorus:LogSource' "$OUT" || true)

# load only when changed (the harvester's own idempotency rule, #3870)
bash "$S/service-harvest-load.sh" --generated "$OUT" || { "$CHORUS_LOG" log.harvest.failed silas reason=load rows="$ROWS" 2>/dev/null || true; exit 1; }
"$CHORUS_LOG" log.harvest.completed silas rows="$ROWS" unmapped="$UNMAPPED" graph="$CHORUS_HARVEST_GRAPH" 2>/dev/null || true
echo "log-harvest: $ROWS LogSource rows -> <$CHORUS_HARVEST_GRAPH>, $UNMAPPED unmapped"
[ "$UNMAPPED" -eq 0 ] || { echo "log-harvest: $UNMAPPED unit(s) have no UnitDomainMapping row — add them to roles/silas/ontology/unit-domain-4084.ttl" >&2; exit 3; }
