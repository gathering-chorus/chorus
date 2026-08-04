#!/usr/bin/env bash
# chorus-model-deploy.sh (#3509) — deploy the MODEL (chorus.ttl schema) into Fuseki.
#
# The gap this closes: the werk pipeline deployed CODE but not the MODEL — chorus.ttl
# changes never reached urn:chorus:ontology, so the live schema went stale (merged != live;
# discovered #3509 when the live graph was found sourced from deleted v2-draft files).
#
# This is the SCHEMA deployer (urn:chorus:ontology). Since #3698 it ALSO hydrates the
# value-stream PURE-ABox instances into urn:chorus:instances via the INSTANCE_SET section
# at the tail (a SEPARATE transaction, after this one). crawler-hydrate-graph.sh hydrates
# the OTHER, filesystem-crawled instances. Two graphs — ontology (schema + punned ABox) and
# instances (authored pure ABox) — deployed by their own transactions in this script.
#
# Mechanism (#3550 — CORRECTED here: the pre-#3550 header wrongly said "single SPARQL COPY"
# and that stale line produced a false architect wipe-warning on #3698). NOT a whole-graph
# COPY/replace. Avoids #3496, needs NO Fuseki restart: POST the Turtle into a fresh staging
# graph, then ONE atomic SPARQL transaction that is a PER-SUBJECT ADDITIVE MERGE — DELETE
# from the live graph only the triples whose SUBJECT is (re)defined in staging, then INSERT
# staging; DROP staging. Co-tenants NOT in the deployed set survive by construction (the old
# blind COPY clobbered them — #3496). The ONLY destructive leg is the ontology-only
# RETIRE_ABSENT clause (default 0, gated off on TTL= partials); it is NEVER in the instances
# section. GSP PUT-replace is NOT used — it 500s (NodeTableTRDF/Read) on any existing graph
# (#3496). A restart is NOT used — a schema deploy must never disrupt the shared DB (Jeff 2026-06-19).
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
    # #3675 — service definitions enter the MODEL_SET the day they're authored,
    # never live-only (the #3587/#3593 wipe class).
    "$CHORUS_ROOT/designing/data/service-instances.ttl"
    # #3654 — the board domain (Chunk/ChunkMembership + shapes carrying the
    # uniqueWithin/uniqueGlobal annotations). Enters MODEL_SET the day authored so
    # read_shape (which queries urn:chorus:ontology) can see the shapes and the
    # retire-guard doesn't wipe the live-only domain (#3587/#3593 wipe class).
    "$CHORUS_ROOT/roles/wren/ontology/board-3654.ttl"
    # #3686 — role-level hard priorities: rolePriority (Role, uniqueGlobal) +
    # ownerSequence (Product/Domain, uniqueWithin ownedBy) as ADDITIVE shapes.
    # Same day-authored MODEL_SET discipline as #3654.
    "$CHORUS_ROOT/roles/wren/ontology/priorities-3686.ttl"
    # #3726 — the security SCHEMA + surfaces enter MODEL_SET so a fresh load
    # rebuilds them instead of leaving them live-only (the #3587 wipe class, and
    # why the envelope surface-table must survive a reload). security-model-3618
    # carries the Principal/APISurface/Credential class defs + shapes + the three
    # worker Principal individuals (ABox-in-ontology, faithful to live). The five
    # surface files carry the APISurface instances the envelope reads from
    # urn:chorus:ontology. All ontology-graph resident; the Principal/scope
    # INSTANCES load into the security domain graph in the SECURITY_SET section
    # below. (The nostr credential shape+instances ride #3691.)
    "$CHORUS_ROOT/roles/silas/ontology/security-model-3618.ttl"
    "$CHORUS_ROOT/roles/silas/ontology/security-3619-surfaces.ttl"
    "$CHORUS_ROOT/roles/silas/ontology/security-3619-surfaces-cards.ttl"
    "$CHORUS_ROOT/roles/silas/ontology/security-3619-surfaces-jobs.ttl"
    "$CHORUS_ROOT/roles/silas/ontology/security-3619-surfaces-wave2.ttl"
    "$CHORUS_ROOT/roles/silas/ontology/security-3619-surfaces-final.ttl"
  )
fi
FUSEKI_GSP="${FUSEKI_GSP:-http://localhost:3030/pods/data}"
FUSEKI_QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"

# #3630 — carry the Fuseki write credential on GSP writes (empty unless
# FUSEKI_ADMIN_PASSWORD set; harmless until shiro requires auth → deploy-
# before-require). Same #3566 helper the other writers source.
source "$CHORUS_ROOT/platform/scripts/fuseki-auth.sh"
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
curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$STAGING" -o /dev/null 2>/dev/null || true
for ttl in "${MODEL_SET[@]}"; do
  code=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /tmp/chorus-model-deploy-resp.txt -w '%{http_code}' -X POST \
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
    curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$STAGING" -o /dev/null 2>/dev/null || true
    exit 1
  fi
  RETIRE_CLAUSE=" ; DELETE { GRAPH <$ONTOLOGY_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$ONTOLOGY_GRAPH> { ?s a ?t ; ?p ?o . FILTER(?t IN (<${NS_CHORUS}Domain>, <${NS_CHORUS}SubDomain>)) } FILTER NOT EXISTS { GRAPH <$STAGING> { ?s ?sp ?so } } }"
fi
MERGE_SPARQL="DELETE { GRAPH <$ONTOLOGY_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$STAGING> { ?s ?sp ?so } GRAPH <$ONTOLOGY_GRAPH> { ?s ?p ?o } } ; INSERT { GRAPH <$ONTOLOGY_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$STAGING> { ?s ?p ?o } }${RETIRE_CLAUSE}"
ccode=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /tmp/chorus-model-copy-resp.txt -w '%{http_code}' -X POST \
  -H 'Content-Type: application/sparql-update' \
  --data-binary "$MERGE_SPARQL" "$FUSEKI_UPDATE" 2>/dev/null) || ccode="000"
if [ "$ccode" != "200" ] && [ "$ccode" != "204" ]; then
  echo "chorus-model-deploy: additive merge staging->ontology failed (http $ccode)" >&2
  head -3 /tmp/chorus-model-copy-resp.txt >&2
  "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$ONTOLOGY_GRAPH" reason="merge-http-$ccode" 2>/dev/null || true
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$STAGING" -o /dev/null 2>/dev/null || true
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
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$STAGING" -o /dev/null 2>/dev/null || true
  exit 1
fi
curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$STAGING" -o /dev/null 2>/dev/null || true

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

# #3736 — single-request truth: stamp WHICH commit this model deploy came from into the
# graph itself, so "is the live model the landed model?" is one SPARQL request against the
# STORE (the landed≠live class: #3735 merged clean + board Done, live graph unchanged).
# werk-deploy's canonical leg verifies stamp == landedCommit after invoking this script.
# Fail-loud: a deploy whose stamp can't be written is NOT verified (same bar as verify-empty).
_stamp_sha="$(git -C "$CHORUS_ROOT" rev-parse HEAD 2>/dev/null || echo unknown)"
_stamp_ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
_stamp_update="DELETE WHERE { GRAPH <$ONTOLOGY_GRAPH> { <urn:chorus:model-deploy> ?p ?o } };
INSERT DATA { GRAPH <$ONTOLOGY_GRAPH> {
  <urn:chorus:model-deploy> <urn:chorus:vocab#deployedFromCommit> \"$_stamp_sha\" ;
                            <urn:chorus:vocab#deployedAt> \"$_stamp_ts\" . } }"
_scode=$(curl -s -o /dev/null -w '%{http_code}' "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" \
  -H 'Content-Type: application/sparql-update' --data-binary "$_stamp_update" "$FUSEKI_UPDATE" 2>/dev/null) || _scode="000"
case "$_scode" in
  2*) echo "chorus-model-deploy: stamped deployedFromCommit=$_stamp_sha" ;;
  *)  echo "chorus-model-deploy: STAMP WRITE FAILED (http $_scode) — deploy applied but unverifiable by commit; failing loud (#3736)" >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$ONTOLOGY_GRAPH" reason="stamp-write-failed" http="$_scode" 2>/dev/null || true
      exit 1 ;;
esac

# =============================================================================
# INSTANCE_SET (#3698, Silas-ruled) — hydrate PURE-ABox instances into
# urn:chorus:instances. SEPARATE from and AFTER the ontology transaction above.
#
# The two sections use the SAME per-subject additive merge (#3550: DELETE only the
# staged subjects' triples, then INSERT staging — NEVER a whole-graph COPY/replace).
# The ONE deliberate difference: this section carries NO retire-clause. The ontology
# section owns the RETIRE_ABSENT domain-retirement leg (#3593 — destructively DELETEs
# Domain/SubDomain subjects absent from staging); that destructive leg stays
# QUARANTINED to urn:chorus:ontology and must NEVER be expressed against the instances
# graph (it would delete every co-tenant — cards/steps/files — not in the one TTL).
# Here: purely additive, so a co-tenant can never be deleted by construction.
#
# WHY a separate graph at all: value-stream instances are PURE ABox (ADR-025 →
# urn:chorus:instances), unlike the PUNNED Domain/Service individuals (owl:Class +
# chorus:Domain, ABox-in-ontology → urn:chorus:ontology). #3705's gathering/life
# migration rides this same INSTANCE_SET, making that migration self-testing.
# Full deploys only (a TTL= partial/test deploy skips instance hydration).
# =============================================================================
if [ -z "${TTL:-}" ]; then
  INSTANCE_GRAPH="${INSTANCE_GRAPH:-urn:chorus:instances}"
  INSTANCE_STAGING="${INSTANCE_GRAPH}-staging-deploy"
  INSTANCE_SET=(
    "$CHORUS_ROOT/designing/data/value-stream-instances.ttl"
  )
  for ttl in "${INSTANCE_SET[@]}"; do
    [ -f "$ttl" ] || { echo "chorus-model-deploy: INSTANCE_SET TTL not found: $ttl" >&2; exit 1; }
    if command -v riot >/dev/null 2>&1 && ! riot --validate "$ttl" >/dev/null 2>&1; then
      echo "chorus-model-deploy: riot validate FAILED for INSTANCE_SET $ttl — NOT deploying instances" >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$INSTANCE_GRAPH" reason="riot-invalid-instance" 2>/dev/null || true
      exit 1
    fi
  done
  # Stage the INSTANCE_SET into a FRESH staging graph (GSP POST merges into staging).
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$INSTANCE_STAGING" -o /dev/null 2>/dev/null || true
  for ttl in "${INSTANCE_SET[@]}"; do
    icode=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /tmp/chorus-model-inst-resp.txt -w '%{http_code}' -X POST \
      -H 'Content-Type: text/turtle' --data-binary "@$ttl" "$FUSEKI_GSP?graph=$INSTANCE_STAGING" 2>/dev/null) || icode="000"
    if [ "$icode" != "200" ] && [ "$icode" != "201" ] && [ "$icode" != "204" ]; then
      echo "chorus-model-deploy: INSTANCE_SET staging load failed for $ttl (http $icode)" >&2
      head -3 /tmp/chorus-model-inst-resp.txt >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$INSTANCE_GRAPH" reason="instance-staging-http-$icode" 2>/dev/null || true
      curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$INSTANCE_STAGING" -o /dev/null 2>/dev/null || true
      exit 1
    fi
  done
  # Per-subject additive merge into the instances graph — NO retire clause.
  INSTANCE_MERGE="DELETE { GRAPH <$INSTANCE_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$INSTANCE_STAGING> { ?s ?sp ?so } GRAPH <$INSTANCE_GRAPH> { ?s ?p ?o } } ; INSERT { GRAPH <$INSTANCE_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$INSTANCE_STAGING> { ?s ?p ?o } }"
  imcode=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /tmp/chorus-model-inst-merge.txt -w '%{http_code}' -X POST \
    -H 'Content-Type: application/sparql-update' --data-binary "$INSTANCE_MERGE" "$FUSEKI_UPDATE" 2>/dev/null) || imcode="000"
  if [ "$imcode" != "200" ] && [ "$imcode" != "204" ]; then
    echo "chorus-model-deploy: INSTANCE_SET merge staging->instances failed (http $imcode)" >&2
    head -3 /tmp/chorus-model-inst-merge.txt >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$INSTANCE_GRAPH" reason="instance-merge-http-$imcode" 2>/dev/null || true
    curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$INSTANCE_STAGING" -o /dev/null 2>/dev/null || true
    exit 1
  fi
  # Output-verify: every staged instance subject actually landed (catches a lying-2xx).
  _imissing=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
    "query=SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <$INSTANCE_STAGING> { ?s ?p ?o } FILTER NOT EXISTS { GRAPH <$INSTANCE_GRAPH> { ?s ?q ?r } } }" \
    -H 'Accept: text/csv' 2>/dev/null | tail -1 | tr -dc '0-9')
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$INSTANCE_STAGING" -o /dev/null 2>/dev/null || true
  if [ "${_imissing:-0}" -ne 0 ] 2>/dev/null; then
    echo "chorus-model-deploy: INSTANCE-VERIFY FAILED — ${_imissing} staged subject(s) absent from <$INSTANCE_GRAPH> post-merge" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$INSTANCE_GRAPH" reason="instance-verify-missing" missing="${_imissing}" 2>/dev/null || true
    exit 1
  fi
  _in=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
    "query=SELECT (COUNT(*) AS ?n) WHERE { GRAPH <$INSTANCE_GRAPH> { ?s ?p ?o } }" \
    -H "Accept: application/sparql-results+json" 2>/dev/null \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['n']['value'])" 2>/dev/null) || _in="?"
  echo "chorus-model-deploy: hydrated ${#INSTANCE_SET[@]} instance file(s) -> <$INSTANCE_GRAPH> (http $imcode, $_in triples live)"
  "$CHORUS_LOG" model.deployed "$ROLE" graph="$INSTANCE_GRAPH" triples="${_in}" 2>/dev/null || true
fi

# =============================================================================
# SECURITY_SET (#3726) — the identity substrate: Principal instances + hasScope
# grants into urn:chorus:domains:security, the graph the owl-api door resolves
# the allow-set / holdsRole / hasScope from. Its OWN domain graph, distinct from
# both the ontology (schema) and the value-stream instances graph.
#
# WHY this section exists: before #3726 these reached the live graph ONLY via a
# one-time hand-run migration (security-3618-migrate.sh → /batch). A fresh Fuseki
# load came up with NO identity — every door then refused every request, locking
# out all three roles until someone re-ran the migration by hand. Binding the
# TTLs here makes a reload reproduce the allow-set automatically.
#
# SAFE BY CONSTRUCTION: same per-subject ADDITIVE merge as INSTANCE_SET (DELETE
# only the staged subjects' triples, then INSERT staging). NO retire clause — a
# co-tenant of the security graph can never be deleted. Full deploys only.
# The security SCHEMA (class defs, shapes, surfaces, worker principals) rides
# MODEL_SET into urn:chorus:ontology above; this section is the ABox identity
# instances only. (Nostr credential shape+instances ride #3691.)
# =============================================================================
if [ -z "${TTL:-}" ]; then
  SECURITY_GRAPH="${SECURITY_GRAPH:-urn:chorus:domains:security}"
  SECURITY_STAGING="${SECURITY_GRAPH}-staging-deploy"
  SECURITY_SET=(
    "$CHORUS_ROOT/roles/silas/ontology/identity-principals-3613.ttl"
    "$CHORUS_ROOT/roles/silas/ontology/security-scopes-3689.ttl"
  )
  for ttl in "${SECURITY_SET[@]}"; do
    [ -f "$ttl" ] || { echo "chorus-model-deploy: SECURITY_SET TTL not found: $ttl" >&2; exit 1; }
    if command -v riot >/dev/null 2>&1 && ! riot --validate "$ttl" >/dev/null 2>&1; then
      echo "chorus-model-deploy: riot validate FAILED for SECURITY_SET $ttl — NOT deploying identity" >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$SECURITY_GRAPH" reason="riot-invalid-security" 2>/dev/null || true
      exit 1
    fi
  done
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$SECURITY_STAGING" -o /dev/null 2>/dev/null || true
  for ttl in "${SECURITY_SET[@]}"; do
    scode=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /tmp/chorus-model-sec-resp.txt -w '%{http_code}' -X POST \
      -H 'Content-Type: text/turtle' --data-binary "@$ttl" "$FUSEKI_GSP?graph=$SECURITY_STAGING" 2>/dev/null) || scode="000"
    if [ "$scode" != "200" ] && [ "$scode" != "201" ] && [ "$scode" != "204" ]; then
      echo "chorus-model-deploy: SECURITY_SET staging load failed for $ttl (http $scode)" >&2
      head -3 /tmp/chorus-model-sec-resp.txt >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$SECURITY_GRAPH" reason="security-staging-http-$scode" 2>/dev/null || true
      curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$SECURITY_STAGING" -o /dev/null 2>/dev/null || true
      exit 1
    fi
  done
  SECURITY_MERGE="DELETE { GRAPH <$SECURITY_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$SECURITY_STAGING> { ?s ?sp ?so } GRAPH <$SECURITY_GRAPH> { ?s ?p ?o } } ; INSERT { GRAPH <$SECURITY_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$SECURITY_STAGING> { ?s ?p ?o } }"
  smcode=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /tmp/chorus-model-sec-merge.txt -w '%{http_code}' -X POST \
    -H 'Content-Type: application/sparql-update' --data-binary "$SECURITY_MERGE" "$FUSEKI_UPDATE" 2>/dev/null) || smcode="000"
  if [ "$smcode" != "200" ] && [ "$smcode" != "204" ]; then
    echo "chorus-model-deploy: SECURITY_SET merge staging->security failed (http $smcode)" >&2
    head -3 /tmp/chorus-model-sec-merge.txt >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$SECURITY_GRAPH" reason="security-merge-http-$smcode" 2>/dev/null || true
    curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$SECURITY_STAGING" -o /dev/null 2>/dev/null || true
    exit 1
  fi
  # #3726 — SINGLE-REQUEST TRUTH: the verify must distinguish "0 subjects missing"
  # from "could not ask". A bare `| tr -dc 0-9` with `:-0` makes an empty/failed
  # response read as 0-missing = PASS — the could-not-ask-reads-as-success class
  # (the same defect as the #3536 guard at :174/:274, carded separately). We
  # require the CSV to carry its header (?n) AND a numeric row; absent either, the
  # store did not answer THIS query and we fail-closed rather than pass blind.
  _sresp=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
    "query=SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <$SECURITY_STAGING> { ?s ?p ?o } FILTER NOT EXISTS { GRAPH <$SECURITY_GRAPH> { ?s ?q ?r } } }" \
    -H 'Accept: text/csv' 2>/dev/null)
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$SECURITY_STAGING" -o /dev/null 2>/dev/null || true
  if ! printf '%s' "$_sresp" | head -1 | grep -q '^n'; then
    echo "chorus-model-deploy: SECURITY-VERIFY could not ask (no CSV header) — refusing to pass a blind verify (#3726 single-request-truth)" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$SECURITY_GRAPH" reason="security-verify-unanswered" 2>/dev/null || true
    exit 1
  fi
  _smissing=$(printf '%s\n' "$_sresp" | tail -1 | tr -dc '0-9')
  if [ "${_smissing:-1}" -ne 0 ] 2>/dev/null; then
    echo "chorus-model-deploy: SECURITY-VERIFY FAILED — ${_smissing:-?} staged subject(s) absent from <$SECURITY_GRAPH> post-merge" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$SECURITY_GRAPH" reason="security-verify-missing" missing="${_smissing:-unknown}" 2>/dev/null || true
    exit 1
  fi
  _sn=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
    "query=PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(DISTINCT ?p) AS ?n) WHERE { GRAPH <$SECURITY_GRAPH> { ?p a c:Principal } }" \
    -H "Accept: application/sparql-results+json" 2>/dev/null \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['n']['value'])" 2>/dev/null) || _sn="?"
  echo "chorus-model-deploy: hydrated ${#SECURITY_SET[@]} security file(s) -> <$SECURITY_GRAPH> (http $smcode, $_sn principals live)"
  "$CHORUS_LOG" model.deployed "$ROLE" graph="$SECURITY_GRAPH" principals="${_sn}" 2>/dev/null || true
fi

exit 0
