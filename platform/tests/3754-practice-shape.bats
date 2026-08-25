#!/usr/bin/env bats
# @test-type: unit — runs SHACL over fixtures written to BATS_TEST_TMPDIR against
# the repo's PracticeShape; no store, no service, no network.
# #3754 — PracticeShape's five proofs.
#
# The shape exists to make one class of rot IMPOSSIBLE: `expresses` pointing at
# something that is not a live Principle. Eleven of twenty-three current targets
# are already dangling, which is what an unconstrained property produces — the
# page reads fine while the model lies.
#
# Four of these are NEGATIVE proofs (#3734): each puts the model in the exact
# state the constraint exists to catch and shows SHACL report non-conformance.
# The fifth is the control — a well-formed practice must still PASS, or the
# shape would be "everything fails", which separates nothing.

setup() {
  REPO="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}"
  SHAPE="$REPO/roles/kade/ontology/practices-3754.ttl"
  TMP="$BATS_TEST_TMPDIR"
  PREFIX='@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
'
  # A live Principle + Policy the fixtures can legitimately point at.
  WORLD='chorus:principle-live a chorus:Principle ; rdfs:label "live" ; rdfs:comment "c" .
chorus:policy-live a chorus:Policy ; rdfs:label "pol" .
'
}

# Print "CONFORMS" / "VIOLATES" for a data fixture against the shape.
verdict() {
  local data="$1"
  local out
  out=$(shacl validate --shapes "$SHAPE" --data "$data" 2>/dev/null)
  if echo "$out" | grep -q "sh:conforms *true"; then echo "CONFORMS"; else echo "VIOLATES"; fi
}

@test "shape file is valid turtle and declares its instances graph" {
  run riot --validate "$SHAPE"
  [ "$status" -eq 0 ]
  grep -q 'chorus:instancesGraph "urn:chorus:domains:practices"' "$SHAPE"
}

# --- the control: this MUST pass, or the shape separates nothing -------------

@test "control: a well-formed v2 practice PASSES" {
  printf '%s%s%s' "$PREFIX" "$WORLD" 'chorus:practice-ok a chorus:Practice ;
    rdfs:label "Test-Driven Development" ; rdfs:comment "write the test first" ;
    chorus:expresses chorus:principle-live ;
    chorus:operationalizes chorus:policy-live ;
    chorus:enactedBy "werk-test" .
' > "$TMP/ok.ttl"
  [ "$(verdict "$TMP/ok.ttl")" = "CONFORMS" ]
}

# --- negative proofs ---------------------------------------------------------

@test "negative: expresses -> a retired/dangling IRI FAILS" {
  # the live rot: 11 of 23 targets look exactly like this — an IRI with no
  # Principle behind it. Unconstrained, this validated clean.
  printf '%s%s%s' "$PREFIX" "$WORLD" 'chorus:practice-dangling a chorus:Practice ;
    rdfs:label "p" ; rdfs:comment "c" ;
    chorus:expresses chorus:principle-retired-does-not-exist .
' > "$TMP/dangling.ttl"
  [ "$(verdict "$TMP/dangling.ttl")" = "VIOLATES" ]
}

@test "negative: expresses -> a non-Principle (wrong type) FAILS" {
  printf '%s%s%s' "$PREFIX" "$WORLD" 'chorus:practice-wrongtype a chorus:Practice ;
    rdfs:label "p" ; rdfs:comment "c" ;
    chorus:expresses chorus:policy-live .
' > "$TMP/wrongtype.ttl"
  [ "$(verdict "$TMP/wrongtype.ttl")" = "VIOLATES" ]
}

@test "negative: a practice with NO expresses at all FAILS" {
  printf '%s%s%s' "$PREFIX" "$WORLD" 'chorus:practice-orphan a chorus:Practice ;
    rdfs:label "p" ; rdfs:comment "c" .
' > "$TMP/orphan.ttl"
  [ "$(verdict "$TMP/orphan.ttl")" = "VIOLATES" ]
}

@test "negative: operationalizes -> a non-Policy FAILS (optional, but typed)" {
  printf '%s%s%s' "$PREFIX" "$WORLD" 'chorus:practice-badpolicy a chorus:Practice ;
    rdfs:label "p" ; rdfs:comment "c" ;
    chorus:expresses chorus:principle-live ;
    chorus:operationalizes chorus:principle-live .
' > "$TMP/badpolicy.ttl"
  [ "$(verdict "$TMP/badpolicy.ttl")" = "VIOLATES" ]
}

@test "declared-not-enacted is legal: no enactedBy still PASSES" {
  # Energized Work and Slack are real XP practices we do NOT enact. The model
  # must be able to say so honestly rather than omit them or fake a binding.
  printf '%s%s%s' "$PREFIX" "$WORLD" 'chorus:practice-declared a chorus:Practice ;
    rdfs:label "Energized Work" ; rdfs:comment "declared, not enacted" ;
    chorus:expresses chorus:principle-live .
' > "$TMP/declared.ttl"
  [ "$(verdict "$TMP/declared.ttl")" = "CONFORMS" ]
}
