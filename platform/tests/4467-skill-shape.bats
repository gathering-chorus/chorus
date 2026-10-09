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
chorus:cicd a chorus:Domain .
EOF
  cat > "$T/good.ttl" <<'EOF'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
chorus:skill-werk-deploy-env-up a chorus:Skill ;
    rdfs:label "werk-deploy env-up" ; rdfs:comment "Brings the card's demo variant up." ;
    chorus:executor "deterministic" ; chorus:implementedBy "werk-deploy" ; chorus:arguments "env-up" ; chorus:hasDomain chorus:cicd ;
    chorus:ownedBy chorus:principal-wren .
chorus:skill-go a chorus:Skill ;
    rdfs:label "go" ; rdfs:comment "Jeff accepts the presented card." ;
    chorus:executor "human" ; chorus:hasDomain chorus:cicd ; chorus:ownedBy chorus:principal-wren .
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

@test "negative proof: a skill with no domain is refused (Jeff's traversal reaches skills through domains)" {
  run report "$T/bad.ttl"
  [[ "$output" == *"hasDomain"* ]] || { echo "$output"; false; }
}

@test "negative proof: a bad name, an executor outside the three, and a step skill with no order are refused" {
  run report "$T/bad.ttl"
  [[ "$output" == *"sh:conforms  false"* ]] || { echo "$output"; false; }
  [[ "$output" == *"skill-<name>"* ]] || false
  [[ "$output" == *"robot"* ]] || false
  [[ "$output" == *"skillOrder"* ]] || false
}

# #4471 — the skills bed of the fall cleanup.
@test "#4471 negative proof: a skill with two owners, no executor, or delegating to a non-skill is refused" {
  cat > "$T/bad4471.ttl" <<'TTL'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
chorus:principal-kade a chorus:Principal .
chorus:skill-two-keepers a chorus:Skill ;
    rdfs:label "/two" ; rdfs:comment "Has two keepers." ; chorus:executor "agent" ; chorus:hasDomain chorus:cicd ;
    chorus:ownedBy chorus:principal-wren , chorus:principal-kade .
chorus:skill-no-runner a chorus:Skill ;
    rdfs:label "/none" ; rdfs:comment "Says nothing about who runs it." ; chorus:hasDomain chorus:cicd ;
    chorus:ownedBy chorus:principal-wren .
chorus:skill-hands-off a chorus:Skill ;
    rdfs:label "/hands" ; rdfs:comment "Delegates to a domain, not a skill." ; chorus:executor "agent" ;
    chorus:hasDomain chorus:cicd ; chorus:delegatesTo chorus:cicd ; chorus:ownedBy chorus:principal-wren .
TTL
  run report "$T/bad4471.ttl"
  [[ "$output" == *"skill-two-keepers"* ]] || { echo "$output"; false; }
  [[ "$output" == *"skill-no-runner"* ]] || false
  [[ "$output" == *"skill-hands-off"* ]] || false
  [[ "$output" == *"delegatesTo"* ]] || false
}

@test "#4471 a skill delegating to a skill conforms" {
  cat > "$T/good4471.ttl" <<'TTL'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
chorus:skill-demo a chorus:Skill ;
    rdfs:label "/demo" ; rdfs:comment "Presents a card." ; chorus:executor "agent" ; chorus:hasDomain chorus:cicd ;
    chorus:delegatesTo chorus:skill-go ; chorus:ownedBy chorus:principal-wren .
chorus:skill-go a chorus:Skill ;
    rdfs:label "go" ; rdfs:comment "Jeff accepts." ; chorus:executor "human" ; chorus:hasDomain chorus:cicd ;
    chorus:ownedBy chorus:principal-wren .
TTL
  run report "$T/good4471.ttl"
  [[ "$output" == *"sh:conforms  true"* ]] || { echo "$output"; false; }
}

@test "#4471 one home: Skill is declared with its shape, and implementedIn / hasSkill are gone from the model" {
  run grep -c 'chorus:Skill a owl:Class' "$SHAPE"
  [ "$output" -eq 1 ]
  run grep -c 'chorus:Skill a owl:Class' "$ROOT/roles/silas/ontology/chorus.ttl"
  [ "$output" -eq 0 ]
  run grep -rlE 'chorus:(implementedIn|hasSkill)[[:space:]]' "$ROOT/roles" "$ROOT/designing/data" --include=*.ttl
  [ -z "$output" ] || { echo "still uses the old words: $output"; false; }
}
