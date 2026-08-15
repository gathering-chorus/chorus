#!/usr/bin/env bash
# #3870 — service drift check: unmapped / vanished / stale ServiceInstances.
# RED (exit 1) when any drift row exists, printing each finding BY NAME.
# GREEN (exit 0) only on zero rows. Refuses (exit 2) when the data source is
# missing or unreachable — an unreachable store is not a quiet store (the
# fuseki-down-returns-[] lesson: absence must never impersonate health).
#
# Live:    service-drift-check.sh                     (queries Fuseki)
# Fixture: service-drift-check.sh --data <file.ttl> [--cutoff <iso8601>]
#          (arq over the file — the bats suite's hermetic path; SAME .rq file,
#          so the tested thing is the shipped thing.)
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
QUERY_SRC="$HERE/service-drift.rq"
[ -f "$QUERY_SRC" ] || { echo "service-drift-check: $QUERY_SRC missing" >&2; exit 2; }

FUSEKI="${CHORUS_FUSEKI:-http://localhost:3030}"
DATASET="${CHORUS_FUSEKI_DATASET:-pods}"

DATA="" CUTOFF=""
while [ $# -gt 0 ]; do
  case "$1" in
    --data)   DATA="$2"; shift 2 ;;
    --cutoff) CUTOFF="$2"; shift 2 ;;
    *) echo "service-drift-check: unknown arg $1" >&2; exit 2 ;;
  esac
done

if [ -z "$CUTOFF" ]; then
  CUTOFF="$(date -u -v-24H '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null || date -u -d '24 hours ago' '+%Y-%m-%dT%H:%M:%SZ')"
fi

# Inject the cutoff literal into the query (arq has no portable outer-var bind).
Q="$(mktemp)"; trap 'rm -f "$Q"' EXIT
sed "s|?cutoff|\"$CUTOFF\"^^xsd:dateTime|g" "$QUERY_SRC" > "$Q"

if [ -n "$DATA" ]; then
  [ -f "$DATA" ] || { echo "service-drift-check: fixture $DATA not found" >&2; exit 2; }
  ARQ="${ARQ:-$(command -v arq || echo /opt/homebrew/bin/arq)}"
  [ -x "$ARQ" ] || { echo "service-drift-check: arq not found" >&2; exit 2; }
  RESULT="$("$ARQ" --data "$DATA" --query "$Q" --results=TSV)" \
    || { echo "service-drift-check: arq failed" >&2; exit 2; }
else
  RESULT="$(curl -sf --max-time 15 "$FUSEKI/$DATASET/sparql" \
      --data-urlencode "query@$Q" -H 'Accept: text/tab-separated-values')" \
    || { echo "service-drift-check: Fuseki unreachable at $FUSEKI — refusing to report quiet" >&2; exit 2; }
fi

FINDINGS="$(printf '%s\n' "$RESULT" | tail -n +2 | sed '/^[[:space:]]*$/d' || true)"
if [ -n "$FINDINGS" ]; then
  COUNT="$(printf '%s\n' "$FINDINGS" | wc -l | tr -d ' ')"
  echo "service-drift: RED — $COUNT finding(s)"
  printf '%s\n' "$FINDINGS"
  exit 1
fi
echo "service-drift: green — no unmapped, vanished, or stale instances"
exit 0
