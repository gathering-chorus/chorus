#!/usr/bin/env bats
# @test-type: unit
# @domain: domains
# 4700-domain-radius.bats — domain-radius, and product-radius's value stream + step,
# on a raw-graph fixture so every flag is computed, never planted in the fixture (#4700).
# Exercises: platform/scripts/domain-radius, platform/scripts/product-radius,
#            platform/scripts/lib/radius_graph.py

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
    "consumes": [["clearing", "awareness"]],
    "hosts": [["awareness", "service-pulse"]],
    "atStep": [["awareness", "value-stream-step-directing"], ["messages", "value-stream-step-directing"]],
    "inStream": [["value-stream-step-directing", "value-stream-chorus"]],
    "automates": [["pipeline-cicd", "value-stream-chorus"]],
    "hasStep": [["pipeline-cicd", "cicd-step-1"]],
    "forStep": [["cicd-1-nudge", "cicd-step-1"]],
    "callsSkill": [["cicd-1-nudge", "skill-clearing-nudge"]],
    "ownedBy": [["pulse", "principal-wren"], ["awareness", "principal-wren"],
                ["clearing", "principal-abby-normal"], ["messages", "principal-wren"]],
    "platform": [["awareness", "false"], ["messages", "true"], ["service-pulse", "false"]],
    "composes": [["pulse", "messages"]],
    "inLayer": [["awareness", "layer-capability"], ["messages", "layer-foundation"]],
    "executor": [["skill-loom-gemba", "deterministic"], ["skill-clearing-nudge", "deterministic"]],
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
├─ platform     no · layer capability
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
├─ composes     messages
├─ consumes     —
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

@test "negative proof: a domain with no layer is flagged" {
  plant 'g["edges"]["inLayer"] = [e for e in g["edges"]["inLayer"] if e[0] != "awareness"]'
  run "$DR" awareness --from-fixture "$DIR/planted.json" --check
  [ "$status" -eq 2 ] || return 1
  [[ "$output" == *"layer —"* ]] || return 1
  [[ "$output" == *"flag         no layer"* ]] || return 1
}

@test "negative proof: a skill that runs a pipeline with no Pipeline row is flagged on its domain" {
  plant 'g["edges"]["executor"] = [["skill-clearing-nudge", "pipeline"]]; g["edges"]["implementedBy"] = [["skill-clearing-nudge", "no-such-pipeline"]]'
  run "$DR" messages --from-fixture "$DIR/planted.json" --check
  [ "$status" -eq 2 ] || return 1
  [[ "$output" == *"flag         runs a pipeline that has no Pipeline row"* ]] || return 1
  # and the same skill pointed at a real Pipeline row is clean
  plant 'g["edges"]["executor"] = [["skill-clearing-nudge", "pipeline"]]; g["edges"]["implementedBy"] = [["skill-clearing-nudge", "cicd"]]'
  run "$DR" messages --from-fixture "$DIR/planted.json" --check
  [ "$status" -eq 0 ] || return 1
}

@test "negative proof: a value stream no product's domain reaches is flagged" {
  plant 'g["types"]["ValueStream"].append("value-stream-orphan")'
  run "$PR" --from-fixture "$DIR/planted.json" --check
  [ "$status" -eq 2 ] || return 1
  [[ "$output" == *"valuestream-no-product: 1"* ]] || return 1
}

@test "a link to a non-Domain name is flagged, never rendered as a domain" {
  plant 'g["edges"]["hasDomain"].append(["pulse", "ghost"])'
  run "$DR" --from-fixture "$DIR/planted.json"
  [ "$status" -eq 0 ] || return 1
  [[ "$output" != *"domain-radius ghost"* ]] || return 1
  run "$PR" --from-fixture "$DIR/planted.json" --check
  [ "$status" -eq 2 ] || return 1
  [[ "$output" == *"link-to-missing-domain: 1"* ]] || return 1
}

@test "named products filter --json and --html too, and parts get a row" {
  plant 'g["edges"]["partOf"] = [["clearing", "pulse"]]'
  run "$PR" pulse --from-fixture "$DIR/planted.json" --json -
  [ "$status" -eq 0 ] || return 1
  printf '%s' "$output" | python3 -c '
import json, sys
assert [p["product"] for p in json.load(sys.stdin)["products"]] == ["pulse"]' || return 1
  run "$PR" pulse --from-fixture "$DIR/planted.json"
  [[ "$output" == *"├─ parts        clearing"* ]] || return 1
}

@test "negative proof: a domain with no platform flag is flagged undecided" {
  plant 'g["edges"]["platform"] = [e for e in g["edges"]["platform"] if e[0] != "awareness"]'
  run "$DR" awareness --from-fixture "$DIR/planted.json" --check
  [ "$status" -eq 2 ] || return 1
  [[ "$output" == *"platform     undecided"* ]] || return 1
  [[ "$output" == *"flag         platform flag not written (true or false)"* ]] || return 1
}

@test "negative proof: a product that consumes a platform domain is flagged" {
  plant 'g["edges"]["consumes"].append(["pulse", "messages"])'
  run "$PR" --from-fixture "$DIR/planted.json" --check
  [ "$status" -eq 2 ] || return 1
  [[ "$output" == *"consumes-platform: 1"* ]] || return 1
}

@test "domain-radius shows who composes a platform domain" {
  run "$DR" messages --from-fixture "$DIR/clean.json"
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"platform     yes · layer foundation"* ]] || return 1
  [[ "$output" == *"read by      pulse (composes)"* ]] || return 1
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
