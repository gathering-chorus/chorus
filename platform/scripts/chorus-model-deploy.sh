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
# #3540 — deploy the model SET, not chorus.ttl alone. werk-domains.ttl carries the
# werk-subproduct domains incl. the tests-domain shape (Test/TestResult/pyramidLayer/
# hermeticity/covers) — it existed but was never landed. A single TTL= override still
# works (tests deploy one file in isolation to a throwaway graph).
if [ -n "${TTL:-}" ]; then
  MODEL_SET=("$TTL")
else
  MODEL_SET=(
    "$CHORUS_ROOT/roles/silas/ontology/chorus.ttl"
    "$CHORUS_ROOT/roles/kade/ontology/werk-domains.ttl"
  )
fi
FUSEKI_GSP="${FUSEKI_GSP:-http://localhost:3030/pods/data}"
FUSEKI_QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
ONTOLOGY_GRAPH="${ONTOLOGY_GRAPH:-urn:chorus:ontology}"
CHORUS_LOG="${CHORUS_LOG:-$CHORUS_ROOT/platform/scripts/chorus-log}"
ROLE="${DEPLOY_ROLE:-${CHORUS_ROLE:-system}}"

for ttl in "${MODEL_SET[@]}"; do
  [ -f "$ttl" ] || { echo "chorus-model-deploy: TTL not found: $ttl" >&2; exit 1; }
done

# Don't deploy a broken model — riot-validate every set member first.
if command -v riot >/dev/null 2>&1; then
  for ttl in "${MODEL_SET[@]}"; do
    if ! riot --validate "$ttl" >/dev/null 2>&1; then
      echo "chorus-model-deploy: riot validate FAILED for $ttl — NOT deploying" >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$ONTOLOGY_GRAPH" reason="riot-invalid" 2>/dev/null || true
      exit 1
    fi
  done
fi

FUSEKI_UPDATE="${FUSEKI_UPDATE:-http://localhost:3030/pods/update}"
STAGING="${ONTOLOGY_GRAPH}-staging-deploy"

# Step 1: load the model SET into a FRESH staging graph (native Turtle via GSP POST
# — POST merges, so set members accumulate into one staging graph). Clear any
# leftover staging from a prior aborted run first.
curl -s -X DELETE "$FUSEKI_GSP?graph=$STAGING" -o /dev/null 2>/dev/null || true
for ttl in "${MODEL_SET[@]}"; do
  code=$(curl -s -o /tmp/chorus-model-deploy-resp.txt -w '%{http_code}' -X POST \
    -H 'Content-Type: text/turtle' --data-binary "@$ttl" \
    "$FUSEKI_GSP?graph=$STAGING" 2>/dev/null) || code="000"
  if [ "$code" != "200" ] && [ "$code" != "201" ] && [ "$code" != "204" ]; then
    echo "chorus-model-deploy: staging load failed for $ttl (http $code)" >&2
    head -3 /tmp/chorus-model-deploy-resp.txt >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$ONTOLOGY_GRAPH" reason="staging-load-http-$code" 2>/dev/null || true
    exit 1
  fi
done

# Step 2 (#3550): per-domain ADDITIVE merge — NOT a whole-graph COPY/replace.
# COPY <staging> TO <ontology> drops the target graph first, which wiped live-loaded
# data NOT in the deployed TTL set (#3529 value-stream wiring — the #3496 clobber).
# Instead, in ONE transaction: DELETE from the ontology only the triples whose
# SUBJECT is (re)defined in staging, then INSERT staging. A domain deploy replaces
# only its OWN subjects' triples; every sibling domain + live-loaded instance data
# survives. No full-graph clear (dodges #3496's large-clear NodeTableTRDF failure).
MERGE_SPARQL="DELETE { GRAPH <$ONTOLOGY_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$STAGING> { ?s ?sp ?so } GRAPH <$ONTOLOGY_GRAPH> { ?s ?p ?o } } ; INSERT { GRAPH <$ONTOLOGY_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$STAGING> { ?s ?p ?o } }"
ccode=$(curl -s -o /tmp/chorus-model-copy-resp.txt -w '%{http_code}' -X POST \
  -H 'Content-Type: application/sparql-update' \
  --data-binary "$MERGE_SPARQL" "$FUSEKI_UPDATE" 2>/dev/null) || ccode="000"
curl -s -X DELETE "$FUSEKI_GSP?graph=$STAGING" -o /dev/null 2>/dev/null || true
if [ "$ccode" != "200" ] && [ "$ccode" != "204" ]; then
  echo "chorus-model-deploy: additive merge staging->ontology failed (http $ccode)" >&2
  head -3 /tmp/chorus-model-copy-resp.txt >&2
  "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$ONTOLOGY_GRAPH" reason="merge-http-$ccode" 2>/dev/null || true
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

echo "chorus-model-deploy: deployed ${#MODEL_SET[@]} model file(s) -> <$ONTOLOGY_GRAPH> (http $code, $n triples live)"
"$CHORUS_LOG" model.deployed "$ROLE" graph="$ONTOLOGY_GRAPH" triples="$n" 2>/dev/null || true
exit 0
