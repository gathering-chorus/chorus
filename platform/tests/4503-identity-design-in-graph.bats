#!/usr/bin/env bats
# @test-type: contract — reads the model and the page files with arq and grep; no store, no service
# @domain: identity
#
# #4503 — Jeff, 2026-10-12: "only 1 login / sign in flow", "accidental architecture",
# then: does the identity design use the graph and our service template? It did not.
# The design is now the service-identity row in ADR-029's sections, and the service
# page renders them in ADR-029's order. Files read:
#   roles/silas/ontology/chorus.ttl
#   designing/data/service-instances.ttl
#   platform/api/public/athena/service.html

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
MODEL="$ROOT/roles/silas/ontology/chorus.ttl"
SVC="$ROOT/designing/data/service-instances.ttl"
PAGE="$ROOT/platform/api/public/athena/service.html"
P='PREFIX chorus: <https://jeffbridwell.com/chorus#> PREFIX sh: <http://www.w3.org/ns/shacl#>'

setup() { command -v arq >/dev/null 2>&1 || skip "arq (Jena) not installed — UNMEASURED here, not green"; }

ask() { local q="$1"; shift; local d=(); for f in "$@"; do d+=(--data "$f"); done
  arq "${d[@]}" --results=csv "$P ASK { $q }" 2>/dev/null | tail -1 | tr -d '\r'; }

# ADR-029 sections a service row must carry, by property
SECTIONS="overview contract validation failureMode asIs toBe implementationPlan filesInScope gaps notInScope pathToClose reference"

missing_sections() { # missing_sections <service-instances file> → the sections service-identity lacks
  local out=""
  for p in $SECTIONS; do
    [ "$(ask "chorus:service-identity chorus:$p ?v . FILTER(STRLEN(STR(?v)) > 20)" "$1")" = "true" ] || out="$out $p"
  done
  printf '%s' "$out"
}

@test "ServiceShape carries ADR-029's consumer half: dependsOn, contract, validation, failureMode, filesInScope" {
  for p in dependsOn contract validation failureMode filesInScope reference; do
    [ "$(ask "chorus:ServiceShape sh:property [ sh:path chorus:$p ; chorus:exposure ?x ]" "$MODEL")" = "true" ] || { echo "ServiceShape lacks $p (or its exposure)"; return 1; }
  done
  # every design section public (Wren 11:19): a design that reads "withheld" to Jeff is empty
  for p in overview asIs toBe implementationPlan pathToClose gaps notInScope contract validation failureMode filesInScope reference; do
    [ "$(ask "chorus:ServiceShape sh:property [ sh:path chorus:$p ; chorus:exposure chorus:public ]" "$MODEL")" = "true" ] || { echo "$p is not public"; return 1; }
  done
}

@test "the identity design is in the graph: every ADR-029 section on service-identity" {
  got="$(missing_sections "$SVC")"
  if [ -n "$got" ]; then echo "missing:$got"; return 1; fi
  # a contract per consumer and more than one failure mode, not one blob
  n=$(arq --data "$SVC" --results=csv "$P SELECT (COUNT(?c) AS ?n) WHERE { chorus:service-identity chorus:contract ?c }" 2>/dev/null | tail -1 | tr -d '\r')
  [ "$n" -ge 4 ] || { echo "only $n contracts"; return 1; }
  # ADR-029's mandatory diagrams: a consumer flowchart and a sequence for the critical path
  # ADR-029: consumer, As-Is and To-Be flowcharts, and a sequence for the critical path
  nf=$(arq --data "$SVC" --results=csv "$P SELECT (COUNT(?d) AS ?n) WHERE { chorus:service-identity chorus:diagram ?d . FILTER(CONTAINS(?d, 'flowchart')) }" 2>/dev/null | tail -1 | tr -d '\r')
  [ "$nf" -ge 3 ] || { echo "only $nf flowcharts"; return 1; }
  [ "$(ask 'chorus:service-identity chorus:diagram ?d . FILTER(CONTAINS(?d, "As is"))' "$SVC")" = "true" ]
  [ "$(ask 'chorus:service-identity chorus:diagram ?d . FILTER(CONTAINS(?d, "To be"))' "$SVC")" = "true" ]
  [ "$(ask 'chorus:service-identity chorus:diagram ?d . FILTER(CONTAINS(?d, "sequenceDiagram"))' "$SVC")" = "true" ]
}

@test "NEGATIVE PROOF: a service row missing a section fails the check" {
  bad="$BATS_TEST_TMPDIR/svc.ttl"
  grep -v 'chorus:validation """' "$SVC" > "$bad"
  run missing_sections "$bad"
  [ "$output" = " validation" ] || { echo "got [$output]"; return 1; }
}

@test "consumers are read, not typed: the surfaces that sign people in depend on identity" {
  for c in clearing gathering-app; do
    [ "$(ask "chorus:service-$c chorus:dependsOn chorus:service-identity" "$SVC")" = "true" ] || { echo "$c does not dependsOn identity"; return 1; }
  done
}

@test "the service page renders the sections in ADR-029's order and finds consumers inversely" {
  order="$(grep -oE "^\s+\['[a-zA-Z]+', +'[A-Z][a-z]+( [a-z]+)*'" "$PAGE" | sed -E "s/.*'([A-Z][a-z]+( [a-z]+)*)'.*/\1/" | tr '\n' '|')"
  [ "$order" = "Promise|Consumers|Contracts|Validation|Failure modes|As is|To be|Migration plan|Files in scope|Gaps|Not in scope|Path to close|References|" ] || { echo "order: $order"; return 1; }
  grep -q 'asList(r.dependsOn)' "$PAGE"
  if grep -q 'service-identity\|chorus:identity' "$PAGE"; then echo "the page names one service"; return 1; fi
}
