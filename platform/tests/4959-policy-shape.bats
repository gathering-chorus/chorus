#!/usr/bin/env bats
# @test-type: contract — runs SHACL over fixtures written to BATS_TEST_TMPDIR against
# @domain: policies — the product domain this suite guards (#4334)
# @card: #4959
# @owner: silas
# the repo's PolicyShape; no store, no service, no network.
# #4959 AC1 — a Policy can carry a check a machine decides.
#
# A rule that gates must say which gate runs it (gate), how it is checked
# (checkKind + checkSpec), how many violations already exist (baseline, the
# ratchet ceiling), and which replay of a real violation must trip it
# (violationFixture: Jeff only wants rules a machine can decide). A rule with
# no check stays a plain policy and is untouched. The negatives each put a
# check-bearing row in the state the shape exists to refuse (#3734); the
# controls show a well-formed row and a plain legacy row still pass.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  SHAPES="$BATS_TEST_TMPDIR/shapes.ttl"
  # PolicyShape alone: CoreShape's door-stamp rule is a SPARQL constraint proven by its own suite (#4576).
  cp "$REPO/roles/wren/ontology/policies-4077.ttl" "$SHAPES"
  TMP="$BATS_TEST_TMPDIR"
  HEAD='@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
chorus:principal-silas a chorus:Principal .
'
}

verdict() {
  local out
  # shacl exits non-zero on a violation; bats runs with errexit, so keep the report.
  out=$(shacl validate --shapes "$SHAPES" --data "$1" 2>/dev/null) || true
  # A run that never reached a verdict (parse error, missing tool) is neither: it fails loud,
  # so a broken shape cannot pass every negative by default (#3734).
  if echo "$out" | grep -q "sh:conforms *true"; then echo CONFORMS
  elif echo "$out" | grep -q "sh:conforms *false"; then echo VIOLATES
  else echo "NO-VERDICT: $out" | head -3; fi
}

row() { printf '%s%s\n' "$HEAD" "$1" > "$TMP/$2.ttl"; verdict "$TMP/$2.ttl"; }

GOOD='chorus:policy-no-raw-kill a chorus:Policy ; rdfs:label "No raw process killing" ;
  rdfs:comment "use launchctl kickstart" ; chorus:ownedBy chorus:principal-silas'

@test "shape files are valid turtle" {
  run riot --validate "$REPO/roles/wren/ontology/policies-4077.ttl"
  [ "$status" -eq 0 ]
}

@test "control: a plain policy with no check still PASSES" {
  [ "$(row "$GOOD"' .' plain)" = CONFORMS ]
}

@test "control: a check-bearing policy with gate, check, baseline and fixture PASSES" {
  [ "$(row "$GOOD"' ; chorus:gate "arch" ; chorus:status "ratchet" ; chorus:baseline 3 ;
    chorus:checkKind "sparql" ; chorus:checkSpec "SELECT ?s WHERE { ?s a chorus:Nothing }" ;
    chorus:violationFixture "platform/tests/fixtures/4959/raw-kill.ttl" .' good)" = CONFORMS ]
}

@test "negative: a gate outside arch|data|quality|code|product|hook FAILS" {
  [ "$(row "$GOOD"' ; chorus:gate "vibes" ; chorus:baseline 0 ; chorus:checkKind "sparql" ;
    chorus:checkSpec "q" ; chorus:violationFixture "f" .' badgate)" = VIOLATES ]
}

@test "negative: a status outside active|ratchet|retired FAILS" {
  [ "$(row "$GOOD"' ; chorus:status "maybe" .' badstatus)" = VIOLATES ]
}

@test "negative: a negative baseline FAILS" {
  [ "$(row "$GOOD"' ; chorus:gate "code" ; chorus:baseline -1 ; chorus:checkKind "test" ;
    chorus:checkSpec "t" ; chorus:violationFixture "f" .' negbase)" = VIOLATES ]
}

@test "negative: a check with no violation fixture FAILS (a machine cannot prove it decides)" {
  [ "$(row "$GOOD"' ; chorus:gate "code" ; chorus:baseline 0 ; chorus:checkKind "test" ;
    chorus:checkSpec "t" .' nofixture)" = VIOLATES ]
}

@test "negative: a check with no gate FAILS" {
  [ "$(row "$GOOD"' ; chorus:baseline 0 ; chorus:checkKind "test" ; chorus:checkSpec "t" ;
    chorus:violationFixture "f" .' nogate)" = VIOLATES ]
}

@test "negative: a check with no baseline FAILS" {
  [ "$(row "$GOOD"' ; chorus:gate "code" ; chorus:checkKind "test" ; chorus:checkSpec "t" ;
    chorus:violationFixture "f" .' nobase)" = VIOLATES ]
}

@test "negative: a check with no checkSpec FAILS" {
  [ "$(row "$GOOD"' ; chorus:gate "code" ; chorus:baseline 0 ; chorus:checkKind "test" ;
    chorus:violationFixture "f" .' nospec)" = VIOLATES ]
}

@test "control: a policy derived from two ADRs PASSES (derivedFrom is many-valued)" {
  [ "$(row "$GOOD"' ; chorus:derivedFrom chorus:adr-052, chorus:decision-048 .' twoderived)" = CONFORMS ]
}

@test "negative: a checkKind outside sparql|test|command|runtime FAILS" {
  [ "$(row "$GOOD"' ; chorus:gate "code" ; chorus:baseline 0 ; chorus:checkKind "hope" ;
    chorus:checkSpec "t" ; chorus:violationFixture "f" .' badkind)" = VIOLATES ]
}

# The GovernanceCheck fold rides the follow-on land with the policy rows: the store learns
# this shape only after the land (athena-land runs in cicd-land's success handler), and
# retiring the class before its 11 rows move would orphan them.
