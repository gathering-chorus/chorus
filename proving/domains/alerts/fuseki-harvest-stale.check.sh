#!/usr/bin/env bash
# fuseki-harvest-stale.check.sh — #2327
# Distinguishes three states so the alert doesn't conflate unreachability with empty:
#   - curl failure (Fuseki unreachable / timeout / HTTP error)  → exit 0 with warn, log to skip-log
#   - curl success, count == 0                                   → exit 1 (alert fires)
#   - curl success, count > 0                                    → exit 0 (ok)
# Overrides (for testing):
#   FUSEKI_URL       — SPARQL endpoint (default: http://localhost:3030/pods/query)
#   FUSEKI_SKIP_LOG  — if set, append one line per curl-failure skip

set -u

FUSEKI_URL="${FUSEKI_URL:-http://localhost:3030/pods/query}"
QUERY='PREFIX jb: <https://jeffbridwell.com/ontology#> SELECT (COUNT(*) AS ?c) WHERE { GRAPH ?g { ?s jb:dateTaken ?d } FILTER(STRSTARTS(STR(?g), '"'"'urn:gathering:photos'"'"')) }'

RESULT=$(curl -sf --max-time 10 "$FUSEKI_URL" \
  --data-urlencode "query=${QUERY}" \
  -H "Accept: application/json" 2>/dev/null)
CURL_RC=$?

if [ "$CURL_RC" -ne 0 ]; then
  ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  msg="fuseki-unreachable: curl_rc=${CURL_RC} url=${FUSEKI_URL}"
  echo "warn: ${msg} — skip (no alert fired)"
  if [ -n "${FUSEKI_SKIP_LOG:-}" ]; then
    printf '%s %s\n' "$ts" "$msg" >> "$FUSEKI_SKIP_LOG"
  fi
  exit 0
fi

COUNT=$(printf '%s' "$RESULT" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d['results']['bindings'][0]['c']['value'])" 2>/dev/null)

if [ -z "${COUNT:-}" ]; then
  ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  msg="fuseki-malformed: unable to parse count from response"
  echo "warn: ${msg} — skip (no alert fired)"
  if [ -n "${FUSEKI_SKIP_LOG:-}" ]; then
    printf '%s %s\n' "$ts" "$msg" >> "$FUSEKI_SKIP_LOG"
  fi
  exit 0
fi

if [ "$COUNT" -eq 0 ]; then
  # #3709 — RECORD THE FIRE. Both skip paths above have always written evidence;
  # the one path that actually wakes a human wrote none. So a recurring zero was
  # undiagnosable: by the time anyone looked the query answered non-zero and
  # there was nothing to compare against, and the alarm got dismissed as noise
  # for months. Log the observation and the raw response.
  ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  if [ -n "${FUSEKI_SKIP_LOG:-}" ]; then
    printf '%s fuseki-harvest-empty: count=0 url=%s response=%s\n' \
      "$ts" "$FUSEKI_URL" \
      "$(printf '%s' "$RESULT" | tr -d '\n' | cut -c1-400)" >> "$FUSEKI_SKIP_LOG"
  fi
  echo "fuseki-harvest-empty: 0 photos in urn:gathering:photos* graphs (url=${FUSEKI_URL} observed=${ts})"
  exit 1
fi

echo "ok: ${COUNT} photos in Fuseki"
exit 0
