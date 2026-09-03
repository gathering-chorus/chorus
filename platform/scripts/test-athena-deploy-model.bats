#!/usr/bin/env bats
# @test-type: integration
# test-chorus-model-deploy.bats (#3509) — the MODEL (chorus.ttl schema) deploys into Fuseki.
# AC: a deploy step loads chorus.ttl -> urn:chorus:ontology and the 4 primitive shapes
# (+ StepShape) are queryable live; an invalid model is refused fail-loud (no deploy).
# Runs against a throwaway test graph so it never touches the live ontology graph.

# #3593 — resolve the script + model UNDER TEST beside this test file (the werk during a
# werk run), NOT via CHORUS_ROOT (which points at canonical and would test the unedited
# tree). A test exercises the code it ships with.
SCRIPT="$BATS_TEST_DIRNAME/athena-deploy-model.sh"
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
TTL="$ROOT/roles/silas/ontology/chorus.ttl"
TEST_GRAPH="urn:chorus:ontology-test-bats-3509"
Q="http://localhost:3030/pods/query"
GSP="http://localhost:3030/pods/data"

# #3606 — this teardown was UNAUTHENTICATED. Post-#3564 every one of these DELETEs
# 401'd and `|| true` swallowed it, so the suite has never cleaned up: 25,996
# triples were still resident live across -3509 (17,994), -proving (7,998),
# -partial and -retire.
#
# Residue is not just clutter here. Each test deploys into a graph and then asserts
# shapes are queryable in it. With last run's triples already present, those
# assertions pass whether or not this run's deploy did anything — the suite cannot
# distinguish "the model deployed" from "the graph was never emptied". A drop that
# cannot be verified is not a drop.
# shellcheck source=/dev/null
. "$ROOT/platform/scripts/fuseki-auth.sh" 2>/dev/null || true

_drop_graph() {
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$GSP?graph=$1" -o /dev/null 2>/dev/null || true
}

_all_test_graphs() {
  printf '%s\n' "$TEST_GRAPH" "${TEST_GRAPH}-bad" "${TEST_GRAPH}-retire" \
    "${TEST_GRAPH}-partial" "${TEST_GRAPH}-empty" "${TEST_GRAPH}-proving"
}

_residue_triples() {
  local g total=0 n
  while IFS= read -r g; do
    n="$(curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" "$Q" -H "Accept: text/csv" \
      --data-urlencode "query=SELECT (COUNT(*) AS ?n) WHERE { GRAPH <$g> { ?s ?p ?o } }" \
      2>/dev/null | tail -1 | tr -d '[:space:]')"
    total=$(( total + ${n:-0} ))
  done <<< "$(_all_test_graphs)"
  echo "$total"
}

_clear_all() {
  local g
  while IFS= read -r g; do _drop_graph "$g"; done <<< "$(_all_test_graphs)"
}

# #3606 — SEAM THE SPINE. Two tests here drive chorus-model-deploy.sh into its
# refusal paths on purpose ("invalid TTL is refused fail-loud", "empty-staging
# guard REFUSES"). Passing correctly, they each emit a real
# `model.deploy.failed` to the LIVE spine, where the health surface reads it back
# as a production incident.
#
# Measured, not assumed: every model.deploy.failed in Loki over the last 7 days —
# 16 of 16 — came from these two test graphs (-3509-bad / riot-invalid and
# -3509-empty / retire-guard-empty-staging). There were no real deploy failures at
# all. The recurring "nightly model-deploy failing" alarm has been this suite
# reporting its own successful refusals.
#
# CHORUS_LOG_FILE is the #3615 membrane seam; chorus-log routes through
# chorus-hook-shim, which honors it (verified live: emission landed in the tmp
# seam, zero occurrences in the live spine). Seam in the suite — never
# CHORUS_CONTEXT=prod, which would only re-point the same writes at production.
setup_file() {
  export CHORUS_LOG_FILE="${CHORUS_LOG_FILE:-$BATS_RUN_TMPDIR/membrane-spine.log}"
  _clear_all
  local left
  left="$(_residue_triples)"
  if [ "${left:-0}" != "0" ]; then
    echo "FATAL: ${left} triples survived the pre-run clear of the #3509 test graphs." >&2
    echo "Every assertion here queries those graphs; residue makes them pass off the" >&2
    echo "last run instead of this deploy. Refusing to report a green we did not earn." >&2
    exit 1
  fi
}

teardown() {
  _clear_all
}

teardown_file() {
  _clear_all
}

@test "chorus-model-deploy loads chorus.ttl into the ontology graph (exit 0)" {
  run env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT"
  [ "$status" -eq 0 ]
}

@test "all 4 primitive shapes + StepShape are queryable after deploy" {
  env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT" >/dev/null 2>&1
  run curl -s "$Q" --data-urlencode "query=PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX sh: <http://www.w3.org/ns/shacl#> SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH <$TEST_GRAPH> { ?s a sh:NodeShape . FILTER(?s IN (chorus:ProductShape,chorus:DomainShape,chorus:ServiceShape,chorus:ValueStreamShape,chorus:StepShape)) } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"value":"5"'* ]]
}

@test "DomainShape carries chorus:purpose (the capability) after deploy" {
  env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT" >/dev/null 2>&1
  run curl -s "$Q" --data-urlencode "query=PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX sh: <http://www.w3.org/ns/shacl#> ASK { GRAPH <$TEST_GRAPH> { chorus:DomainShape sh:property [ sh:path chorus:purpose ] } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"boolean":true'* ]]
}

@test "#3736 a successful deploy stamps deployedFromCommit == repo HEAD (single-request truth)" {
  env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" bash "$SCRIPT" >/dev/null 2>&1
  head_sha="$(git -C "$CHORUS_ROOT" rev-parse HEAD)"
  run curl -s "$Q" --data-urlencode "query=SELECT ?c WHERE { GRAPH <$TEST_GRAPH> { <urn:chorus:model-deploy> <urn:chorus:vocab#deployedFromCommit> ?c } }" -H "Accept: text/csv"
  echo "stamp query: $output"
  [[ "$output" == *"$head_sha"* ]]
}

@test "#3736 negative proof: a graph deployed WITHOUT the stamp fails the stamp query (check can go red)" {
  # Simulate the pre-#3736 world: load valid TTL via GSP directly (no script, no stamp).
  # The stamp SELECT must return NO rows — proving the werk-deploy gate that reads it
  # can distinguish stamped-by-the-deployer from merely-has-triples.
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X PUT -H 'Content-Type: text/turtle' \
    --data-binary @"$TTL" "$GSP?graph=${TEST_GRAPH}-proving" -o /dev/null
  run curl -s "$Q" --data-urlencode "query=SELECT ?c WHERE { GRAPH <${TEST_GRAPH}-proving> { <urn:chorus:model-deploy> <urn:chorus:vocab#deployedFromCommit> ?c } }" -H "Accept: text/csv"
  # header line only, no value row
  [ "$(echo "$output" | grep -c .)" -le 1 ]
}

# --- #3752: staged retirements — execute, idempotent, and three refusals ---

@test "#3752 staged claim retirement executes on deploy and is idempotent on rerun" {
  RG="${TEST_GRAPH}-retclaim"
  # NB: --data-binary with a leading @ in a literal reads a FILE — write the
  # fixture turtle to disk first (the curl @-semantics trap).
  TT="$BATS_TEST_TMPDIR/claim.ttl"
  printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\nchorus:testdom chorus:definesVocabulary chorus:TestClaimX .\n' > "$TT"
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X PUT -H 'Content-Type: text/turtle' \
    --data-binary @"$TT" "$GSP?graph=$RG" -o /dev/null
  RF="$BATS_TEST_TMPDIR/ret.jsonl"
  printf '{"subject_domain":"testdom","object_class":"TestClaimX","graph":"%s"}\n' "$RG" > "$RF"
  run env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" RETIREMENTS_FILE="$RF" bash "$SCRIPT"
  echo "output: $output"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "retirement executed .* claim testdom->TestClaimX"
  run curl -s "$Q" --data-urlencode "query=ASK { GRAPH <$RG> { <https://jeffbridwell.com/chorus#testdom> <https://jeffbridwell.com/chorus#definesVocabulary> <https://jeffbridwell.com/chorus#TestClaimX> } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"boolean":false'* ]]
  # idempotent rerun: already-absent is noted, never an error
  run env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" RETIREMENTS_FILE="$RF" bash "$SCRIPT"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "already absent"
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$GSP?graph=$RG" -o /dev/null 2>/dev/null || true
}

@test "#3752 NEGATIVE PROOF: malformed staging line REFUSES the deploy" {
  RF="$BATS_TEST_TMPDIR/bad.jsonl"
  printf 'this is not json\n' > "$RF"
  run env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" RETIREMENTS_FILE="$RF" bash "$SCRIPT"
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "MALFORMED"
}

@test "#3752 NEGATIVE PROOF: claim SERVED at execute time REFUSES (Wren's window)" {
  # 'credentials' is served by the live owl-api; a staged Credential retirement
  # must refuse AT EXECUTE regardless of what the store says.
  RF="$BATS_TEST_TMPDIR/served.jsonl"
  printf '{"subject_domain":"testdom","object_class":"Credential","graph":"%s"}\n' "${TEST_GRAPH}-sv" > "$RF"
  run env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" RETIREMENTS_FILE="$RF" bash "$SCRIPT"
  echo "output: $output"
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "SERVED at /credentials RIGHT NOW"
}

@test "#3752/#4080 NEGATIVE PROOF: unanswerable owl-api DEFERS claim retirements (never blind) and the deploy proceeds" {
  # #4080 changed the shape of this refusal: the script used to exit 1 here, which
  # sat before the SECURITY_SET load and left a fresh werk store with 0 Principal
  # rows (the hollow env-up). Now an unanswerable serve-check DEFERS the claim
  # retirement — the claim SURVIVES, the deploy continues — and says so.
  RG="${TEST_GRAPH}-na"
  TT="$BATS_TEST_TMPDIR/claim-na.ttl"
  printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\nchorus:testdom chorus:definesVocabulary chorus:TestClaimX .\n' > "$TT"
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X PUT -H 'Content-Type: text/turtle' \
    --data-binary @"$TT" "$GSP?graph=$RG" -o /dev/null
  RF="$BATS_TEST_TMPDIR/noapi.jsonl"
  printf '{"subject_domain":"testdom","object_class":"TestClaimX","graph":"%s"}\n' "$RG" > "$RF"
  run env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" RETIREMENTS_FILE="$RF" OWL_API_URL="http://localhost:1" bash "$SCRIPT"
  echo "output: $output"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "serve-check UNANSWERED"
  # the negative that matters: nothing was retired blind — the claim is still there
  run curl -s "$Q" --data-urlencode "query=ASK { GRAPH <$RG> { <https://jeffbridwell.com/chorus#testdom> <https://jeffbridwell.com/chorus#definesVocabulary> <https://jeffbridwell.com/chorus#TestClaimX> } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"boolean":true'* ]]
  # and the same staging line WITH an answering serve-check does retire it (so the deferral above is the unanswered path, not the harness)
  run env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" RETIREMENTS_FILE="$RF" bash "$SCRIPT"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "retirement executed .* claim testdom->TestClaimX"
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$GSP?graph=$RG" -o /dev/null 2>/dev/null || true
}

# --- #3732: whole-graph retirement — backup-verified, fail-closed ---

@test "#3732 graph retirement backs up, drops, verifies, and is idempotent" {
  RG="${TEST_GRAPH}-dropme"
  TT="$BATS_TEST_TMPDIR/drop.ttl"
  printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\nchorus:DropA chorus:x "1" .\nchorus:DropB chorus:x "2" .\n' > "$TT"
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X PUT -H 'Content-Type: text/turtle' --data-binary @"$TT" "$GSP?graph=$RG" -o /dev/null
  RF="$BATS_TEST_TMPDIR/g.jsonl"
  printf '{"retire_graph":"%s","reason":"bats fixture","by":"silas","card":"3732"}\n' "$RG" > "$RF"
  run env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" RETIREMENTS_FILE="$RF" bash "$SCRIPT"
  echo "output: $output"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "graph retirement executed"
  # the graph is empty AND a backup file exists with the triples
  run curl -s "$Q" --data-urlencode "query=SELECT (COUNT(*) AS ?n) WHERE { GRAPH <$RG> { ?s ?p ?o } }" -H "Accept: text/csv"
  [[ "$output" == *"0"* ]]
  ls "$CHORUS_ROOT/platform/backups/graph-retirements/" | grep -q "dropme"
  # idempotent
  run env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" RETIREMENTS_FILE="$RF" bash "$SCRIPT"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "already empty"
}

@test "#3732 NEGATIVE PROOF: unbackupable graph is NOT dropped (no restore path, no destruction)" {
  RG="${TEST_GRAPH}-nobackup"
  TT="$BATS_TEST_TMPDIR/nb.ttl"
  printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\nchorus:KeepA chorus:x "1" .\n' > "$TT"
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X PUT -H 'Content-Type: text/turtle' --data-binary @"$TT" "$GSP?graph=$RG" -o /dev/null
  RF="$BATS_TEST_TMPDIR/nb.jsonl"
  printf '{"retire_graph":"%s","reason":"backup will fail","by":"silas","card":"3732"}\n' "$RG" > "$RF"
  # Backup dir is unwritable → no backup file → refuse (data must survive).
  UNWRITABLE="$BATS_TEST_TMPDIR/ro"; mkdir -p "$UNWRITABLE"; chmod 500 "$UNWRITABLE"
  run env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" RETIREMENTS_FILE="$RF" GRAPH_BACKUP_DIR="$UNWRITABLE/nested" bash "$SCRIPT"
  echo "output: $output"
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "no verified restore path"
  # and the data SURVIVED
  run curl -s "$Q" --data-urlencode "query=ASK { GRAPH <$RG> { <https://jeffbridwell.com/chorus#KeepA> ?p ?o } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"boolean":true'* ]]
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X DELETE "$GSP?graph=$RG" -o /dev/null 2>/dev/null || true
}

@test "an invalid TTL is refused fail-loud (exit 1, no deploy)" {
  badttl="$(mktemp)"; printf 'this is not @@ valid turtle .\n' > "$badttl"
  run env ONTOLOGY_GRAPH="${TEST_GRAPH}-bad" TTL="$badttl" bash "$SCRIPT"
  rm -f "$badttl"
  [ "$status" -eq 1 ]
}

# --- #3731: could-not-ask must never read as success (three fail-open guards) ---

@test "#3731 NEGATIVE PROOF: verify against a dead query endpoint FAILS CLOSED (was: 0-missing blank-pass)" {
  run env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" FUSEKI_QUERY="http://localhost:9/dead" bash "$SCRIPT"
  echo "output: $output"
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "could not ask"
}

@test "#3731 NEGATIVE PROOF: SHACL validator crash reports UNKNOWN, never 0 violations (non-gating)" {
  fake="$BATS_TEST_TMPDIR/bin"; mkdir -p "$fake"
  printf '#!/bin/sh\nexit 3\n' > "$fake/shacl-crash"; chmod +x "$fake/shacl-crash"
  run env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" SHACL_REPORT=1 SHACL_BIN="$fake/shacl-crash" bash "$SCRIPT"
  echo "output: $output"
  [ "$status" -eq 0 ]   # report stays non-gating — the deploy itself is fine
  echo "$output" | grep -q "CRASHED — violations UNKNOWN"
  ! echo "$output" | grep -q "0 violation(s)"
}

@test "#3731 NEGATIVE PROOF: absent shacl is an explicit skip, not a clean-run lookalike" {
  run env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" SHACL_REPORT=1 SHACL_BIN="/nonexistent-shacl-3731" bash "$SCRIPT"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "SHACL report SKIPPED"
}

@test "#3731 NEGATIVE PROOF: absent riot REFUSES the deploy (was: silent unvalidated deploy)" {
  run env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" RIOT_BIN="/nonexistent-riot-3731" bash "$SCRIPT"
  echo "output: $output"
  [ "$status" -eq 1 ]
  echo "$output" | grep -q "REFUSING — riot"
}

@test "#3731 absent riot + explicit ALLOW_UNVALIDATED=1 proceeds LOUDLY (not a clean run)" {
  run env ONTOLOGY_GRAPH="$TEST_GRAPH" TTL="$TTL" RIOT_BIN="/nonexistent-riot-3731" ALLOW_UNVALIDATED=1 bash "$SCRIPT"
  echo "output: $output"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q "deploying UNVALIDATED TTL"
}

# --- #3593: retire-subject (gap #2) + its safety gate ---

@test "#3593 retire-subject deletes a Domain absent from staging; keeps present + non-domain" {
  G="${TEST_GRAPH}-retire"
  curl -s -X DELETE "$GSP?graph=$G" -o /dev/null 2>/dev/null || true
  pre="$(mktemp)"; cat > "$pre" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:domainKeep a chorus:Domain ; chorus:purpose "keep" .
chorus:domainGone a chorus:Domain ; chorus:purpose "gone" .
chorus:liveInst  a chorus:Test ; chorus:purpose "instance" .
EOF
  curl -s -X POST -H 'Content-Type: text/turtle' --data-binary "@$pre" "$GSP?graph=$G" >/dev/null
  keep="$(mktemp).ttl"; cat > "$keep" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:domainKeep a chorus:Domain ; chorus:purpose "keep" .
EOF
  env ONTOLOGY_GRAPH="$G" TTL="$keep" RETIRE_ABSENT=1 bash "$SCRIPT" >/dev/null 2>&1
  rm -f "$pre" "$keep"
  # present domain kept
  run curl -s "$Q" --data-urlencode "query=PREFIX chorus: <https://jeffbridwell.com/chorus#> ASK { GRAPH <$G> { chorus:domainKeep a chorus:Domain } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"boolean":true'* ]]
  # live-only NON-domain preserved (gap #1 invariant — retire is typed to Domain/SubDomain only)
  run curl -s "$Q" --data-urlencode "query=PREFIX chorus: <https://jeffbridwell.com/chorus#> ASK { GRAPH <$G> { chorus:liveInst a chorus:Test } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"boolean":true'* ]]
  # absent domain RETIRED (no triples remain)
  run curl -s "$Q" --data-urlencode "query=PREFIX chorus: <https://jeffbridwell.com/chorus#> ASK { GRAPH <$G> { chorus:domainGone ?p ?o } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"boolean":false'* ]]
  curl -s -X DELETE "$GSP?graph=$G" -o /dev/null 2>/dev/null || true
}

@test "#3593 a partial (TTL=) deploy does NOT retire absent domains (gate blocks mass-delete)" {
  G="${TEST_GRAPH}-partial"
  curl -s -X DELETE "$GSP?graph=$G" -o /dev/null 2>/dev/null || true
  pre="$(mktemp)"; cat > "$pre" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:domainKeep a chorus:Domain ; chorus:purpose "keep" .
chorus:domainGone a chorus:Domain ; chorus:purpose "gone" .
EOF
  curl -s -X POST -H 'Content-Type: text/turtle' --data-binary "@$pre" "$GSP?graph=$G" >/dev/null
  keep="$(mktemp).ttl"; cat > "$keep" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:domainKeep a chorus:Domain ; chorus:purpose "keep" .
EOF
  # TTL= override + no RETIRE_ABSENT → gate defaults OFF; domainGone must SURVIVE
  env ONTOLOGY_GRAPH="$G" TTL="$keep" bash "$SCRIPT" >/dev/null 2>&1
  rm -f "$pre" "$keep"
  run curl -s "$Q" --data-urlencode "query=PREFIX chorus: <https://jeffbridwell.com/chorus#> ASK { GRAPH <$G> { chorus:domainGone a chorus:Domain } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"boolean":true'* ]]
  curl -s -X DELETE "$GSP?graph=$G" -o /dev/null 2>/dev/null || true
}

@test "#3593 MODEL_SET includes the 34-domain sources (domains-wren-silas + domains-kade-3581)" {
  run grep -cE 'domains-wren-silas\.ttl|domains-kade-3581\.ttl' "$SCRIPT"
  [ "$output" -ge 2 ]
}

# --- #3536: stop-truncating primitive (default-off flip + empty-staging backstop) ---

@test "#3536 RETIRE_ABSENT defaults OFF — no truncate-by-default (the 06-26 wipe root)" {
  run grep -cE 'RETIRE_ABSENT:-0' "$SCRIPT"
  [ "$output" -ge 1 ]
  # the old default-1-on-full-deploy form must be gone
  run grep -cE 'RETIRE_ABSENT:-\$\(\[ -z' "$SCRIPT"
  [ "$output" -eq 0 ]
}

@test "#3536 empty-staging guard: retire against 0-domain staging REFUSES, never wipes live" {
  G="${TEST_GRAPH}-empty"
  curl -s -X DELETE "$GSP?graph=$G" -o /dev/null 2>/dev/null || true
  pre="$(mktemp)"; cat > "$pre" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:domainKeep a chorus:Domain ; chorus:purpose "keep" .
EOF
  curl -s -X POST -H 'Content-Type: text/turtle' --data-binary "@$pre" "$GSP?graph=$G" >/dev/null
  # staging TTL with NO domain subjects (valid TTL, zero domains) + explicit retire
  nodom="$(mktemp).ttl"; cat > "$nodom" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:someInst a chorus:Test ; chorus:purpose "not a domain" .
EOF
  run env ONTOLOGY_GRAPH="$G" TTL="$nodom" RETIRE_ABSENT=1 bash "$SCRIPT"
  rm -f "$pre" "$nodom"
  # guard REFUSES (exit non-zero)
  [ "$status" -ne 0 ]
  # live domain SURVIVES — no wipe
  run curl -s "$Q" --data-urlencode "query=PREFIX chorus: <https://jeffbridwell.com/chorus#> ASK { GRAPH <$G> { chorus:domainKeep a chorus:Domain } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"boolean":true'* ]]
  curl -s -X DELETE "$GSP?graph=$G" -o /dev/null 2>/dev/null || true
}

# --- #3536 AC5: the proving case — the exact 06-20 shape-wipe this card exists to fix ---

@test "#3536 AC5 proving case: re-deploy restores DomainShape's 11 sh:property AND preserves a co-tenant" {
  G="${TEST_GRAPH}-proving"
  curl -s -X DELETE "$GSP?graph=$G" -o /dev/null 2>/dev/null || true
  # Reproduce the wipe: a stripped DomainShape (0 sh:property, the 06-20 broken state)
  # + a co-tenant subject that is NOT in chorus.ttl (must survive the deploy).
  pre="$(mktemp)"; cat > "$pre" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
chorus:DomainShape a sh:NodeShape .
chorus:coTenantProbe a chorus:Test ; chorus:purpose "co-tenant, absent from chorus.ttl" .
EOF
  curl -s -X POST -H 'Content-Type: text/turtle' --data-binary "@$pre" "$GSP?graph=$G" >/dev/null
  rm -f "$pre"
  # deploy the REAL model (chorus.ttl) into the test graph
  env ONTOLOGY_GRAPH="$G" TTL="$TTL" bash "$SCRIPT" >/dev/null 2>&1
  # AC5a — DomainShape's 11 sh:property attrs are RESTORED (the wipe is undone)
  run curl -s "$Q" --data-urlencode "query=PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX sh: <http://www.w3.org/ns/shacl#> SELECT (COUNT(?pp) AS ?n) WHERE { GRAPH <$G> { chorus:DomainShape sh:property ?pp } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"value":"11"'* ]]
  # AC5b — the co-tenant (not in chorus.ttl) SURVIVES (co-tenant-safe, AC3 proven end-to-end)
  run curl -s "$Q" --data-urlencode "query=PREFIX chorus: <https://jeffbridwell.com/chorus#> ASK { GRAPH <$G> { chorus:coTenantProbe a chorus:Test } }" -H "Accept: application/sparql-results+json"
  [[ "${output// /}" == *'"boolean":true'* ]]
  curl -s -X DELETE "$GSP?graph=$G" -o /dev/null 2>/dev/null || true
}
