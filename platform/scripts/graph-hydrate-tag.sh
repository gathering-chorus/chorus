#!/usr/bin/env bash
# graph-hydrate-tag.sh — #3017: the scheduled graph ingest+tag cycle.
#
# This is the durable fix for "the graph re-stales to a dead tree": the graph
# hydrator was never scheduled (manual-only, ran once against a leftover werk).
# This wrapper runs the full ingest+tag pipeline against CANONICAL, in order,
# and is what the com.chorus.graph-hydrate LaunchAgent invokes on a schedule.
#
# Pipeline:
#   1. ingest   — crawler-hydrate-graph.sh (chorus:File instances from canonical)
#   2. reconcile— drop File instances hydrated from /chorus-werk/ (transient /
#                 retired werks; canonical is the source of truth). Pragmatic
#                 stand-in for #2827 §D deletes-reconciliation.
#   3. tag      — enrichment-write-fileInDomain.sh   (domain-radius / belongs-to)
#               — enrichment-write-fileDependsOn.sh  (blast-radius / depends-on)
#
# Always pins CHORUS_ROOT to canonical so the graph mirrors the real tree, not
# whatever werk a session happened to run from (the root cause of the stale graph).
#
# Spine event: graph.hydrate.cycle {phase, ok, duration_ms}

set -uo pipefail

CHORUS_ROOT="/Users/jeffbridwell/CascadeProjects/chorus"
SCRIPTS="$CHORUS_ROOT/platform/scripts"
FUSEKI_UPDATE="${FUSEKI_UPDATE:-http://localhost:3030/pods/update}"
HYDRATION_GRAPH="${HYDRATION_GRAPH:-urn:chorus:instances}"
CHORUS_LOG="$SCRIPTS/chorus-log"
ROLE="${DEPLOY_ROLE:-system}"

start_ts=$(python3 -c 'import time; print(int(time.time()*1000))')

log_phase() { "$CHORUS_LOG" graph.hydrate.cycle "$ROLE" phase="$1" ok="$2" 2>/dev/null || true; }

# 1. ingest (canonical)
if CHORUS_ROOT="$CHORUS_ROOT" DEPLOY_ROLE="$ROLE" bash "$SCRIPTS/crawler-hydrate-graph.sh"; then
  log_phase ingest true
else
  log_phase ingest false
fi

# 2. reconcile — drop werk-path File instances (stale leftovers; canonical only)
rc=$(curl -s -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/sparql-update' \
  --data-binary 'PREFIX chorus: <https://jeffbridwell.com/chorus#>
  DELETE { GRAPH <'"$HYDRATION_GRAPH"'> { ?f ?p ?o } }
  WHERE  { GRAPH <'"$HYDRATION_GRAPH"'> { ?f a chorus:File ; chorus:filePath ?fp .
           FILTER(CONTAINS(STR(?fp),"/chorus-werk/")) . ?f ?p ?o } }' \
  "$FUSEKI_UPDATE" 2>/dev/null || echo "000")
[ "$rc" = "200" ] || [ "$rc" = "204" ] && log_phase reconcile true || log_phase reconcile false

# 3. tag — domain-radius then blast-radius
CHORUS_ROOT="$CHORUS_ROOT" DEPLOY_ROLE="$ROLE" bash "$SCRIPTS/enrichment-write-fileInDomain.sh"  && log_phase tag-belongs true  || log_phase tag-belongs false
CHORUS_ROOT="$CHORUS_ROOT" DEPLOY_ROLE="$ROLE" bash "$SCRIPTS/enrichment-write-fileDependsOn.sh" && log_phase tag-depends true || log_phase tag-depends false

end_ts=$(python3 -c 'import time; print(int(time.time()*1000))')
"$CHORUS_LOG" graph.hydrate.cycle "$ROLE" phase=complete ok=true duration_ms=$((end_ts - start_ts)) 2>/dev/null || true
echo "graph-hydrate-tag: cycle complete ($((end_ts - start_ts))ms)"
