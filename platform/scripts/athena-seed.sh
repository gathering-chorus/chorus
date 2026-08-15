#!/usr/bin/env bash
# athena-seed.sh (#3895) — seed the PURE-ABox value-stream/role instances into
# urn:chorus:instances through the governed DAL door (athena-model seed).
#
# EXTRACTED from chorus-model-deploy.sh (#3895). That script is the RECOVERY
# path (#3785): it must authenticate to the STORE only and must NEVER require a
# CHORUS_IDENTITY_TOKEN or call athena-model — the 2026-08-06 lockout proved a
# recovery path that gates on identity cannot recover an emptied allow-set.
# #3839's instance-seed leg violated that invariant by living inside it. The
# split restores it:
#
#   chorus-model-deploy.sh  — schema + security + principles, store-auth only,
#                             runs in recovery with CSS DOWN. Guarded by
#                             platform/tests/recovery-path-ungated-3785.bats.
#   athena-seed.sh (this)   — the DAL-gated instance leg. Requires a VERIFIED
#                             identity (#3687); fails CLOSED without one. Runs
#                             at land (werk-deploy calls model-deploy THEN this)
#                             and on demand. NOT part of recovery: a store
#                             restored by the recovery path is complete except
#                             for these pure-ABox instances, which re-seed the
#                             moment identity is back.
#
# Spine: model.deployed {graph, triples} on success; model.deploy.failed
# {graph, reason} on any refusal. Exit 0 = seeded + output-verified; 1 = not.
set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
FUSEKI_QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
CHORUS_LOG="${CHORUS_LOG:-$CHORUS_ROOT/platform/scripts/chorus-log}"
ROLE="${DEPLOY_ROLE:-${CHORUS_ROLE:-system}}"

# TTL= partial deploys never included instance hydration (chorus-model-deploy
# behavior preserved): refuse loudly instead of seeding a partial world.
if [ -n "${TTL:-}" ]; then
  echo "athena-seed: TTL= partial deploys do not seed instances — nothing to do" >&2
  exit 0
fi

if true; then
  INSTANCE_GRAPH="${INSTANCE_GRAPH:-urn:chorus:instances}"
  # #3839 — kind:path, because the door validates against the class's shape and
  # the caller must say which class it is loading. A file carrying two kinds is
  # listed twice, once per kind — the same file, seeded once for each.
  INSTANCE_SET_KINDS=(
    "value-stream:$CHORUS_ROOT/designing/data/value-stream-instances.ttl"
    "value-stream-step:$CHORUS_ROOT/designing/data/value-stream-step-instances.ttl"
    # #3838 — the four roles. Until that card they existed ONLY in the live
    # store: every ownedBy and holdsRole pointed at individuals no deploy could
    # reproduce, which is why re-seeding identity was unsafe.
    "role:$CHORUS_ROOT/roles/wren/ontology/role-instances-3838.ttl"
  )
  # riot-validate each distinct file once before any write.
  #
  # Dedup through a plain STRING, not by reading the array back. Canonical runs
  # bash 3.2, where `${arr[*]}` on an EMPTY array is an unbound-variable error
  # under `set -u` — so the array-reading version died on its first iteration,
  # every time, on the only box that matters. It ran clean here because this werk
  # has a newer bash. That is the whole bug: the deploy is 3.2, the werk is not.
  _seen_ttl=""
  INSTANCE_SET=()
  for entry in "${INSTANCE_SET_KINDS[@]}"; do
    f="${entry#*:}"
    case "$_seen_ttl" in
      *"|$f|"*) ;;
      *) INSTANCE_SET+=("$f"); _seen_ttl="${_seen_ttl}|$f|" ;;
    esac
  done
  for ttl in ${INSTANCE_SET[@]+"${INSTANCE_SET[@]}"}; do
    [ -f "$ttl" ] || { echo "athena-seed: INSTANCE_SET TTL not found: $ttl" >&2; exit 1; }
    if command -v riot >/dev/null 2>&1 && ! riot --validate "$ttl" >/dev/null 2>&1; then
      echo "athena-seed: riot validate FAILED for INSTANCE_SET $ttl — NOT deploying instances" >&2
      "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$INSTANCE_GRAPH" reason="riot-invalid-instance" 2>/dev/null || true
      exit 1
    fi
  done
  # #3839 — instances go through THE DOOR, not around it.
  #
  # This used to stage each TTL with a GSP POST and merge it with a raw SPARQL
  # transaction. That path calls no validator: every instance in the graph
  # arrived somewhere no shape could refuse it. Proven on #3838 — chorus:uniqueGlobal
  # on webId is real and enforced in the DAL, and Principals never travel that
  # path, so the rule and its enforcement never meet.
  #
  # athena-model seed reads each class's SHACL shape, validates EVERY subject
  # before writing anything, fails closed, and emits model.seed.refused naming
  # the subject and the constraint that refused it.
  #
  # ONE call, all kinds. Each --kind/--ttl pair states what the caller believes
  # that file is; a subject whose rdf:type disagrees is refused rather than
  # validated against whatever class it claims. They go together because the
  # kinds reference each other (a stream contains its steps, each step is
  # inStream its stream) — see seed_multi's comment for why two calls could
  # never bootstrap that.
  # The door requires a VERIFIED identity (#3687 retired DEPLOY_ROLE env-trust).
  # The staged-POST path this replaces needed no identity at all — that was part
  # of what made it a way around the door. Mint here and fail loudly: a deploy
  # that silently skipped instances because it could not identify itself is the
  # failure mode this card exists to end.
  if [ -z "${CHORUS_IDENTITY_TOKEN:-}" ]; then
    CHORUS_IDENTITY_TOKEN="$("$CHORUS_ROOT/platform/scripts/chorus-identity-token" "$ROLE" 2>/dev/null || true)"
    export CHORUS_IDENTITY_TOKEN
  fi
  if [ -z "${CHORUS_IDENTITY_TOKEN:-}" ]; then
    echo "athena-seed: cannot mint a CSS identity token for role '$ROLE' — instances NOT deployed." >&2
    echo "  The instance leg writes through athena-model, which fails closed without a verified identity." >&2
    echo "  Run with DEPLOY_ROLE=<a role with ~/.chorus/identity/<role>/cred.json>, or bring CSS up." >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$INSTANCE_GRAPH" reason="no-identity-token" 2>/dev/null || true
    exit 1
  fi

  SEED_ARGS=()
  for entry in "${INSTANCE_SET_KINDS[@]}"; do
    ikind="${entry%%:*}"
    ittl="${entry#*:}"
    [ -f "$ittl" ] || { echo "athena-seed: INSTANCE_SET TTL not found: $ittl" >&2; exit 1; }
    SEED_ARGS+=(--kind "$ikind" --ttl "$ittl")
  done
  if ! sout=$(athena-model seed ${SEED_ARGS[@]+"${SEED_ARGS[@]}"} --graph "$INSTANCE_GRAPH" --provenance deploy 2>&1); then
    echo "athena-seed: REFUSED — instances NOT written (the batch is one transaction)" >&2
    # The refusal names the subject and the constraint — print it whole. A
    # deploy that fails with a count and no names is not usable (#3839 AC).
    echo "$sout" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$INSTANCE_GRAPH" reason="instance-seed-refused" 2>/dev/null || true
    exit 1
  fi
  echo "athena-seed: $sout"

  # #3839 — output-verify, rewritten because the old one no longer could work.
  #
  # It compared the STAGING graph against the live graph. seed writes through the
  # DAL and never stages, so that query would have compared an empty set and
  # passed every time — a verify that cannot fail, which is worse than none.
  #
  # This asks the question that still means something: every SUBJECT declared in
  # the source TTLs must be present in the live graph. Expected count comes from
  # the files (riot → N-Triples → distinct subjects); found count comes from the
  # store. A dead endpoint yields no CSV header and REFUSES rather than passing
  # blind (#3731).
  _expected_iris=$(for f in ${INSTANCE_SET[@]+"${INSTANCE_SET[@]}"}; do riot --output=ntriples "$f" 2>/dev/null; done \
    | awk '{print $1}' | grep '^<' | sort -u)
  _expected=$(printf '%s\n' "$_expected_iris" | grep -c '^<' || true)
  _values=$(printf '%s ' $_expected_iris)
  _fresp=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
    "query=SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { VALUES ?s { $_values } GRAPH <$INSTANCE_GRAPH> { ?s ?p ?o } }" \
    -H 'Accept: text/csv' 2>/dev/null)
  if ! printf '%s' "$_fresp" | head -1 | grep -q '^n'; then
    echo "athena-seed: INSTANCE-VERIFY could not ask (no CSV header from the store) — refusing to pass a blind verify (#3731)" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$INSTANCE_GRAPH" reason="instance-verify-unanswered" 2>/dev/null || true
    exit 1
  fi
  _found=$(printf '%s\n' "$_fresp" | tail -1 | tr -dc '0-9')
  if [ "${_found:-0}" -ne "${_expected:-1}" ] 2>/dev/null; then
    echo "athena-seed: INSTANCE-VERIFY FAILED — ${_expected} subject(s) declared in source, ${_found} present in <$INSTANCE_GRAPH>" >&2
    "$CHORUS_LOG" model.deploy.failed "$ROLE" graph="$INSTANCE_GRAPH" reason="instance-verify-missing" expected="${_expected}" found="${_found}" 2>/dev/null || true
    exit 1
  fi
  _in=$(curl -s "$FUSEKI_QUERY" --data-urlencode \
    "query=SELECT (COUNT(*) AS ?n) WHERE { GRAPH <$INSTANCE_GRAPH> { ?s ?p ?o } }" \
    -H "Accept: application/sparql-results+json" 2>/dev/null \
    | python3 -c "import sys,json;print(json.load(sys.stdin)['results']['bindings'][0]['n']['value'])" 2>/dev/null) || _in="?"
  echo "athena-seed: seeded ${#INSTANCE_SET_KINDS[@]} kind(s) from ${#INSTANCE_SET[@]} file(s) -> <$INSTANCE_GRAPH> (${_expected} subjects verified, $_in triples live)"
  "$CHORUS_LOG" model.deployed "$ROLE" graph="$INSTANCE_GRAPH" triples="${_in}" 2>/dev/null || true
fi
