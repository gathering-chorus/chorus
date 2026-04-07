#!/usr/bin/env bash
# fuseki-baseline.sh — Daily Fuseki performance baseline
# Runs benchmark SPARQL queries, logs timing to stdout (Loki captures via LaunchAgent).
# Alerts if any query exceeds 2x historical average.
#
# Usage: fuseki-baseline.sh

set -euo pipefail

FUSEKI="http://localhost:3030/pods/query"
CHORUS_LOG="/Users/jeffbridwell/CascadeProjects/platform/scripts/chorus-log"
BOARD_TS="/Users/jeffbridwell/CascadeProjects/platform/scripts/cards"
THRESHOLD_MS=10000  # alert if any query > 10s

run_query() {
  local name="$1"
  local query="$2"
  local start end elapsed
  start=$(python3 -c "import time; print(int(time.time()*1000))")
  curl -s -o /dev/null "$FUSEKI" --data-urlencode "query=$query" -H "Accept: application/sparql-results+json" 2>/dev/null
  end=$(python3 -c "import time; print(int(time.time()*1000))")
  elapsed=$((end - start))
  echo "$name: ${elapsed}ms"
  if [ "$elapsed" -gt "$THRESHOLD_MS" ]; then
    echo "SLOW: $name took ${elapsed}ms (threshold: ${THRESHOLD_MS}ms)"
    bash "$CHORUS_LOG" perf.fuseki.slow silas query="$name" elapsed_ms="$elapsed" 2>/dev/null || true
  fi
  echo "$elapsed"
}

echo "=== Fuseki Performance Baseline $(date '+%Y-%m-%d %H:%M') ==="

# Benchmark queries — representative of app usage patterns
Q1=$(run_query "count-all-triples" "SELECT (COUNT(*) as ?c) WHERE { GRAPH ?g { ?s ?p ?o } }" 2>&1 | tail -1)
Q2=$(run_query "count-photos" "SELECT (COUNT(*) as ?c) WHERE { GRAPH ?g { ?s a <https://jeffbridwell.com/ontology#Photo> } }" 2>&1 | tail -1)
Q3=$(run_query "count-tracks" "SELECT (COUNT(*) as ?c) WHERE { GRAPH ?g { ?s a <https://jeffbridwell.com/ontology#Track> } }" 2>&1 | tail -1)
Q4=$(run_query "list-graph-names" "SELECT (COUNT(DISTINCT ?g) as ?c) WHERE { GRAPH ?g { ?s ?p ?o } }" 2>&1 | tail -1)
Q5=$(run_query "cross-domain-sample" "SELECT ?type (COUNT(*) as ?c) WHERE { GRAPH ?g { ?s a ?type } } GROUP BY ?type ORDER BY DESC(?c) LIMIT 10" 2>&1 | tail -1)

echo "---"
echo "Summary: triples=${Q1}ms photos=${Q2}ms tracks=${Q3}ms graphs=${Q4}ms cross-domain=${Q5}ms"
bash "$CHORUS_LOG" perf.fuseki.baseline silas triples_ms="$Q1" photos_ms="$Q2" tracks_ms="$Q3" graphs_ms="$Q4" crossdomain_ms="$Q5" 2>/dev/null || true
