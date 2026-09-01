#!/usr/bin/env bats
# @test-type: contract — SHACL conformance of ProductShape's consumes/provides
# branches, run through Jena `shacl` against fixtures this test authors itself.
#
# #4053 — Jeff, reading the atlas: "there is no dependency from products class to
# services class at all." chorus:consumes appears 79x in the data (84 Domain
# targets, 21 Service) and ProductShape declared it nowhere, so it was authored,
# ungoverned, unserved and undrawable.
#
# Silas's ruling 2026-09-01: consumes is ONE verb — depends-on-a-capability —
# and Domain vs Service is granularity of target, not a different relation. So
# sh:or, not two predicates. He attached a condition, and it is the point of
# this file: "a mis-authored two-branch sh:or constrains nothing and still reads
# green. Ship a fixture where consumes points at NEITHER and show the shape
# FAILING." His sharpening: Product is the target that matters, because it is
# the plausible wrong answer a broken sh:or would wave through.

SHAPES="${BATS_TEST_DIRNAME}/../../roles/silas/ontology/chorus.ttl"
NS="https://jeffbridwell.com/chorus#"

setup() {
  command -v shacl >/dev/null || skip "Jena shacl not on PATH"
  [ -f "$SHAPES" ] || skip "shapes file not found: $SHAPES"
  WORK="$(mktemp -d)"
}

teardown() { [ -n "${WORK:-}" ] && rm -rf "$WORK"; }

# Writes a Product whose consumes points at $2 (a full turtle object term),
# with $1 naming the target's own rdf:type declaration (may be empty).
fixture() {
  local decl="$1" target="$2"
  cat > "$WORK/data.ttl" <<TTL
@prefix chorus: <${NS}> .
@prefix rdfs:   <http://www.w3.org/2000/01/rdf-schema#> .
chorus:fixture-product a chorus:Product ;
  rdfs:label "Fixture" ; rdfs:comment "fixture" ;
  chorus:vision "v" ; chorus:valueProposition "vp" ; chorus:audience "a" ;
  chorus:gaps "g" ; chorus:status "operating" ;
  chorus:ownedBy chorus:role-wren ;
  chorus:atStep chorus:value-stream-step-directing ;
  chorus:hasDesignDoc chorus:fixture-doc ;
  chorus:hasDomain chorus:fixture-domain ;
  chorus:consumes ${target} .
${decl}
chorus:role-wren a chorus:Role .
chorus:value-stream-step-directing a chorus:ValueStreamStep .
chorus:fixture-doc a chorus:Document .
chorus:fixture-domain a chorus:Domain .
TTL
}

# True when the validation report blames the consumes path specifically —
# ProductShape has other minCount rules, so "report is non-empty" would be a
# hollow assertion that any malformed fixture could satisfy.
consumes_violation() {
  shacl validate --shapes "$SHAPES" --data "$WORK/data.ttl" 2>/dev/null \
    | grep -A6 -i "resultPath" | grep -qi "consumes"
}

@test "consumes -> a Service CONFORMS (the shape does not over-constrain)" {
  fixture "chorus:fixture-service a chorus:Service ." "chorus:fixture-service"
  run consumes_violation
  [ "$status" -ne 0 ]
}

@test "consumes -> a Domain CONFORMS — 84 of the live targets are Domains" {
  fixture "" "chorus:fixture-domain"
  run consumes_violation
  [ "$status" -ne 0 ]
}

# ── Silas's condition: the branches must actually exclude something ──────────

@test "NEGATIVE: consumes -> a Product FAILS — the plausible wrong answer" {
  fixture "chorus:other-product a chorus:Product ." "chorus:other-product"
  run consumes_violation
  [ "$status" -eq 0 ]
}

@test "NEGATIVE: consumes -> a literal FAILS — not a node of either branch" {
  fixture "" '"not-a-node"'
  run consumes_violation
  [ "$status" -eq 0 ]
}

@test "NEGATIVE: consumes -> an untyped node FAILS — membership is by type, not by IRI shape" {
  fixture "" "chorus:some-untyped-thing"
  run consumes_violation
  [ "$status" -eq 0 ]
}
