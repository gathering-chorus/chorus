#!/usr/bin/env bash
# athena-deploy-model.sh (#3509) — deploy the MODEL (chorus.ttl schema) into Fuseki.
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
    # #4010 — service-instances.ttl LEFT the MODEL_SET. It used to load Service
    # ABox into urn:chorus:ontology (#3675). That graph is DBA-only on the write
    # side (athena-model:916), so parking instances there made every Service
    # unwritable through the generated API. They now hydrate into
    # urn:chorus:domains:services in the SERVICES_SET section below — the same
    # shape Principle/Value/Practice/Document already use.
    # #3991 (closes the #3916 red) — product-instances.ttl is THE definition home
    # for the child products (#3545/#3603) but was never in MODEL_SET, so the store
    # held product-* with chorus.ttl's structural edges and NO types: untyped-
    # subjects-with-edges, red on products-3603 test 7 for weeks. Same
    # landed-but-never-deployed class as governance-checks (#3881).
    "$CHORUS_ROOT/designing/data/product-instances.ttl"
    # #3654 — the board domain (Chunk/ChunkMembership + shapes carrying the
    # uniqueWithin/uniqueGlobal annotations). Enters MODEL_SET the day authored so
    # read_shape (which queries urn:chorus:ontology) can see the shapes and the
    # retire-guard doesn't wipe the live-only domain (#3587/#3593 wipe class).
    "$CHORUS_ROOT/roles/wren/ontology/board-3654.ttl"
    # #3686 — role-level hard priorities: rolePriority (Role, uniqueGlobal) +
    # ownerSequence (Product/Domain, uniqueWithin ownedBy) as ADDITIVE shapes.
    # Same day-authored MODEL_SET discipline as #3654.
    "$CHORUS_ROOT/roles/wren/ontology/priorities-3686.ttl"
    # #3881 — the ADR-058 GovernanceCheck registry (#3846). Landed in the repo
    # but never deployed (not in MODEL_SET = live-only-in-reverse); caught by
    # athena-validate's own empty-registry warn on day one. Same day-authored
    # MODEL_SET discipline as #3654/#3686.
    "$CHORUS_ROOT/roles/silas/ontology/governance-checks-3846.ttl"
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
    # #3691 — the Nostr-credential shape enters MODEL_SET. The file existed and
    # 5 credentials were live, but the shape was never deployed, so nothing
    # enforced it (the DECLARED-but-not-SERVED gap the 2026-08-02 audit named).
    "$CHORUS_ROOT/roles/silas/ontology/nostr-credential-shape-3691.ttl"
    # #3733 — the graph-status registry: which graphs are sanctioned model
    # content is model DATA, queried by reconcile (allowlist retired).
    "$CHORUS_ROOT/roles/silas/ontology/graph-status-3733.ttl"
    # #3749 — the loom model's serving layer: PrincipleShape (first of the trio),
    # minted through athena-model's TBox verbs, never hand-edited. Day-authored
    # MODEL_SET discipline (#3654/#3675/#3686): in the manifest before anything
    # is written to it, so nothing here is ever live-only.
    "$CHORUS_ROOT/roles/wren/ontology/principles-3749.ttl"
    # #4006 — Value: the level the principles answer to. The shape enters
    # MODEL_SET the day it is authored, never live-only (the #3587/#3593 wipe
    # class), and athena-make will not serve the class without it.
    "$CHORUS_ROOT/roles/wren/ontology/values-shape-4006.ttl"
    # #3754 — leg 3 of the loom quartet: PracticeShape. Same day-authored
    # MODEL_SET discipline as #3749 — in the manifest before anything is
    # written to it, so the shape is never live-only. The constraint it
    # carries (expresses sh:class chorus:Principle, minCount 1) is what turns
    # a dangling practice→principle edge into a validation failure instead of
    # a silent lie; practice v1 retires in the same land (40 staged subjects).
    "$CHORUS_ROOT/roles/kade/ontology/practices-3754.ttl"
    # #3902 — the vocabulary's semver projection (GENERATED by athena-model's
    # pen; ledger at designing/schemas/model-version-ledger.jsonl is the home).
    # In MODEL_SET so the STORE carries chorus:vocabVersion and athena-make serves it.
    "$CHORUS_ROOT/designing/data/model-version.ttl"
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
  [ -f "$ttl" ] || { echo "athena-deploy-model: TTL not found: $ttl" >&2; exit 1; }
done

# Don't deploy a broken model — riot-validate every set member first.
# #3731 — ABSENT riot used to mean "every .ttl deploys unvalidated, output
# identical to a clean run" (fail-open hole 3). Now: refuse loudly, with an
# EXPLICIT escape (ALLOW_UNVALIDATED=1) whose output is unmistakably not a
# clean run. RIOT_BIN/SHACL_BIN are test seams so the absent state is drivable.
RIOT_BIN="${RIOT_BIN:-riot}"
SHACL_BIN="${SHACL_BIN:-shacl}"
if ! command -v "$RIOT_BIN" >/dev/null 2>&1; then
  if [ "${ALLOW_UNVALIDATED:-0}" = "1" ]; then
    echo "athena-deploy-model: WARNING — riot NOT INSTALLED, deploying UNVALIDATED TTL (ALLOW_UNVALIDATED=1 set; #3731). This is not a clean run." >&2
    "$CHORUS_LOG" model.deploy.unvalidated "$ROLE" graph="$ONTOLOGY_GRAPH" reason="riot-absent-allowed" 2>/dev/null || true
  else
    echo "athena-deploy-model: REFUSING — riot (Jena) not installed; cannot validate the model before deploy. Install jena, or set ALLOW_UNVALIDATED=1 to proceed loudly (#3731 fail-closed)." >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$ONTOLOGY_GRAPH" reason="riot-absent" 2>/dev/null || true
    exit 1
  fi
else
  for ttl in "${MODEL_SET[@]}"; do
    if ! "$RIOT_BIN" --validate "$ttl" >/dev/null 2>&1; then
      echo "athena-deploy-model: riot validate FAILED for $ttl — NOT deploying" >&2
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
  code=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /tmp/athena-deploy-model-resp.txt -w '%{http_code}' -X POST \
    -H 'Content-Type: text/turtle' --data-binary "@$ttl" \
    "$FUSEKI_GSP?graph=$STAGING" 2>/dev/null) || code="000"
  if [ "$code" != "200" ] && [ "$code" != "201" ] && [ "$code" != "204" ]; then
    echo "athena-deploy-model: staging load failed for $ttl (http $code)" >&2
    head -3 /tmp/athena-deploy-model-resp.txt >&2
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
    echo "athena-deploy-model: REFUSING retire — staging has 0 domain subjects (empty/incomplete staging would delete ALL live domains; #3536 guard)" >&2
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
  echo "athena-deploy-model: additive merge staging->ontology failed (http $ccode)" >&2
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
# #3731 — CSV-header witness (the #3726 single-request-truth pattern): "0
# missing" and "could not ask" were the same value here — a dead query endpoint
# made this verify PASS blind. Require the header + numeric row; else fail-closed.
_vresp=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
  "query=SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <$STAGING> { ?s ?p ?o } FILTER NOT EXISTS { GRAPH <$ONTOLOGY_GRAPH> { ?s ?q ?r } } }" \
  -H 'Accept: text/csv' 2>/dev/null)
if ! printf '%s' "$_vresp" | head -1 | grep -q '^n'; then
  echo "athena-deploy-model: OUTPUT-VERIFY could not ask (no CSV header from the store) — refusing to pass a blind verify (#3731)" >&2
  "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$ONTOLOGY_GRAPH" reason="verify-unanswered" 2>/dev/null || true
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$STAGING" -o /dev/null 2>/dev/null || true
  exit 1
fi
_missing=$(printf '%s\n' "$_vresp" | tail -1 | tr -dc '0-9')
if [ "${_missing:-1}" -ne 0 ] 2>/dev/null; then
  echo "athena-deploy-model: OUTPUT-VERIFY FAILED — ${_missing} staged subject(s) absent from <$ONTOLOGY_GRAPH> post-merge (INSERT dropped data; #3536 AC2)" >&2
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
  echo "athena-deploy-model: PUT returned $code but graph <$ONTOLOGY_GRAPH> is empty — deploy NOT verified" >&2
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
# #3731 — the report stays NON-GATING (Wren's ruling stands), but its three
# states are now distinguishable: ran-clean / ran-with-violations / DID-NOT-RUN.
# A crashed validator used to report "0 violation(s)" — migration-complete-by-
# crash, the could-not-ask class.
# SHACL_REPORT=1 is a test seam: lets the bats suite drive this leg on a cheap
# TTL= partial deploy (production behavior unchanged — full deploys only).
if [ -z "${TTL:-}" ] || [ "${SHACL_REPORT:-0}" = "1" ]; then
  if command -v "$SHACL_BIN" >/dev/null 2>&1; then
    _v2shapes="$CHORUS_ROOT/roles/silas/ontology/chorus.ttl"
    _union="$(mktemp)"; cat "${MODEL_SET[@]}" > "$_union" 2>/dev/null
    if _shacl_out=$("$SHACL_BIN" validate --shapes "$_v2shapes" --data "$_union" 2>/dev/null); then
      _shacl_n=$(printf '%s' "$_shacl_out" | grep -c 'sh:resultSeverity' 2>/dev/null || true)
      echo "athena-deploy-model: SHACL report (V2 shapes, non-gating) — ${_shacl_n:-0} violation(s) [migration-progress signal, not a gate]"
      "$CHORUS_LOG" model.deploy.shacl "$ROLE" graph="$ONTOLOGY_GRAPH" violations="${_shacl_n:-0}" gating=false status=ran 2>/dev/null || true
    else
      echo "athena-deploy-model: SHACL validator CRASHED — violations UNKNOWN, not 0 (#3731; report-only, deploy continues)" >&2
      "$CHORUS_LOG" model.deploy.shacl "$ROLE" graph="$ONTOLOGY_GRAPH" violations=unknown gating=false status=crashed 2>/dev/null || true
    fi
    rm -f "$_union"
  else
    echo "athena-deploy-model: SHACL report SKIPPED — validator not installed; model deployed WITHOUT the V2-shape report (#3731; not a clean-run signal)" >&2
    "$CHORUS_LOG" model.deploy.shacl "$ROLE" graph="$ONTOLOGY_GRAPH" violations=unknown gating=false status=absent 2>/dev/null || true
  fi
fi

echo "athena-deploy-model: deployed ${#MODEL_SET[@]} model file(s) -> <$ONTOLOGY_GRAPH> (http $code, $n triples live)"
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
  2*) echo "athena-deploy-model: stamped deployedFromCommit=$_stamp_sha" ;;
  *)  echo "athena-deploy-model: STAMP WRITE FAILED (http $_scode) — deploy applied but unverifiable by commit; failing loud (#3736)" >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$ONTOLOGY_GRAPH" reason="stamp-write-failed" http="$_scode" 2>/dev/null || true
      exit 1 ;;
esac

# =============================================================================
# STAGED RETIREMENTS (#3752) — execute what `athena-model retire-claim` staged.
#
# The write boundary of the TBox retirement verb: the verb NEVER touches the
# store; it appends a JSONL entry here, and THIS section — the same governed
# deploy that merges the model — executes the delete. Two entry forms:
#   claim:   {"subject_domain","object_class",...}  → one definesVocabulary triple
#   subject: {"retire_subject":"<iri>","graph":...} → every triple of one subject
# Semantics: IDEMPOTENT (already-absent = executed, noted, never an error) and
# FAIL-CLOSED at every ask (#3731). Claim entries are re-checked against
# athena-make's LIVE served routes AT EXECUTE TIME (Wren's review flag: the verb's
# serve-check runs at stage time; a surface can come back up between staging
# and deploy — that window refuses loudly here, and an unanswerable athena-make
# refuses too, never a blind delete).
# =============================================================================
RETIREMENTS_FILE="${RETIREMENTS_FILE:-$CHORUS_ROOT/designing/schemas/model-retirements.jsonl}"
OWL_API_URL="${OWL_API_URL:-http://localhost:3360}"
if [ -f "$RETIREMENTS_FILE" ]; then
  _served_resp=$(curl -s -m 5 "$OWL_API_URL/__model_deploy_probe__" 2>/dev/null || true)
  _rline=0
  while IFS= read -r _rentry || [ -n "$_rentry" ]; do
    _rline=$((_rline + 1))
    [ -z "$_rentry" ] && continue
    _rparsed=$(printf '%s' "$_rentry" | python3 -c '
import json, sys
e = json.load(sys.stdin)
print(e.get("subject_domain",""), e.get("object_class",""), e.get("retire_subject",""), e.get("graph",""), e.get("retire_graph",""), e.get("status","staged"), sep="\x1f")' 2>/dev/null) || {
      echo "athena-deploy-model: RETIREMENTS line $_rline is MALFORMED — refusing the deploy (fail-closed, #3752)" >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$ONTOLOGY_GRAPH" reason="retirement-staging-malformed" line="$_rline" 2>/dev/null || true
      exit 1
    }
    # \x1f (unit separator): TAB is IFS-whitespace in bash and COLLAPSES
    # adjacent delimiters, silently shifting fields left past an empty one —
    # a claim entry became a subject retirement of its own graph name in the
    # first test run. A non-whitespace separator keeps empty fields empty.
    IFS=$'\x1f' read -r _rdom _rcls _rsubj _rgraph _rwholegraph _rstatus <<< "$_rparsed"

    # #3788 — HONOUR THE STATUS. Until this existed the deploy read every line
    # and re-executed it, so `status` was decoration: an entry that had already
    # run stayed armed forever.
    #
    # That cost us a team-wide lockout. Ten entries staged under #3773 fired at
    # 08-06 15:18 as intended; the records were restored from a verified backup;
    # and seven of them fired AGAIN at 16:19 during that card's land, removing
    # jeff, marknakib and all three agents from the allow-set the doors read.
    # Only the workers survived, staged as they were against another graph.
    # Kade found it the next morning when governed writes began refusing. No
    # instrument did.
    #
    # Idempotent-on-absent is the right posture for a FIRST run and the wrong
    # one forever after: it makes a restore and a retirement disagree about the
    # same record, and the retirement wins every deploy. Only `staged` executes.
    case "${_rstatus:-staged}" in
      staged) : ;;
      *)
        echo "athena-deploy-model: retirement line $_rline is '${_rstatus}', not staged — skipping (#3788)"
        "$CHORUS_LOG" model.retirement.skipped "$ROLE" line="$_rline" status="${_rstatus}" \
          target="${_rsubj:-${_rwholegraph:-claim}}" 2>/dev/null || true
        continue
        ;;
    esac

    _rg="${_rgraph:-$ONTOLOGY_GRAPH}"
    if [ -n "$_rwholegraph" ]; then
      # WHOLE-GRAPH retirement (#3732): the ADR-051 Addendum II case — a graph
      # whose every subject is a retired duplicate (blank-node property lists
      # make per-subject retirement lossy). BACKUP IS MANDATORY AND VERIFIED
      # BEFORE THE DROP: never practice destructive ops on live without a
      # restore path (2026-05-30 lesson, in the memory index by name).
      _bkdir="${GRAPH_BACKUP_DIR:-${CHORUS_ROOT}/platform/backups/graph-retirements}"
      mkdir -p "$_bkdir" 2>/dev/null || true
      _bk="$_bkdir/$(printf '%s' "$_rwholegraph" | tr ':/' '__')-$(date -u +%Y%m%dT%H%M%SZ).nt"
      # CONSTRUCT via the query endpoint, NOT a GSP GET: GSP fetch 500s on this
      # store ("Failed to write output: NodeTableTRDF/Read" — the #3496 bug
      # family), which produced a 1-line error file that the size check below
      # correctly refused to accept as a backup. Found by that check, not by
      # reading the code.
      curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" "$FUSEKI_QUERY" \
        --data-urlencode "query=CONSTRUCT { ?s ?p ?o } WHERE { GRAPH <$_rwholegraph> { ?s ?p ?o } }" \
        -H 'Accept: application/n-triples' -o "$_bk" 2>/dev/null || true
      _bklines=$(grep -c . "$_bk" 2>/dev/null || echo 0)
      _livecount=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
        "query=SELECT (COUNT(*) AS ?n) WHERE { GRAPH <$_rwholegraph> { ?s ?p ?o } }" \
        -H 'Accept: text/csv' 2>/dev/null | tail -1 | tr -dc '0-9')
      if [ -z "$_livecount" ]; then
        echo "athena-deploy-model: GRAPH RETIREMENT could not count <$_rwholegraph> — refusing a blind drop (#3732)" >&2
        "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$_rwholegraph" reason="graph-retire-count-unanswered" 2>/dev/null || true
        exit 1
      fi
      if [ "$_livecount" = "0" ]; then
        echo "athena-deploy-model: graph <$_rwholegraph> already empty (idempotent — previously retired)"
        continue
      fi
      if [ "${_bklines:-0}" -lt "$_livecount" ]; then
        echo "athena-deploy-model: GRAPH RETIREMENT REFUSED — backup has ${_bklines:-0} lines for $_livecount live triples; no verified restore path, NOT dropping <$_rwholegraph> (#3732)" >&2
        "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$_rwholegraph" reason="graph-retire-backup-incomplete" backup="$_bk" 2>/dev/null || true
        exit 1
      fi
      _dcode=$(curl -s -o /dev/null -w '%{http_code}' "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" \
        -H 'Content-Type: application/sparql-update' --data-binary "DROP GRAPH <$_rwholegraph>" "$FUSEKI_UPDATE" 2>/dev/null) || _dcode="000"
      case "$_dcode" in 2*) : ;; *)
        echo "athena-deploy-model: GRAPH RETIREMENT drop failed (http $_dcode) for <$_rwholegraph>" >&2
        "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$_rwholegraph" reason="graph-retire-drop-http-$_dcode" 2>/dev/null || true
        exit 1 ;;
      esac
      _post=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
        "query=SELECT (COUNT(*) AS ?n) WHERE { GRAPH <$_rwholegraph> { ?s ?p ?o } }" \
        -H 'Accept: text/csv' 2>/dev/null | tail -1 | tr -dc '0-9')
      if [ "${_post:-1}" != "0" ]; then
        echo "athena-deploy-model: GRAPH RETIREMENT VERIFY FAILED — <$_rwholegraph> still holds ${_post:-?} triples after DROP" >&2
        "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$_rwholegraph" reason="graph-retire-still-present" 2>/dev/null || true
        exit 1
      fi
      echo "athena-deploy-model: graph retirement executed — <$_rwholegraph> dropped ($_livecount triples, backup $_bk)"
      "$CHORUS_LOG" model.retirement.executed "$ROLE" graph="$_rwholegraph" target="graph $_rwholegraph" line="$_rline" 2>/dev/null || true
      continue
    fi
    if [ -n "$_rsubj" ]; then
      _rask="ASK { GRAPH <$_rg> { <$_rsubj> ?p ?o } }"
      _rdel="DELETE WHERE { GRAPH <$_rg> { <$_rsubj> ?p ?o } }"
      _rlabel="subject $_rsubj"
    elif [ -n "$_rdom" ] && [ -n "$_rcls" ]; then
      # Serve-gate at EXECUTE time (claim entries only — a claim under a live
      # route must not retire, and an unanswerable athena-make must not blind-pass).
      if ! printf '%s' "$_served_resp" | grep -q '"served"'; then
        echo "athena-deploy-model: RETIREMENT serve-check UNANSWERED (athena-make gave no route list) — refusing to execute claim retirements blind (#3752, Wren's window)" >&2
        "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$_rg" reason="retirement-serve-check-unanswered" 2>/dev/null || true
        exit 1
      fi
      _rroute_now=$(printf '%s' "$_rcls" | python3 -c '
import sys
c = sys.stdin.read().strip().lower()
irr = {"property":"properties","propertykey":"propertykeys"}
print(irr.get(c, c[:-1]+"ies" if c.endswith("y") else c+"s"))')
      if printf '%s' "$_served_resp" | grep -q "\"/$_rroute_now\""; then
        echo "athena-deploy-model: RETIREMENT REFUSED — class $_rcls is SERVED at /$_rroute_now RIGHT NOW (surface came up since staging); unserve first (#3752)" >&2
        "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$_rg" reason="retirement-claim-served-at-execute" route="$_rroute_now" 2>/dev/null || true
        exit 1
      fi
      _rs="https://jeffbridwell.com/chorus#$_rdom"
      _ro="https://jeffbridwell.com/chorus#$_rcls"
      _rp="https://jeffbridwell.com/chorus#definesVocabulary"
      _rask="ASK { GRAPH <$_rg> { <$_rs> <$_rp> <$_ro> } }"
      _rdel="DELETE DATA { GRAPH <$_rg> { <$_rs> <$_rp> <$_ro> } }"
      _rlabel="claim ${_rdom}->${_rcls}"
    else
      echo "athena-deploy-model: RETIREMENTS line $_rline has neither a claim nor a subject — refusing (fail-closed, #3752)" >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$_rg" reason="retirement-entry-empty" line="$_rline" 2>/dev/null || true
      exit 1
    fi
    _rresp=$(curl -s "$FUSEKI_QUERY" --data-urlencode "query=$_rask" -H "Accept: application/sparql-results+json" 2>/dev/null)
    if ! printf '%s' "$_rresp" | grep -q '"boolean"'; then
      echo "athena-deploy-model: RETIREMENT pre-ask unanswered for $_rlabel — refusing a blind execute (#3731 class)" >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$_rg" reason="retirement-ask-unanswered" 2>/dev/null || true
      exit 1
    fi
    if ! printf '%s' "$_rresp" | grep -qE '"boolean"[ ]*:[ ]*true'; then
      echo "athena-deploy-model: retirement $_rlabel already absent from <$_rg> (idempotent — previously executed)"
      continue
    fi
    _rcode=$(curl -s -o /dev/null -w '%{http_code}' "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" \
      -H 'Content-Type: application/sparql-update' --data-binary "$_rdel" "$FUSEKI_UPDATE" 2>/dev/null) || _rcode="000"
    case "$_rcode" in 2*) : ;; *)
      echo "athena-deploy-model: RETIREMENT delete failed (http $_rcode) for $_rlabel" >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$_rg" reason="retirement-delete-http-$_rcode" 2>/dev/null || true
      exit 1 ;;
    esac
    _rverify=$(curl -s "$FUSEKI_QUERY" --data-urlencode "query=$_rask" -H "Accept: application/sparql-results+json" 2>/dev/null)
    if ! printf '%s' "$_rverify" | grep -q '"boolean"'; then
      echo "athena-deploy-model: RETIREMENT post-verify unanswered for $_rlabel — executed but UNVERIFIED; failing loud" >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$_rg" reason="retirement-verify-unanswered" 2>/dev/null || true
      exit 1
    fi
    if printf '%s' "$_rverify" | grep -qE '"boolean"[ ]*:[ ]*true'; then
      echo "athena-deploy-model: RETIREMENT VERIFY FAILED — $_rlabel still present in <$_rg> after delete" >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$_rg" reason="retirement-still-present" 2>/dev/null || true
      exit 1
    fi
    echo "athena-deploy-model: retirement executed — $_rlabel removed from <$_rg>"
    "$CHORUS_LOG" model.retirement.executed "$ROLE" graph="$_rg" target="$_rlabel" line="$_rline" 2>/dev/null || true
  done < "$RETIREMENTS_FILE"
else
  echo "athena-deploy-model: no retirements staged ($RETIREMENTS_FILE absent)"
fi

# =============================================================================
# INSTANCE_SET — MOVED to `athena-model seed --deploy` (#3895).
#
# #3839 put the DAL-gated instance-seed leg (governed-writer seed + identity-
# token mint) inside this script, violating the #3785 recovery invariant: this
# is the RECOVERY path and must authenticate to the STORE only — never require
# an identity token, never shell to the governed writer. (Those two names are
# deliberately not spelled out here — the #3785 guard greps this file raw, and
# a comment naming them would trip it. The 2026-08-06 lockout is why it exists.)
# The instance leg now lives in the athena-model binary (`seed --deploy`,
# manifest at platform/config/instance-seed-manifest.txt — ADR-038: no new
# deploy-path bash); werk-deploy runs this script THEN that verb at land, so
# landing still seeds instances. Recovery contexts run this script alone.
# Guarded by platform/tests/recovery-path-ungated-3785.bats.
# =============================================================================

# =============================================================================
# SECURITY_SET (#3726) — the identity substrate: Principal instances + hasScope
# grants into urn:chorus:domains:security, the graph the athena-make door resolves
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
#
# #3839 — WHY THIS LEG IS STILL A STAGED MERGE, SAID OUT LOUD.
#
# INSTANCE_SET now writes through athena-model, which shape-checks every subject.
# This leg does not, and cannot as things stand: assert_dal_writable (chorus-model
# lib.rs) refuses urn:chorus:domains:security outright as a DBA-path graph, so
# the governed writer will not touch it by design.
#
# The cost is exact and worth naming rather than leaving to be rediscovered:
# chorus:uniqueGlobal on Principal's webId is REAL and enforced in the DAL, and
# Principals travel this path, so the rule and its enforcement never meet. Two
# principals can claim the same WebID here and nothing objects — and the WebID is
# the correlation key between CSS and this graph. That is the #3838 finding.
#
# NOT fixed here on purpose: closing it means either giving the DAL a DBA mode or
# moving Principals off the DBA graph, and both are decisions about the identity
# substrate, not about a deploy script. Silas owns the Principal half. This
# comment exists so the gap is a stated position with an owner, not a silence.
# =============================================================================
if [ -z "${TTL:-}" ]; then
  SECURITY_GRAPH="${SECURITY_GRAPH:-urn:chorus:domains:security}"
  SECURITY_STAGING="${SECURITY_GRAPH}-staging-deploy"
  SECURITY_SET=(
    "$CHORUS_ROOT/roles/silas/ontology/identity-principals-3613.ttl"
    "$CHORUS_ROOT/roles/silas/ontology/security-scopes-3689.ttl"
    # #3729 — posture ABox: SecurityProbe rows mirror probes.d/ one-to-one;
    # AuthBoundary rows name the verify doors AND their bypass paths. Serve
    # reads resolve these classes to urn:chorus:domains:security (#3570), so
    # they ride the security leg, not MODEL_SET (rows in the ontology graph
    # would serve 0 — the products-incident shape of miss).
    "$CHORUS_ROOT/roles/silas/ontology/security-posture-3729.ttl"
  )
  for ttl in "${SECURITY_SET[@]}"; do
    [ -f "$ttl" ] || { echo "athena-deploy-model: SECURITY_SET TTL not found: $ttl" >&2; exit 1; }
    if command -v riot >/dev/null 2>&1 && ! riot --validate "$ttl" >/dev/null 2>&1; then
      echo "athena-deploy-model: riot validate FAILED for SECURITY_SET $ttl — NOT deploying identity" >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$SECURITY_GRAPH" reason="riot-invalid-security" 2>/dev/null || true
      exit 1
    fi
  done
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$SECURITY_STAGING" -o /dev/null 2>/dev/null || true
  for ttl in "${SECURITY_SET[@]}"; do
    scode=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /tmp/chorus-model-sec-resp.txt -w '%{http_code}' -X POST \
      -H 'Content-Type: text/turtle' --data-binary "@$ttl" "$FUSEKI_GSP?graph=$SECURITY_STAGING" 2>/dev/null) || scode="000"
    if [ "$scode" != "200" ] && [ "$scode" != "201" ] && [ "$scode" != "204" ]; then
      echo "athena-deploy-model: SECURITY_SET staging load failed for $ttl (http $scode)" >&2
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
    echo "athena-deploy-model: SECURITY_SET merge staging->security failed (http $smcode)" >&2
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
    echo "athena-deploy-model: SECURITY-VERIFY could not ask (no CSV header) — refusing to pass a blind verify (#3726 single-request-truth)" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$SECURITY_GRAPH" reason="security-verify-unanswered" 2>/dev/null || true
    exit 1
  fi
  _smissing=$(printf '%s\n' "$_sresp" | tail -1 | tr -dc '0-9')
  if [ "${_smissing:-1}" -ne 0 ] 2>/dev/null; then
    echo "athena-deploy-model: SECURITY-VERIFY FAILED — ${_smissing:-?} staged subject(s) absent from <$SECURITY_GRAPH> post-merge" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$SECURITY_GRAPH" reason="security-verify-missing" missing="${_smissing:-unknown}" 2>/dev/null || true
    exit 1
  fi
  _sn=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
    "query=PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(DISTINCT ?p) AS ?n) WHERE { GRAPH <$SECURITY_GRAPH> { ?p a c:Principal } }" \
    -H "Accept: application/sparql-results+json" 2>/dev/null \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['n']['value'])" 2>/dev/null) || _sn="?"
  echo "athena-deploy-model: hydrated ${#SECURITY_SET[@]} security file(s) -> <$SECURITY_GRAPH> (http $smcode, $_sn principals live)"
  "$CHORUS_LOG" model.deployed "$ROLE" graph="$SECURITY_GRAPH" principals="${_sn}" 2>/dev/null || true
fi

# =============================================================================
# PRINCIPLES_SET (#3749) — the 14 PC (Hemenway) principle instances into their
# ADR-051 home urn:chorus:domains:principles: the graph athena-make's
# resolve_instances_graph projects from `chorus:principles definesVocabulary
# chorus:Principle` with NO instancesGraph override. Reader = writer = one
# canonical graph — the kill of the 2-of-29 writer/reader split (the old
# /api/loom/principles read urn:chorus:instances while 27 of 29 sat in the
# ontology graph). Same SAFE-BY-CONSTRUCTION shape as SECURITY_SET: staged
# load, per-subject ADDITIVE merge (no retire clause), single-request-truth
# verify (#3726 — a blind verify fails closed, never passes).
# =============================================================================
if [ -z "${TTL:-}" ]; then
  PRINCIPLES_GRAPH="${PRINCIPLES_GRAPH:-urn:chorus:domains:principles}"
  PRINCIPLES_STAGING="${PRINCIPLES_GRAPH}-staging-deploy"
  PRINCIPLES_SET=(
    "$CHORUS_ROOT/roles/wren/ontology/principles-instances-3749.ttl"
    # #4006 — the 14 XP principles. The PC set alone was never the loom's
    # principle layer, only the half that got authored; /principles served 14
    # and nothing could bind a Practice to the XP principle it expresses.
    "$CHORUS_ROOT/roles/wren/ontology/principles-xp-4006.ttl"
    # #4006 — the PC↔XP rhymes ride the principles set: same graph, same
    # additive merge, so an edge can never outlive the principles it joins.
    "$CHORUS_ROOT/roles/wren/ontology/principles-rhymes-4006.ttl"
  )
  for ttl in "${PRINCIPLES_SET[@]}"; do
    [ -f "$ttl" ] || { echo "athena-deploy-model: PRINCIPLES_SET TTL not found: $ttl" >&2; exit 1; }
    if command -v riot >/dev/null 2>&1 && ! riot --validate "$ttl" >/dev/null 2>&1; then
      echo "athena-deploy-model: riot validate FAILED for PRINCIPLES_SET $ttl — NOT deploying principles" >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$PRINCIPLES_GRAPH" reason="riot-invalid-principles" 2>/dev/null || true
      exit 1
    fi
  done
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$PRINCIPLES_STAGING" -o /dev/null 2>/dev/null || true
  for ttl in "${PRINCIPLES_SET[@]}"; do
    pcode=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /tmp/chorus-model-prin-resp.txt -w '%{http_code}' -X POST \
      -H 'Content-Type: text/turtle' --data-binary "@$ttl" "$FUSEKI_GSP?graph=$PRINCIPLES_STAGING" 2>/dev/null) || pcode="000"
    if [ "$pcode" != "200" ] && [ "$pcode" != "201" ] && [ "$pcode" != "204" ]; then
      echo "athena-deploy-model: PRINCIPLES_SET staging load failed for $ttl (http $pcode)" >&2
      head -3 /tmp/chorus-model-prin-resp.txt >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$PRINCIPLES_GRAPH" reason="principles-staging-http-$pcode" 2>/dev/null || true
      curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$PRINCIPLES_STAGING" -o /dev/null 2>/dev/null || true
      exit 1
    fi
  done
  PRINCIPLES_MERGE="DELETE { GRAPH <$PRINCIPLES_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$PRINCIPLES_STAGING> { ?s ?sp ?so } GRAPH <$PRINCIPLES_GRAPH> { ?s ?p ?o } } ; INSERT { GRAPH <$PRINCIPLES_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$PRINCIPLES_STAGING> { ?s ?p ?o } }"
  pmcode=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /tmp/chorus-model-prin-merge.txt -w '%{http_code}' -X POST \
    -H 'Content-Type: application/sparql-update' --data-binary "$PRINCIPLES_MERGE" "$FUSEKI_UPDATE" 2>/dev/null) || pmcode="000"
  if [ "$pmcode" != "200" ] && [ "$pmcode" != "204" ]; then
    echo "athena-deploy-model: PRINCIPLES_SET merge staging->principles failed (http $pmcode)" >&2
    head -3 /tmp/chorus-model-prin-merge.txt >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$PRINCIPLES_GRAPH" reason="principles-merge-http-$pmcode" 2>/dev/null || true
    curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$PRINCIPLES_STAGING" -o /dev/null 2>/dev/null || true
    exit 1
  fi
  _presp=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
    "query=SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <$PRINCIPLES_STAGING> { ?s ?p ?o } FILTER NOT EXISTS { GRAPH <$PRINCIPLES_GRAPH> { ?s ?q ?r } } }" \
    -H 'Accept: text/csv' 2>/dev/null)
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$PRINCIPLES_STAGING" -o /dev/null 2>/dev/null || true
  if ! printf '%s' "$_presp" | head -1 | grep -q '^n'; then
    echo "athena-deploy-model: PRINCIPLES-VERIFY could not ask (no CSV header) — refusing to pass a blind verify (#3726 single-request-truth)" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$PRINCIPLES_GRAPH" reason="principles-verify-unanswered" 2>/dev/null || true
    exit 1
  fi
  _pmissing=$(printf '%s\n' "$_presp" | tail -1 | tr -dc '0-9')
  if [ "${_pmissing:-1}" -ne 0 ] 2>/dev/null; then
    echo "athena-deploy-model: PRINCIPLES-VERIFY FAILED — ${_pmissing:-?} staged subject(s) absent from <$PRINCIPLES_GRAPH> post-merge" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$PRINCIPLES_GRAPH" reason="principles-verify-missing" missing="${_pmissing:-unknown}" 2>/dev/null || true
    exit 1
  fi
  _pn=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
    "query=PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(DISTINCT ?p) AS ?n) WHERE { GRAPH <$PRINCIPLES_GRAPH> { ?p a c:Principle } }" \
    -H "Accept: application/sparql-results+json" 2>/dev/null \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['n']['value'])" 2>/dev/null) || _pn="?"
  echo "athena-deploy-model: hydrated ${#PRINCIPLES_SET[@]} principles file(s) -> <$PRINCIPLES_GRAPH> (http $pmcode, $_pn principles live)"
  "$CHORUS_LOG" model.deployed "$ROLE" graph="$PRINCIPLES_GRAPH" principles="${_pn}" 2>/dev/null || true
fi

# =============================================================================
# VALUES_SET (#4006) — the 5 XP value instances into urn:chorus:domains:values.
# Same SAFE-BY-CONSTRUCTION shape as PRINCIPLES_SET: riot-validate first, staged
# load, per-subject ADDITIVE merge, then a single-request-truth verify that
# fails CLOSED when it cannot ask (#3726 — a blind verify never passes).
#
# The extra check this section carries is the DANGLING-EDGE one. Every value
# declares expressedBy -> a Principle, and a value pointing at an IRI no
# principle occupies renders as a live edge to nothing: the /values page shows
# a link, the coverage query counts a hit, and neither is real. The deploy
# refuses rather than serve that.
# =============================================================================
if [ -z "${TTL:-}" ]; then
  VALUES_GRAPH="${VALUES_GRAPH:-urn:chorus:domains:values}"
  VALUES_STAGING="${VALUES_GRAPH}-staging-deploy"
  VALUES_SET=(
    "$CHORUS_ROOT/roles/wren/ontology/values-4006.ttl"
  )
  for ttl in "${VALUES_SET[@]}"; do
    [ -f "$ttl" ] || { echo "athena-deploy-model: VALUES_SET TTL not found: $ttl" >&2; exit 1; }
    if command -v riot >/dev/null 2>&1 && ! riot --validate "$ttl" >/dev/null 2>&1; then
      echo "athena-deploy-model: riot validate FAILED for VALUES_SET $ttl — NOT deploying values" >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$VALUES_GRAPH" reason="riot-invalid-values" 2>/dev/null || true
      exit 1
    fi
  done
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$VALUES_STAGING" -o /dev/null 2>/dev/null || true
  for ttl in "${VALUES_SET[@]}"; do
    vcode=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /tmp/chorus-model-val-resp.txt -w '%{http_code}' -X POST       -H 'Content-Type: text/turtle' --data-binary "@$ttl" "$FUSEKI_GSP?graph=$VALUES_STAGING" 2>/dev/null) || vcode="000"
    if [ "$vcode" != "200" ] && [ "$vcode" != "201" ] && [ "$vcode" != "204" ]; then
      echo "athena-deploy-model: VALUES_SET staging load failed for $ttl (http $vcode)" >&2
      head -3 /tmp/chorus-model-val-resp.txt >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$VALUES_GRAPH" reason="values-staging-http-$vcode" 2>/dev/null || true
      curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$VALUES_STAGING" -o /dev/null 2>/dev/null || true
      exit 1
    fi
  done

  # DANGLING-EDGE GATE — every expressedBy target must be a live Principle in
  # the principles graph. Asked against STAGING before the merge, so a bad edge
  # never reaches the served graph at all.
  _vdang=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
    "query=PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(DISTINCT ?t) AS ?n) WHERE { GRAPH <$VALUES_STAGING> { ?v c:expressedBy ?t } FILTER NOT EXISTS { GRAPH <$PRINCIPLES_GRAPH> { ?t a c:Principle } } }" \
    -H 'Accept: text/csv' 2>/dev/null)
  if ! printf '%s' "$_vdang" | head -1 | grep -q '^n'; then
    echo "athena-deploy-model: VALUES-EDGE-GATE could not ask (no CSV header) — refusing to pass a blind check (#3726)" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$VALUES_GRAPH" reason="values-edge-gate-unanswered" 2>/dev/null || true
    curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$VALUES_STAGING" -o /dev/null 2>/dev/null || true
    exit 1
  fi
  _vdn=$(printf '%s\n' "$_vdang" | tail -1 | tr -dc '0-9')
  if [ "${_vdn:-1}" -ne 0 ] 2>/dev/null; then
    echo "athena-deploy-model: VALUES-EDGE-GATE FAILED — ${_vdn} expressedBy target(s) are not live Principles" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$VALUES_GRAPH" reason="values-dangling-edges" dangling="${_vdn}" 2>/dev/null || true
    curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$VALUES_STAGING" -o /dev/null 2>/dev/null || true
    exit 1
  fi

  VALUES_MERGE="DELETE { GRAPH <$VALUES_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$VALUES_STAGING> { ?s ?sp ?so } GRAPH <$VALUES_GRAPH> { ?s ?p ?o } } ; INSERT { GRAPH <$VALUES_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$VALUES_STAGING> { ?s ?p ?o } }"
  vmcode=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /tmp/chorus-model-val-merge.txt -w '%{http_code}' -X POST \
    -H 'Content-Type: application/sparql-update' --data-binary "$VALUES_MERGE" "$FUSEKI_UPDATE" 2>/dev/null) || vmcode="000"
  if [ "$vmcode" != "200" ] && [ "$vmcode" != "204" ]; then
    echo "athena-deploy-model: VALUES_SET merge staging->values failed (http $vmcode)" >&2
    head -3 /tmp/chorus-model-val-merge.txt >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$VALUES_GRAPH" reason="values-merge-http-$vmcode" 2>/dev/null || true
    curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$VALUES_STAGING" -o /dev/null 2>/dev/null || true
    exit 1
  fi
  _vresp=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
    "query=SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <$VALUES_STAGING> { ?s ?p ?o } FILTER NOT EXISTS { GRAPH <$VALUES_GRAPH> { ?s ?q ?r } } }" \
    -H 'Accept: text/csv' 2>/dev/null)
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$VALUES_STAGING" -o /dev/null 2>/dev/null || true
  if ! printf '%s' "$_vresp" | head -1 | grep -q '^n'; then
    echo "athena-deploy-model: VALUES-VERIFY could not ask (no CSV header) — refusing to pass a blind verify (#3726)" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$VALUES_GRAPH" reason="values-verify-unanswered" 2>/dev/null || true
    exit 1
  fi
  _vmissing=$(printf '%s\n' "$_vresp" | tail -1 | tr -dc '0-9')
  if [ "${_vmissing:-1}" -ne 0 ] 2>/dev/null; then
    echo "athena-deploy-model: VALUES-VERIFY FAILED — ${_vmissing:-?} staged subject(s) absent from <$VALUES_GRAPH> post-merge" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$VALUES_GRAPH" reason="values-verify-missing" missing="${_vmissing:-unknown}" 2>/dev/null || true
    exit 1
  fi
  _vn=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
    "query=PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(DISTINCT ?v) AS ?n) WHERE { GRAPH <$VALUES_GRAPH> { ?v a c:Value } }" \
    -H "Accept: application/sparql-results+json" 2>/dev/null \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['n']['value'])" 2>/dev/null) || _vn="?"
  echo "athena-deploy-model: hydrated ${#VALUES_SET[@]} values file(s) -> <$VALUES_GRAPH> (http $vmcode, $_vn values live)"
  "$CHORUS_LOG" model.deployed "$ROLE" graph="$VALUES_GRAPH" values="${_vn}" 2>/dev/null || true
fi
# =============================================================================
# SERVICES_SET (#4010) — the 9 Service instances into urn:chorus:domains:services.
#
# WHY THIS SECTION EXISTS. ServiceShape used to declare the ontology graph as its
# instancesGraph, which fixed a read (/services served 0) and broke every write:
# the DAL refuses the ontology graph outright (athena-model:916, #3356 AC4), so
# `POST /services` answered 502 graph-dba-only and a service design could not be
# posted through the API that generates the service's own page.
#
# Same SAFE-BY-CONSTRUCTION shape as VALUES_SET: riot-validate first, staged
# load, per-subject ADDITIVE merge, then a verify that fails CLOSED when it
# cannot ask (#3726 — a blind verify never passes).
# =============================================================================
if [ -z "${TTL:-}" ]; then
  SERVICES_GRAPH="${SERVICES_GRAPH:-urn:chorus:domains:services}"
  SERVICES_STAGING="${SERVICES_GRAPH}-staging-deploy"
  SERVICES_SET=(
    "$CHORUS_ROOT/designing/data/service-instances.ttl"
  )
  for ttl in "${SERVICES_SET[@]}"; do
    [ -f "$ttl" ] || { echo "athena-deploy-model: SERVICES_SET TTL not found: $ttl" >&2; exit 1; }
    if command -v riot >/dev/null 2>&1 && ! riot --validate "$ttl" >/dev/null 2>&1; then
      echo "athena-deploy-model: riot validate FAILED for SERVICES_SET $ttl — NOT deploying services" >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$SERVICES_GRAPH" reason="riot-invalid-services" 2>/dev/null || true
      exit 1
    fi
  done
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$SERVICES_STAGING" -o /dev/null 2>/dev/null || true
  for ttl in "${SERVICES_SET[@]}"; do
    scode=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /tmp/chorus-model-svc-resp.txt -w '%{http_code}' -X POST -H 'Content-Type: text/turtle' --data-binary "@$ttl" "$FUSEKI_GSP?graph=$SERVICES_STAGING" 2>/dev/null) || scode="000"
    if [ "$scode" != "200" ] && [ "$scode" != "201" ] && [ "$scode" != "204" ]; then
      echo "athena-deploy-model: SERVICES_SET staging load failed for $ttl (http $scode)" >&2
      head -3 /tmp/chorus-model-svc-resp.txt >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$SERVICES_GRAPH" reason="services-staging-http-$scode" 2>/dev/null || true
      curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$SERVICES_STAGING" -o /dev/null 2>/dev/null || true
      exit 1
    fi
  done
  SERVICES_MERGE="DELETE { GRAPH <$SERVICES_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$SERVICES_STAGING> { ?s ?sp ?so } GRAPH <$SERVICES_GRAPH> { ?s ?p ?o } } ; INSERT { GRAPH <$SERVICES_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$SERVICES_STAGING> { ?s ?p ?o } }"
  smcode=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /tmp/chorus-model-svc-merge.txt -w '%{http_code}' -X POST -H 'Content-Type: application/sparql-update' --data-binary "$SERVICES_MERGE" "$FUSEKI_UPDATE" 2>/dev/null) || smcode="000"
  if [ "$smcode" != "200" ] && [ "$smcode" != "204" ]; then
    echo "athena-deploy-model: SERVICES_SET merge staging->services failed (http $smcode)" >&2
    head -3 /tmp/chorus-model-svc-merge.txt >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$SERVICES_GRAPH" reason="services-merge-http-$smcode" 2>/dev/null || true
    curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$SERVICES_STAGING" -o /dev/null 2>/dev/null || true
    exit 1
  fi
  _sresp=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
    "query=SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <$SERVICES_STAGING> { ?s ?p ?o } FILTER NOT EXISTS { GRAPH <$SERVICES_GRAPH> { ?s ?q ?r } } }" \
    -H 'Accept: text/csv' 2>/dev/null)
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$SERVICES_STAGING" -o /dev/null 2>/dev/null || true
  if ! printf '%s' "$_sresp" | head -1 | grep -q '^n'; then
    echo "athena-deploy-model: SERVICES-VERIFY could not ask (no CSV header) — refusing to pass a blind verify (#3726)" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$SERVICES_GRAPH" reason="services-verify-unanswered" 2>/dev/null || true
    exit 1
  fi
  _smissing=$(printf '%s\n' "$_sresp" | tail -1 | tr -dc '0-9')
  if [ "${_smissing:-1}" -ne 0 ] 2>/dev/null; then
    echo "athena-deploy-model: SERVICES-VERIFY FAILED — ${_smissing:-?} staged subject(s) absent from <$SERVICES_GRAPH> post-merge" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$SERVICES_GRAPH" reason="services-verify-missing" missing="${_smissing:-unknown}" 2>/dev/null || true
    exit 1
  fi
  _sn=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
    "query=PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <$SERVICES_GRAPH> { ?s a c:Service } }" \
    -H "Accept: application/sparql-results+json" 2>/dev/null \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['n']['value'])" 2>/dev/null) || _sn="?"
  echo "athena-deploy-model: hydrated ${#SERVICES_SET[@]} services file(s) -> <$SERVICES_GRAPH> (http $smcode, $_sn services live)"
  "$CHORUS_LOG" model.deployed "$ROLE" graph="$SERVICES_GRAPH" services="${_sn}" 2>/dev/null || true
fi
# PRACTICES_SET (#3754) — practice v2 instances into their ADR-051 home
# urn:chorus:domains:practices, declared on PracticeShape's instancesGraph so
# reader = writer = one canonical graph (the #3749 rule, applied to leg 3).
# v1 does NOT migrate: it was thrown away (Jeff, 2026-08-25) and its 40
# subjects retire through model-retirements.jsonl in this same land, with the
# source blocks removed from chorus.ttl so a redeploy cannot recreate them.
# Same SAFE-BY-CONSTRUCTION shape as PRINCIPLES_SET: staged load, per-subject
# additive merge, single-request-truth verify (#3726 — a blind verify fails
# closed, never passes).
# =============================================================================
if [ -z "${TTL:-}" ]; then
  PRACTICES_GRAPH="${PRACTICES_GRAPH:-urn:chorus:domains:practices}"
  PRACTICES_STAGING="${PRACTICES_GRAPH}-staging-deploy"
  PRACTICES_SET=(
    "$CHORUS_ROOT/roles/kade/ontology/practices-3754-instances.ttl"
  )
  for ttl in "${PRACTICES_SET[@]}"; do
    [ -f "$ttl" ] || { echo "athena-deploy-model: PRACTICES_SET TTL not found: $ttl" >&2; exit 1; }
    if command -v riot >/dev/null 2>&1 && ! riot --validate "$ttl" >/dev/null 2>&1; then
      echo "athena-deploy-model: riot validate FAILED for PRACTICES_SET $ttl — NOT deploying practices" >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$PRACTICES_GRAPH" reason="riot-invalid-practices" 2>/dev/null || true
      exit 1
    fi
  done
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$PRACTICES_STAGING" -o /dev/null 2>/dev/null || true
  for ttl in "${PRACTICES_SET[@]}"; do
    pcode=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /tmp/chorus-model-prac-resp.txt -w '%{http_code}' -X POST \
      -H 'Content-Type: text/turtle' --data-binary "@$ttl" "$FUSEKI_GSP?graph=$PRACTICES_STAGING" 2>/dev/null) || pcode="000"
    if [ "$pcode" != "200" ] && [ "$pcode" != "201" ] && [ "$pcode" != "204" ]; then
      echo "athena-deploy-model: PRACTICES_SET staging load failed for $ttl (http $pcode)" >&2
      head -3 /tmp/chorus-model-prac-resp.txt >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$PRACTICES_GRAPH" reason="practices-staging-http-$pcode" 2>/dev/null || true
      curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$PRACTICES_STAGING" -o /dev/null 2>/dev/null || true
      exit 1
    fi
  done
  PRACTICES_MERGE="DELETE { GRAPH <$PRACTICES_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$PRACTICES_STAGING> { ?s ?sp ?so } GRAPH <$PRACTICES_GRAPH> { ?s ?p ?o } } ; INSERT { GRAPH <$PRACTICES_GRAPH> { ?s ?p ?o } } WHERE { GRAPH <$PRACTICES_STAGING> { ?s ?p ?o } }"
  pmcode=$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -o /tmp/chorus-model-prac-merge.txt -w '%{http_code}' -X POST \
    -H 'Content-Type: application/sparql-update' --data-binary "$PRACTICES_MERGE" "$FUSEKI_UPDATE" 2>/dev/null) || pmcode="000"
  if [ "$pmcode" != "200" ] && [ "$pmcode" != "204" ]; then
    echo "athena-deploy-model: PRACTICES_SET merge staging->practices failed (http $pmcode)" >&2
    head -3 /tmp/chorus-model-prac-merge.txt >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$PRACTICES_GRAPH" reason="practices-merge-http-$pmcode" 2>/dev/null || true
    curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$PRACTICES_STAGING" -o /dev/null 2>/dev/null || true
    exit 1
  fi
  _presp=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
    "query=SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <$PRACTICES_STAGING> { ?s ?p ?o } FILTER NOT EXISTS { GRAPH <$PRACTICES_GRAPH> { ?s ?q ?r } } }" \
    -H 'Accept: text/csv' 2>/dev/null)
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$FUSEKI_GSP?graph=$PRACTICES_STAGING" -o /dev/null 2>/dev/null || true
  if ! printf '%s' "$_presp" | head -1 | grep -q '^n'; then
    echo "athena-deploy-model: PRACTICES-VERIFY could not ask (no CSV header) — refusing to pass a blind verify (#3726 single-request-truth)" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$PRACTICES_GRAPH" reason="practices-verify-unanswered" 2>/dev/null || true
    exit 1
  fi
  _pmissing=$(printf '%s\n' "$_presp" | tail -1 | tr -dc '0-9')
  if [ "${_pmissing:-1}" -ne 0 ] 2>/dev/null; then
    echo "athena-deploy-model: PRACTICES-VERIFY FAILED — ${_pmissing:-?} staged subject(s) absent from <$PRACTICES_GRAPH> post-merge" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$PRACTICES_GRAPH" reason="practices-verify-missing" missing="${_pmissing:-unknown}" 2>/dev/null || true
    exit 1
  fi
  _pn=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
    "query=PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(DISTINCT ?p) AS ?n) WHERE { GRAPH <$PRACTICES_GRAPH> { ?p a c:Practice } }" \
    -H "Accept: application/sparql-results+json" 2>/dev/null \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['n']['value'])" 2>/dev/null) || _pn="?"
  echo "athena-deploy-model: hydrated ${#PRACTICES_SET[@]} practices file(s) -> <$PRACTICES_GRAPH> (http $pmcode, $_pn practices live)"
  "$CHORUS_LOG" model.deployed "$ROLE" graph="$PRACTICES_GRAPH" practices="${_pn}" 2>/dev/null || true
fi

exit 0
