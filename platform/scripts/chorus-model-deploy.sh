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
#
# #3536 cross-links (AC6): #3535 (ADR-linter — the post-deploy VERIFY gate; complements this
# WRITE-side safety, not yet built), #3517 (the binary-deploy atomic+verify analog), and this
# script IS what #3536 hardens — the old blind GSP PUT that clobbered co-tenants is replaced by
# the additive, non-truncating, output-verified merge below.

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
    # #3593 — the 34 V2 domains were LIVE-ONLY (materialized into the graph, not in any
    # deployed file), so a default deploy that made one a staging-subject wiped its decl
    # (the #3587 security incident). Bring their sources INTO the MODEL_SET so a
    # default deploy re-asserts them instead of wiping — AND so the retire-subject step
    # below has the FULL domain set in staging (its safety precondition).
    "$CHORUS_ROOT/roles/wren/ontology/domains-wren-silas.ttl"
    "$CHORUS_ROOT/roles/kade/ontology/domains-kade-3581.ttl"
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
# #3593 retire-subject: on a FULL deploy (the complete MODEL_SET is in staging), also
# DELETE ontology subjects typed chorus:Domain/SubDomain that are ABSENT from staging —
# i.e. domains removed from the model (the strangler-fig RETIRE leg; #3587 left a phantom
# because the additive merge alone never deletes a removed subject). SAFETY: correct ONLY
# when staging holds ALL domains, so it is GATED OFF for TTL= partial deploys (a single-
# file staging would mark every OTHER domain "absent" → mass delete). RETIRE_ABSENT
# defaults to 1 on a full deploy (TTL unset), 0 on a TTL= override; tests force it with a
# throwaway ONTOLOGY_GRAPH. Appended to the SAME transaction (staging still exists here).
# #3536: KILL the truncate-default. Retire (destructive domain-delete) is now explicit
# opt-in (RETIRE_ABSENT=1), never the default — the old "default 1 on full deploy" was the
# 06-26 graph-wipe root (a default deploy whose staging lacked the 34 live domains retired
# them all). Deploys never truncate by default; when retire IS opted in, the union≥live
# guard below refuses on thin staging. ("stop truncating our data" — Jeff, 2026-06-30.)
RETIRE_ABSENT="${RETIRE_ABSENT:-0}"
NS_CHORUS="https://jeffbridwell.com/chorus#"
RETIRE_CLAUSE=""
if [ "$RETIRE_ABSENT" = "1" ]; then
  # #3536 empty-staging guard: retire DELETEs live domain subjects ABSENT from staging.
  # If staging holds ZERO domains, retire would delete EVERY live domain — the catastrophic
  # wipe. Refuse fail-loud. (A count vs LIVE can't be used: retiring N domains legitimately
  # makes staging = live-N, so "staging < live" wrongly blocks all retirement — TDD caught
  # that. Load failures are already caught above; MODEL_SET completeness is the #3593 fix;
  # this is the last-resort backstop against a 0-domain staging ever driving a total wipe.)
  _stag=$(curl -s "$FUSEKI_QUERY" --data-urlencode "query=PREFIX c: <${NS_CHORUS}> SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <$STAGING> { ?s a ?t . FILTER(?t IN (c:Domain, c:SubDomain)) } }" -H 'Accept: text/csv' 2>/dev/null | tail -1 | tr -dc '0-9')
  if [ "${_stag:-0}" -eq 0 ]; then
    echo "chorus-model-deploy: REFUSING retire — staging has 0 domain subjects (empty/incomplete staging would delete ALL live domains; #3536 guard)" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$ONTOLOGY_GRAPH" reason="retire-guard-empty-staging" staging=0 2>/dev/null || true
    curl -s -X DELETE "$FUSEKI_GSP?graph=$STAGING" -o /dev/null 2>/dev/null || true
    exit 1
  fi
  RETIRE_CLAUSE=" ; DELETE { GRAPH <$ONTOLOGY_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$ONTOLOGY_GRAPH> { ?s a ?t ; ?p ?o . FILTER(?t IN (<${NS_CHORUS}Domain>, <${NS_CHORUS}SubDomain>)) } FILTER NOT EXISTS { GRAPH <$STAGING> { ?s ?sp ?so } } }"
fi
MERGE_SPARQL="DELETE { GRAPH <$ONTOLOGY_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$STAGING> { ?s ?sp ?so } GRAPH <$ONTOLOGY_GRAPH> { ?s ?p ?o } } ; INSERT { GRAPH <$ONTOLOGY_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$STAGING> { ?s ?p ?o } }${RETIRE_CLAUSE}"
ccode=$(curl -s -o /tmp/chorus-model-copy-resp.txt -w '%{http_code}' -X POST \
  -H 'Content-Type: application/sparql-update' \
  --data-binary "$MERGE_SPARQL" "$FUSEKI_UPDATE" 2>/dev/null) || ccode="000"
if [ "$ccode" != "200" ] && [ "$ccode" != "204" ]; then
  echo "chorus-model-deploy: additive merge staging->ontology failed (http $ccode)" >&2
  head -3 /tmp/chorus-model-copy-resp.txt >&2
  "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$ONTOLOGY_GRAPH" reason="merge-http-$ccode" 2>/dev/null || true
  curl -s -X DELETE "$FUSEKI_GSP?graph=$STAGING" -o /dev/null 2>/dev/null || true
  exit 1
fi
code="$ccode"

# #3536 AC2 OUTPUT VERIFICATION (postcondition — staging still present, fail-loud at the source).
# Assert every staged subject actually landed in the ontology — catches a lying-2xx / partial
# INSERT. Since the merge re-inserts ALL staging triples for staged subjects, a staged subject
# present post-merge carries its triples — so this IS the "expected classes + shapes present
# post-deploy" check (e.g. DomainShape keeps its sh:property — the #3536 06-20 wipe class).
# Co-tenant preservation / "no unexpected deletion" is STRUCTURAL, not runtime-checked: the
# additive merge only DELETEs staged subjects, so non-staged subjects are untouched by
# construction (a runtime co-tenant diff would be dead code). SHACL input-validation is the
# remaining AC2 gap, gated on a SHACL tool — tracked on the card, not faked here.
_missing=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
  "query=SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <$STAGING> { ?s ?p ?o } FILTER NOT EXISTS { GRAPH <$ONTOLOGY_GRAPH> { ?s ?q ?r } } }" \
  -H 'Accept: text/csv' 2>/dev/null | tail -1 | tr -dc '0-9')
if [ "${_missing:-0}" -ne 0 ] 2>/dev/null; then
  echo "chorus-model-deploy: OUTPUT-VERIFY FAILED — ${_missing} staged subject(s) absent from <$ONTOLOGY_GRAPH> post-merge (INSERT dropped data; #3536 AC2)" >&2
  "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$ONTOLOGY_GRAPH" reason="verify-staged-missing" missing="${_missing}" 2>/dev/null || true
  curl -s -X DELETE "$FUSEKI_GSP?graph=$STAGING" -o /dev/null 2>/dev/null || true
  exit 1
fi
curl -s -X DELETE "$FUSEKI_GSP?graph=$STAGING" -o /dev/null 2>/dev/null || true

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

# #3536 AC2 — SHACL REPORT (report-only, NEVER a gate). Wren's ruling 2026-07-02:
# validate the deployed model against its OWN V2 shapes (chorus.ttl DomainShape/ServiceShape/…)
# and EMIT the violation count as an info signal — a migration-progress number (N→0 as domains
# get authored), NOT a deploy gate. A hard gate would refuse every deploy (the model is mid-
# migration). Deploy-SAFETY is the non-truncation/empty-staging guard above (data-COMPLETENESS
# here is a separate concern). shapes.ttl (V1: SubProduct/SubDomain/CatalogDoc) is deliberately
# ignored — validating V2 data against V1 shapes is meaningless noise. Full deploys only
# (TTL= partial/test deploys skip it — keeps the bats suite fast; the report is for real deploys).
if [ -z "${TTL:-}" ] && command -v shacl >/dev/null 2>&1; then
  _v2shapes="$CHORUS_ROOT/roles/silas/ontology/chorus.ttl"
  _union="$(mktemp)"; cat "${MODEL_SET[@]}" > "$_union" 2>/dev/null
  _shacl_n=$(shacl validate --shapes "$_v2shapes" --data "$_union" 2>/dev/null | grep -c 'sh:resultSeverity' 2>/dev/null || echo 0)
  rm -f "$_union"
  echo "chorus-model-deploy: SHACL report (V2 shapes, non-gating) — ${_shacl_n} violation(s) [migration-progress signal, not a gate]"
  "$CHORUS_LOG" model.deploy.shacl "$ROLE" graph="$ONTOLOGY_GRAPH" violations="${_shacl_n:-0}" gating=false 2>/dev/null || true
fi

echo "chorus-model-deploy: deployed ${#MODEL_SET[@]} model file(s) -> <$ONTOLOGY_GRAPH> (http $code, $n triples live)"
"$CHORUS_LOG" model.deployed "$ROLE" graph="$ONTOLOGY_GRAPH" triples="$n" 2>/dev/null || true
exit 0
