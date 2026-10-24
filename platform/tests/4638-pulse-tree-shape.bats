#!/usr/bin/env bats
# @test-type: unit — validates TTL with Jena's shacl CLI; no store, no network.
# @domain: awareness
# @card: #4638
# @owner: wren
#
# #4638 — the nouns of pulse's live tree: roles/wren/ontology/pulse-tree-4638.ttl,
# loaded by platform/services/athena-deploy/src/lib.rs.
# Jeff 2026-10-19: prompt → role → turn (thoughts, streams) and card (pipeline,
# verbs, notes). Jeff 2026-10-23: the tree is generated from the model, so the
# model comes first. Two proofs: a tree's rows conform, and rows built to break
# each rule are refused with the rule named.

ROOT="$BATS_TEST_DIRNAME/../.."
SHAPE="$ROOT/roles/wren/ontology/pulse-tree-4638.ttl"

setup() {
  command -v shacl >/dev/null 2>&1 || skip "shacl (Jena) not installed"
  T="$BATS_TEST_TMPDIR"
  cat > "$T/base.ttl" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:principal-jeff a chorus:Principal .
chorus:principal-wren a chorus:Principal .
chorus:principal-silas a chorus:Principal .
chorus:session-run-wren-run-1 a chorus:SessionRun .
chorus:card-4638 a chorus:Card .
chorus:bridge-thing a chorus:Channel .
EOF
  cat > "$T/good.ttl" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
chorus:prompt-1 a chorus:Prompt ;
    chorus:promptedBy chorus:principal-jeff ; chorus:promptTo chorus:principal-wren ;
    chorus:promptedAt "2026-10-24T20:41:00Z"^^xsd:dateTime ;
    chorus:promptText "/pull 4638\ni spent all day trying to get u" ; chorus:ownedBy chorus:principal-wren .
chorus:turn-1 a chorus:Turn ;
    chorus:turnOf chorus:session-run-wren-run-1 ; chorus:answersPrompt chorus:prompt-1 ; chorus:onCard chorus:card-4638 ;
    chorus:turnStartedAt "2026-10-24T20:41:02Z"^^xsd:dateTime ; chorus:lastCallAt "2026-10-24T20:44:10Z"^^xsd:dateTime ;
    chorus:ownedBy chorus:principal-wren .
chorus:turn-nudged a chorus:Turn ;
    chorus:turnOf chorus:session-run-wren-run-1 ;
    chorus:turnStartedAt "2026-10-24T20:06:00Z"^^xsd:dateTime ; chorus:turnEndedAt "2026-10-24T20:07:00Z"^^xsd:dateTime ;
    chorus:lastCallAt "2026-10-24T20:06:40Z"^^xsd:dateTime ; chorus:ownedBy chorus:principal-wren .
chorus:note-card a chorus:Note ;
    chorus:inTurn chorus:turn-1 ; chorus:onCard chorus:card-4638 ; chorus:noteText "radius measured: no Prompt, no Turn" ;
    chorus:observedAt "2026-10-24T20:45:00Z"^^xsd:dateTime ; chorus:ownedBy chorus:principal-wren .
chorus:note-turn-only a chorus:Note ;
    chorus:inTurn chorus:turn-nudged ; chorus:noteText "presence rows cleaned" ;
    chorus:observedAt "2026-10-24T20:06:50Z"^^xsd:dateTime ; chorus:ownedBy chorus:principal-wren .
EOF
  cat > "$T/bad.ttl" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
chorus:prompt-from-bridge a chorus:Prompt ;
    chorus:promptedBy chorus:bridge-thing ;
    chorus:promptedAt "2026-10-24T20:41:00Z"^^xsd:dateTime ; chorus:promptText "hi" ; chorus:ownedBy chorus:principal-wren .
chorus:prompt-empty a chorus:Prompt ;
    chorus:promptedBy chorus:principal-jeff ; chorus:promptText "" ; chorus:ownedBy chorus:principal-wren .
chorus:turn-orphan a chorus:Turn ;
    chorus:answersPrompt chorus:card-4638 ;
    chorus:turnStartedAt "2026-10-24T20:41:02Z"^^xsd:dateTime ; chorus:ownedBy chorus:principal-wren .
chorus:note-nowhere a chorus:Note ;
    chorus:onCard chorus:card-4638 ; chorus:noteText "floating" ;
    chorus:observedAt "2026-10-24T20:45:00Z"^^xsd:dateTime ; chorus:ownedBy chorus:principal-wren .
EOF
}

# CoreShape (core-shape-4576.ttl) is left out, as in 4018-awareness-shape.bats: its fields are all
# optional and its StampedByTheDal rule needs the full model set to parse.
report() { cat "$SHAPE" "$T/base.ttl" "$1" > "$T/all.ttl"; shacl validate --shapes "$SHAPE" --data "$T/all.ttl" 2>&1; }
focus() { printf '%s\n' "$1" | grep -A8 "focusNode.*$2" ; }

@test "every property in the tree shapes is described" {
  paths=$(grep -c 'sh:path' "$SHAPE")
  [ "$(grep -c 'sh:description' "$SHAPE")" -eq "$paths" ]
}

@test "the shapes claim no rule the door does not run (no sh:sparql)" {
  [ "$(grep -c 'sh:sparql' "$SHAPE")" -eq 0 ]
}

@test "a prompt, its turn, a nudged turn, a card note and a turn-only note conform" {
  run report "$T/good.ttl"
  [[ "$output" == *"sh:conforms  true"* ]] || { echo "$output"; false; }
}

@test "negative proof: a prompt from a non-actor is refused" {
  run report "$T/bad.ttl"
  [[ "$output" == *"sh:conforms  false"* ]] || { echo "$output"; false; }
  f=$(focus "$output" prompt-from-bridge); [[ "$f" == *"promptedBy"* ]] || { echo "$output"; false; }
}

@test "negative proof: a prompt with no time and empty text is refused" {
  run report "$T/bad.ttl"
  f=$(focus "$output" prompt-empty)
  [[ "$f" == *"promptedAt"* ]] || { echo "$output"; false; }
  [[ "$f" == *"promptText"* ]] || { echo "$output"; false; }
}

@test "negative proof: a turn with no run, no last call, answering a card, is refused" {
  run report "$T/bad.ttl"
  f=$(focus "$output" turn-orphan)
  [[ "$f" == *"turnOf"* ]] || { echo "$output"; false; }
  [[ "$f" == *"lastCallAt"* ]] || { echo "$output"; false; }
  [[ "$f" == *"answersPrompt"* ]] || { echo "$output"; false; }
}

@test "negative proof: a note with no turn is refused (a note stays on its turn)" {
  run report "$T/bad.ttl"
  f=$(focus "$output" note-nowhere); [[ "$f" == *"inTurn"* ]] || { echo "$output"; false; }
}

@test "Turn and Note are short-term memory; Prompt is a record" {
  grep -A3 '^chorus:Turn a owl:Class' "$SHAPE" | grep -q 'rdfs:subClassOf chorus:ShortTermMemory'
  grep -A3 '^chorus:Note a owl:Class' "$SHAPE" | grep -q 'rdfs:subClassOf chorus:ShortTermMemory'
  [ "$(grep -A3 '^chorus:Prompt a owl:Class' "$SHAPE" | grep -c 'ShortTermMemory')" -eq 0 ]
}

@test "the tree file is in the model set athena-deploy loads" {
  grep -q 'roles/wren/ontology/pulse-tree-4638.ttl' "$ROOT/platform/services/athena-deploy/src/lib.rs"
}
