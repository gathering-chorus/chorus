#!/usr/bin/env bash
# chorus-model-deploy.sh (#3509) — deploy the MODEL (chorus.ttl schema) into Fuseki.
#
# The gap this closes: the werk pipeline deployed CODE but not the MODEL — chorus.ttl
# changes never reached urn:chorus:ontology, so the live schema went stale (merged != live;
# discovered #3509 when the live graph was found sourced from deleted v2-draft files).
#
# This is the SCHEMA deployer (urn:chorus:ontology) — DISTINCT from crawler-hydrate-graph.sh,
# which hydrates INSTANCES (urn:chorus:instances) from the filesystem. Two graphs, two
# deployers; this one was missing.
#
# Mechanism (avoids #3496, needs NO Fuseki restart): POST the Turtle into a fresh staging
# graph, then a single SPARQL COPY (one transaction = atomic, no empty-read window on the
# live graph) replaces the ontology graph; DROP staging. GSP PUT-replace is NOT used — it
# 500s (NodeTableTRDF/Read) on any existing graph in this Fuseki (#3496). A restart is NOT
# used — a schema deploy must never disrupt the shared DB (Jeff 2026-06-19).
# Idempotent: same chorus.ttl -> same graph. Fail-loud on any non-2xx (the model did NOT deploy).
#
# Spine: model.deployed {graph, triples} on success; model.deploy.failed {graph, reason/http}.
# Exit: 0 deployed + verified; 1 invalid model / load failed / verify failed.

set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
TTL="${TTL:-$CHORUS_ROOT/roles/silas/ontology/chorus.ttl}"
FUSEKI_GSP="${FUSEKI_GSP:-http://localhost:3030/pods/data}"
FUSEKI_QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
ONTOLOGY_GRAPH="${ONTOLOGY_GRAPH:-urn:chorus:ontology}"
CHORUS_LOG="${CHORUS_LOG:-$CHORUS_ROOT/platform/scripts/chorus-log}"
ROLE="${DEPLOY_ROLE:-${CHORUS_ROLE:-system}}"

[ -f "$TTL" ] || { echo "chorus-model-deploy: TTL not found: $TTL" >&2; exit 1; }

# Don't deploy a broken model — riot-validate first.
if command -v riot >/dev/null 2>&1; then
  if ! riot --validate "$TTL" >/dev/null 2>&1; then
    echo "chorus-model-deploy: riot validate FAILED for $TTL — NOT deploying" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$ONTOLOGY_GRAPH" reason="riot-invalid" 2>/dev/null || true
    exit 1
  fi
fi

FUSEKI_UPDATE="${FUSEKI_UPDATE:-http://localhost:3030/pods/update}"
STAGING="${ONTOLOGY_GRAPH}-staging-deploy"

# Step 1: load the model into a FRESH staging graph (native Turtle via GSP POST).
# Clear any leftover staging from a prior aborted run first.
curl -s -X DELETE "$FUSEKI_GSP?graph=$STAGING" -o /dev/null 2>/dev/null || true
code=$(curl -s -o /tmp/chorus-model-deploy-resp.txt -w '%{http_code}' -X POST \
  -H 'Content-Type: text/turtle' --data-binary "@$TTL" \
  "$FUSEKI_GSP?graph=$STAGING" 2>/dev/null) || code="000"
if [ "$code" != "200" ] && [ "$code" != "201" ] && [ "$code" != "204" ]; then
  echo "chorus-model-deploy: staging load failed (http $code)" >&2
  head -3 /tmp/chorus-model-deploy-resp.txt >&2
  "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$ONTOLOGY_GRAPH" reason="staging-load-http-$code" 2>/dev/null || true
  exit 1
fi

# Step 2: atomic replace — COPY <staging> TO <ontology> is one transaction (no
# empty-read window on the live graph). Then drop staging. NOT GSP PUT (#3496).
ccode=$(curl -s -o /tmp/chorus-model-copy-resp.txt -w '%{http_code}' -X POST \
  -H 'Content-Type: application/sparql-update' \
  --data-binary "COPY <$STAGING> TO <$ONTOLOGY_GRAPH>" "$FUSEKI_UPDATE" 2>/dev/null) || ccode="000"
curl -s -X DELETE "$FUSEKI_GSP?graph=$STAGING" -o /dev/null 2>/dev/null || true
if [ "$ccode" != "200" ] && [ "$ccode" != "204" ]; then
  echo "chorus-model-deploy: atomic COPY staging->ontology failed (http $ccode)" >&2
  head -3 /tmp/chorus-model-copy-resp.txt >&2
  "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$ONTOLOGY_GRAPH" reason="copy-http-$ccode" 2>/dev/null || true
  exit 1
fi
code="$ccode"

# Verify it actually landed — count triples (proof, not assumption).
n=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
  "query=SELECT (COUNT(*) AS ?n) WHERE { GRAPH <$ONTOLOGY_GRAPH> { ?s ?p ?o } }" \
  -H "Accept: application/sparql-results+json" 2>/dev/null \
  | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['n']['value'])" 2>/dev/null) || n="0"
if [ -z "$n" ] || [ "$n" = "0" ]; then
  echo "chorus-model-deploy: PUT returned $code but graph <$ONTOLOGY_GRAPH> is empty — deploy NOT verified" >&2
  "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$ONTOLOGY_GRAPH" reason="verify-empty" 2>/dev/null || true
  exit 1
fi

echo "chorus-model-deploy: deployed $TTL -> <$ONTOLOGY_GRAPH> (http $code, $n triples live)"
"$CHORUS_LOG" model.deployed "$ROLE" graph="$ONTOLOGY_GRAPH" triples="$n" 2>/dev/null || true
exit 0
