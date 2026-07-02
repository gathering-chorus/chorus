#!/usr/bin/env bats
# @test-type: integration
# test-chorus-model-deploy.bats (#3509) — the MODEL (chorus.ttl schema) deploys into Fuseki.
# AC: a deploy step loads chorus.ttl -> urn:chorus:ontology and the 4 primitive shapes
# (+ StepShape) are queryable live; an invalid model is refused fail-loud (no deploy).
# Runs against a throwaway test graph so it never touches the live ontology graph.

# #3593 — resolve the script + model UNDER TEST beside this test file (the werk during a
# werk run), NOT via CHORUS_ROOT (which points at canonical and would test the unedited
# tree). A test exercises the code it ships with.
SCRIPT="$BATS_TEST_DIRNAME/chorus-model-deploy.sh"
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
TTL="$ROOT/roles/silas/ontology/chorus.ttl"
TEST_GRAPH="urn:chorus:ontology-test-bats-3509"
Q="http://localhost:3030/pods/query"
GSP="http://localhost:3030/pods/data"

teardown() {
  curl -s -X DELETE "$GSP?graph=$TEST_GRAPH" -o /dev/null 2>/dev/null || true
  curl -s -X DELETE "$GSP?graph=${TEST_GRAPH}-bad" -o /dev/null 2>/dev/null || true
  curl -s -X DELETE "$GSP?graph=${TEST_GRAPH}-retire" -o /dev/null 2>/dev/null || true
  curl -s -X DELETE "$GSP?graph=${TEST_GRAPH}-partial" -o /dev/null 2>/dev/null || true
  curl -s -X DELETE "$GSP?graph=${TEST_GRAPH}-empty" -o /dev/null 2>/dev/null || true
  curl -s -X DELETE "$GSP?graph=${TEST_GRAPH}-proving" -o /dev/null 2>/dev/null || true
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

@test "an invalid TTL is refused fail-loud (exit 1, no deploy)" {
  badttl="$(mktemp)"; printf 'this is not @@ valid turtle .\n' > "$badttl"
  run env ONTOLOGY_GRAPH="${TEST_GRAPH}-bad" TTL="$badttl" bash "$SCRIPT"
  rm -f "$badttl"
  [ "$status" -eq 1 ]
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
