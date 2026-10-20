#!/usr/bin/env bats
# @test-type: contract — runs SHACL over fixtures written to BATS_TEST_TMPDIR against
# @domain: tests — the product domain this suite guards (#4334)
# the repo's ActorShape and PersonaShape (kade/ontology/werk-domains); no store, no network.
# #4662 (Jeff 2026-10-20 13:24) — "actor" means whoever acts; the BDD scenario
# subject is Persona. Both shapes now carry required fields: 4022 skipped Actor
# because it had none (quartet-walk.sh: "manifest carries no requiredFields").
#
# Each NEGATIVE proof (#3734) puts a row in the state the shape exists to refuse
# and shows SHACL non-conformance; the controls show a well-formed row conforms,
# so the shape is not "everything fails".

setup() {
  REPO="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}"
  SHAPES="$BATS_TEST_TMPDIR/shapes.nt"
  # Only the two shapes under test, from the repo's file: the whole file names
  # sh:sparql constraints (StampedByTheDal) defined elsewhere, and a shapes graph
  # SHACL cannot parse would make every verdict "not conforming" for the wrong
  # reason. sh:node CoreShape is dropped too: CoreShape's fields are optional.
  riot --output=nt "$REPO/roles/kade/ontology/werk-domains.ttl" 2>/dev/null \
    | grep -E '^<https://jeffbridwell\.com/chorus#(Actor|Persona)Shape' \
    | grep -vE 'shacl#(sparql|node)>' > "$SHAPES"
  PREFIX='@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
'
}

# CONFORMS, VIOLATES (a real report saying sh:conforms false), or ERROR — an
# unparseable shapes graph must never read as a refusal.
verdict() {
  local out
  out=$(shacl validate --shapes "$SHAPES" --data "$1" 2>&1)
  if echo "$out" | grep -q "sh:conforms *true"; then echo "CONFORMS"
  elif echo "$out" | grep -q "sh:conforms *false"; then echo "VIOLATES"
  else echo "ERROR"; fi
}

fixture() { printf '%s%s\n' "$PREFIX" "$1" > "$BATS_TEST_TMPDIR/$2.ttl"; echo "$BATS_TEST_TMPDIR/$2.ttl"; }

@test "both shapes are in the file, with their required fields" {
  [ "$(grep -c 'ActorShape' "$SHAPES")" -ge 4 ] || return 1
  [ "$(grep -c 'PersonaShape' "$SHAPES")" -ge 4 ] || return 1
  grep -q 'minCount' "$SHAPES" || return 1
}

@test "control: an Actor with a name and a kind conforms" {
  f=$(fixture 'chorus:a1 a chorus:Actor ; rdfs:label "Silas" ; chorus:actorKind "agent" .' a1)
  [ "$(verdict "$f")" = CONFORMS ] || return 1
}

# NEGATIVE PROOF: the state 4022 could not measure — an Actor with no required field.
@test "an Actor with no kind is refused" {
  f=$(fixture 'chorus:a2 a chorus:Actor ; rdfs:label "Silas" .' a2)
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

@test "an Actor with no name is refused" {
  f=$(fixture 'chorus:a3 a chorus:Actor ; chorus:actorKind "agent" .' a3)
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

@test "an Actor's kind is person, agent or service, nothing else" {
  f=$(fixture 'chorus:a4 a chorus:Actor ; rdfs:label "X" ; chorus:actorKind "human" .' a4)
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

@test "control: a Persona with a name and the part it plays conforms" {
  f=$(fixture 'chorus:p1 a chorus:Persona ; rdfs:label "Jeff on his phone" ; chorus:playsRole "Jeff" .' p1)
  [ "$(verdict "$f")" = CONFORMS ] || return 1
}

@test "a Persona with no part played is refused" {
  f=$(fixture 'chorus:p2 a chorus:Persona ; rdfs:label "Jeff on his phone" .' p2)
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

@test "Persona joins the tests vocabulary beside Actor; Actor keeps its home, so its route is not dropped" {
  run grep -c 'chorus:QualityFinding , chorus:Actor , chorus:Persona ;' "$REPO/roles/kade/ontology/werk-domains.ttl"
  [ "$output" -ge 1 ] || return 1
  run grep -c 'definesVocabulary chorus:Actor' "$REPO/roles/kade/ontology/werk-domains.ttl"
  [ "$output" -eq 0 ] || return 1
}

# AC 3 — nothing retired. The 10-18 retirement emptied CoreShape on 77 shapes;
# this card must add no retirement line naming Actor or Persona.
no_retirement() { ! grep -qE '"object_class": *"(Actor|Persona|ActorShape|PersonaShape)"' "$1"; }

@test "the retirements ledger names neither Actor nor Persona" {
  no_retirement "$REPO/designing/schemas/model-retirements.jsonl" || return 1
}

# NEGATIVE PROOF: a ledger that retires Actor's vocabulary line is caught.
@test "negative proof: a retirement naming Actor is caught" {
  printf '%s\n' '{"subject_domain": "tests", "predicate": "definesVocabulary", "object_class": "Actor", "status": "staged"}' > "$BATS_TEST_TMPDIR/ledger.jsonl"
  ! no_retirement "$BATS_TEST_TMPDIR/ledger.jsonl" || return 1
}
