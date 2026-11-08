#!/usr/bin/env bats
# @test-type: integration:api — the shape cases run SHACL on fixtures; case 6 reads the live Teammate row through the door
# @domain: roles
# #4933 AC1 — a Teammate is an Actor (decided #4662, 10-20; unbuilt until now), and
# Borg Queen is a Teammate like the other agents. The login reads the Teammate row
# (AC2), so a role with no row cannot open a session.
#
# NEGATIVE PROOF (#3734): a Teammate with neither an actorKind nor a Principal
# (signsInAs) must VIOLATE once Teammate is a subclass of Actor. The kind is not
# copied onto Teammate rows: signsInAs is enough, because the Principal carries
# principalKind (in security, a graph the roles validation cannot see), so the
# requirement is sh:or ( actorKind | signsInAs ) and the five live rows need no patch.

setup() {
  # the checkout this test file is in, never the one an env var names (Silas 17:48)
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  ONT="$REPO/roles/wren/ontology/hats-4175.ttl"
  SHAPES="$BATS_TEST_TMPDIR/shapes.nt"
  riot --output=nt "$REPO/roles/kade/ontology/werk-domains.ttl" 2>/dev/null \
    | grep -E '^(_:|<https://jeffbridwell\.com/chorus#ActorShape)' \
    | grep -vE 'shacl#(sparql|node)>' > "$SHAPES"
  SUB=$(riot --output=nt "$ONT" 2>/dev/null \
    | grep -F '<https://jeffbridwell.com/chorus#Teammate> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://jeffbridwell.com/chorus#Actor>' || true)
  PREFIX='@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
'
  DOOR="${CHORUS_DOOR:-http://localhost:3340/owl/v1}"
}

verdict() {
  local out
  out=$(shacl validate --shapes "$SHAPES" --data "$1" 2>&1)
  if echo "$out" | grep -q "sh:conforms *true"; then echo "CONFORMS"
  elif echo "$out" | grep -q "sh:conforms *false"; then echo "VIOLATES"
  else echo "ERROR"; fi
}

# The fixture carries the model's own subClassOf line (empty if the model lacks it),
# so the proof tests the repo's model, not a line the test wrote.
fixture() { printf '%s%s\n%s\n' "$PREFIX" "$SUB" "$1" > "$BATS_TEST_TMPDIR/$2.ttl"; echo "$BATS_TEST_TMPDIR/$2.ttl"; }

@test "the model says Teammate is a subclass of Actor" {
  [ -n "$SUB" ] || { echo "no 'chorus:Teammate rdfs:subClassOf chorus:Actor' in $ONT"; return 1; }
}

@test "negative proof: a Teammate with no kind and no Principal violates ActorShape" {
  f=$(fixture 'chorus:t1 a chorus:Teammate ; rdfs:label "Kade" .' t1)
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

@test "negative proof: a Teammate whose stated kind is not person/agent/service violates" {
  f=$(fixture 'chorus:t3 a chorus:Teammate ; rdfs:label "Kade" ; chorus:actorKind "robot" .' t3)
  [ "$(verdict "$f")" = VIOLATES ] || return 1
}

@test "control: a Teammate that signs in as a Principal conforms, with no actorKind copy" {
  f=$(fixture 'chorus:t2 a chorus:Teammate ; rdfs:label "Kade" ; chorus:signsInAs chorus:p2 .' t2)
  [ "$(verdict "$f")" = CONFORMS ] || return 1
}

@test "the generator sees no blank-node field on Teammate (every inherited property path is an IRI)" {
  data="$BATS_TEST_TMPDIR/ont.nt"
  riot --output=nt "$REPO/roles/silas/ontology/chorus.ttl" "$REPO/roles/kade/ontology/werk-domains.ttl" "$ONT" 2>/dev/null > "$data"
  out=$(arq --data "$data" --results=csv 'PREFIX sh: <http://www.w3.org/ns/shacl#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#> SELECT ?p WHERE { <https://jeffbridwell.com/chorus#Teammate> rdfs:subClassOf* ?tc . ?s sh:targetClass ?tc ; sh:property ?p . ?p sh:path ?path . FILTER(!isIRI(?path)) }' | tail -n +2)
  [ -z "$out" ] || { echo "blank-node paths: $out"; return 1; }
}

@test "borg-queen has a Teammate row: signsInAs principal-borg-queen, servesAs role-borg-queen" {
  run curl -sf --max-time 5 "$DOOR/roles/teammates"
  [ "$status" -eq 0 ] || { echo "door unreachable"; return 1; }
  echo "$output" | python3 -c '
import sys, json
rows = {r["name"]: r for r in json.load(sys.stdin)["data"]}
bq = rows.get("borg-queen")
assert bq, "no borg-queen Teammate row: " + ",".join(sorted(rows))
assert bq.get("signsInAs") == "principal-borg-queen", bq
sa = bq.get("servesAs"); sa = sa if isinstance(sa, list) else [sa]
assert "role-borg-queen" in sa, bq
'
}
