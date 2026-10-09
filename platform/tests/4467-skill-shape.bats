#!/usr/bin/env bats
# @test-type: unit — validates TTL with Jena's shacl CLI; no store, no network.
# @domain: skills
# @card: #4467
# @owner: wren
#
# #4467 — Jeff, 2026-10-09: "we would make the tables first". The skills shape
# lands before chorus-make (#4465) reads rows. A verb is a deterministic skill
# (Jeff 08-22), so one Skill class carries an executor; StepSkill orders the
# skills inside a pipeline step. Two proofs: good rows conform, and rows built to
# break each rule are refused, each violation named.

ROOT="$BATS_TEST_DIRNAME/../.."
SHAPE="$ROOT/roles/wren/ontology/skills-4467.ttl"

setup() {
  command -v shacl >/dev/null 2>&1 || skip "shacl (Jena) not installed"
  T="$BATS_TEST_TMPDIR"
  cat > "$T/base.ttl" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
chorus:principal-wren a chorus:Principal .
chorus:pipeline-step-cicd-demo a chorus:PipelineStep .
EOF
  cat > "$T/good.ttl" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
chorus:skill-werk-deploy-env-up a chorus:Skill ;
    rdfs:label "werk-deploy env-up" ; rdfs:comment "Brings the card's demo variant up." ;
    chorus:executor "deterministic" ; chorus:implementedBy "werk-deploy" ; chorus:arguments "env-up" ;
    chorus:ownedBy chorus:principal-wren .
chorus:skill-go a chorus:Skill ;
    rdfs:label "go" ; rdfs:comment "Jeff accepts the presented card." ;
    chorus:executor "human" ; chorus:ownedBy chorus:principal-wren .
chorus:stepskill-cicd-demo-1 a chorus:StepSkill ;
    chorus:forStep chorus:pipeline-step-cicd-demo ; chorus:callsSkill chorus:skill-werk-deploy-env-up ;
    chorus:skillOrder 1 ; chorus:ownedBy chorus:principal-wren .
EOF
  cat > "$T/bad.ttl" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
chorus:Werk_Deploy a chorus:Skill ;
    rdfs:label "werk-deploy" ; rdfs:comment "Named off the format." ;
    chorus:executor "robot" ; chorus:ownedBy chorus:principal-wren .
chorus:stepskill-cicd-demo-2 a chorus:StepSkill ;
    chorus:forStep chorus:pipeline-step-cicd-demo ; chorus:callsSkill chorus:Werk_Deploy ;
    chorus:ownedBy chorus:principal-wren .
EOF
}

report() { cat "$SHAPE" "$T/base.ttl" "$1" > "$T/all.ttl"; shacl validate --shapes "$SHAPE" --data "$T/all.ttl" 2>&1; }

@test "every property in the skills shape is described" {
  run grep -c 'sh:path' "$SHAPE"
  paths="$output"
  run grep -c 'sh:description' "$SHAPE"
  [ "$output" -eq "$paths" ]
}

@test "the shape claims no rule the door does not run (no sh:sparql)" {
  run grep -c 'sh:sparql' "$SHAPE"
  [ "$output" -eq 0 ]
}

@test "good skills and a step's ordered skill conform" {
  run report "$T/good.ttl"
  [[ "$output" == *"sh:conforms  true"* ]] || { echo "$output"; false; }
}

@test "negative proof: a bad name, an executor outside the three, and a step skill with no order are refused" {
  run report "$T/bad.ttl"
  [[ "$output" == *"sh:conforms  false"* ]] || { echo "$output"; false; }
  [[ "$output" == *"skill-<name>"* ]] || false
  [[ "$output" == *"robot"* ]] || false
  [[ "$output" == *"skillOrder"* ]] || false
}
