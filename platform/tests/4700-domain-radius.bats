#!/usr/bin/env bats
# @test-type: unit
# @domain: ontology
# 4700-domain-radius.bats — domain-radius, and product-radius's value stream + step,
# on a raw-graph fixture so every flag is computed by scripts/domain-radius,
# scripts/product-radius and lib/radius_graph, never planted in the fixture (#4700).

setup() {
  export DIR="$BATS_TEST_TMPDIR"
  export DR="${BATS_TEST_DIRNAME}/../scripts/domain-radius"
  export PR="${BATS_TEST_DIRNAME}/../scripts/product-radius"
  # Clean pulse/awareness slice of the graph, as fetch_graph returns it.
  cat > "$DIR/clean.json" << 'FIXTURE'
{
  "types": {
    "Product": ["pulse", "clearing"],
    "Domain": ["awareness", "messages"],
    "Service": ["service-pulse"],
    "Pipeline": ["pipeline-cicd"],
    "ValueStream": ["value-stream-chorus"],
    "ValueStreamStep": ["value-stream-step-directing"],
    "Skill": ["skill-loom-gemba", "skill-clearing-nudge"],
    "PipelineStep": ["cicd-step-1"],
    "StepSkill": ["cicd-1-nudge"]
  },
  "edges": {
    "hasDomain": [["pulse", "awareness"], ["clearing", "messages"],
                  ["skill-loom-gemba", "awareness"], ["skill-clearing-nudge", "messages"]],
    "consumes": [["clearing", "awareness"], ["pulse", "messages"]],
    "hosts": [["awareness", "service-pulse"]],
    "atStep": [["awareness", "value-stream-step-directing"], ["messages", "value-stream-step-directing"]],
    "inStream": [["value-stream-step-directing", "value-stream-chorus"]],
    "automates": [["pipeline-cicd", "value-stream-chorus"]],
    "hasStep": [["pipeline-cicd", "cicd-step-1"]],
    "forStep": [["cicd-1-nudge", "cicd-step-1"]],
    "callsSkill": [["cicd-1-nudge", "skill-clearing-nudge"]],
    "ownedBy": [["pulse", "principal-wren"], ["awareness", "principal-wren"],
                ["clearing", "principal-abby-normal"], ["messages", "principal-wren"]],
    "purpose": [["awareness", "what each actor is doing"], ["messages", "who said what to whom"]]
  }
}
FIXTURE
}

# Apply a python edit to the clean fixture: plant one violation.
plant() {
  python3 -c '
import json, sys
g = json.load(open(sys.argv[1]))
exec(sys.argv[2])
json.dump(g, open(sys.argv[3], "w"))
' "$DIR/clean.json" "$1" "$DIR/planted.json"
}

@test "domain-radius renders the agreed tree for awareness, read from the fixture" {
  run "$DR" awareness --from-fixture "$DIR/clean.json"
  [ "$status" -eq 0 ]
  expected='domain-radius awareness
├─ owner wren · product pulse · value stream chorus · step directing
├─ services pulse · pipelines — · skills loom-gemba
├─ platform     not modelled
└─ read by      clearing'
  [ "$output" = "$expected" ] || { echo "got:"; echo "$output"; return 1; }
}

@test "domain-radius finds a pipeline through StepSkill to the domain's skill" {
  run "$DR" messages --from-fixture "$DIR/clean.json"
  [ "$status" -eq 0 ]
  [[ "$output" == *"pipelines cicd · skills clearing-nudge"* ]] || return 1
}

@test "product-radius shows the product's value stream and step, derived through its domains" {
  run "$PR" pulse --from-fixture "$DIR/clean.json"
  [ "$status" -eq 0 ]
  expected='product-radius pulse
├─ owner wren · value stream chorus · step directing
├─ owns         awareness
│  └─ services pulse · pipelines — · skills loom-gemba
├─ consumes     messages   [platform: not modelled]
├─ uses tool    — (not modelled)
└─ used by      clearing (via awareness)'
  [[ "$output" == "$expected"* ]] || { echo "got:"; echo "$output"; return 1; }
}

@test "the clean fixture has no flags, so --check passes for both" {
  run "$DR" --from-fixture "$DIR/clean.json" --check
  [ "$status" -eq 0 ]
  [[ "$output" != *"flag"* ]] || return 1
  run "$PR" --from-fixture "$DIR/clean.json" --check
  [ "$status" -eq 0 ]
}

@test "negative proof: a domain linked to a non-Product is flagged" {
  plant 'g["edges"]["hasDomain"].append(["messages", "awareness"])'
  run "$DR" awareness --from-fixture "$DIR/planted.json" --check
  [ "$status" -eq 2 ]
  [[ "$output" == *"flag         owned through a link from a non-Product"* ]] || return 1
}

@test "negative proof: a skill whose domain is missing is flagged" {
  plant 'g["edges"]["hasDomain"].append(["skill-loom-gemba", "no-such-domain"])'
  run "$PR" --from-fixture "$DIR/planted.json" --check
  [ "$status" -eq 2 ]
  [[ "$output" == *"skill-missing-domain: 1"* ]] || return 1
}

@test "negative proof: a domain with no value-stream step is flagged" {
  plant 'g["edges"]["atStep"] = [e for e in g["edges"]["atStep"] if e[0] != "awareness"]'
  run "$DR" awareness --from-fixture "$DIR/planted.json" --check
  [ "$status" -eq 2 ]
  [[ "$output" == *"step —"* ]] || return 1
  [[ "$output" == *"flag         no value-stream step"* ]] || return 1
}

@test "negative proof: a service hosted by two domains is flagged" {
  plant 'g["edges"]["hosts"].append(["messages", "service-pulse"])'
  run "$PR" --from-fixture "$DIR/planted.json" --check
  [ "$status" -eq 2 ]
  [[ "$output" == *"service-hosted-twice: 1"* ]] || return 1
}

@test "--html renders the same tree, and --json carries value stream and step" {
  run "$DR" awareness --from-fixture "$DIR/clean.json" --html "$DIR/d.html"
  [ "$status" -eq 0 ]
  grep -qF "<title>Domain Radius</title>" "$DIR/d.html"
  grep -qF "├─ owner wren · product pulse · value stream chorus · step directing" "$DIR/d.html"
  run "$PR" --from-fixture "$DIR/clean.json" --html "$DIR/p.html"
  [ "$status" -eq 0 ]
  grep -qF "├─ owner wren · value stream chorus · step directing" "$DIR/p.html"
  run "$PR" --from-fixture "$DIR/clean.json" --json -
  [ "$status" -eq 0 ]
  printf '%s' "$output" | python3 -c '
import json, sys
p = {x["product"]: x for x in json.load(sys.stdin)["products"]}["pulse"]
assert p["valueStreams"] == ["value-stream-chorus"], p
assert p["steps"] == ["directing"], p'
}

@test "an unknown domain is refused, never an empty tree" {
  run "$DR" no-such-domain --from-fixture "$DIR/clean.json"
  [ "$status" -eq 1 ]
  [[ "$output" == *"no such domain"* ]] || return 1
}
