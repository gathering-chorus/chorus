#!/usr/bin/env bats
# @test-type: unit
# @domain: ontology
# @card: #4743
# @owner: abby-normal
# 4743-pulse-product-radius.bats — prove pulse's product radius (AC3)
# Exercises: platform/scripts/product-radius, platform/scripts/lib/radius_graph.py

setup() {
  export DIR="$BATS_TEST_TMPDIR"
  export PR="${BATS_TEST_DIRNAME}/../scripts/product-radius"
}

# The measured pulse graph fixture based on the model updates in #4743
clean_fixture() {
  cat << 'FIXTURE'
{
  "types": {
    "Product": ["pulse", "chorus", "clearing"],
    "Domain": [
      "awareness", "streams", "cards", "messages", "alerts",
      "persistence", "identity", "spine", "pipelines", "metrics",
      "services", "events", "logs", "monitors", "roles", "security"
    ],
    "Service": ["pulse-service"],
    "Pipeline": ["pipeline-pulse"],
    "ValueStream": ["value-stream-chorus"],
    "ValueStreamStep": ["value-stream-step-directing"],
    "Skill": ["skill-loom-gemba"],
    "PipelineStep": ["pulse-step-1"],
    "StepSkill": ["pulse-1-gemba"]
  },
  "edges": {
    "hasDomain": [
      ["pulse", "awareness"],
      ["pulse", "streams"],
      ["clearing", "cards"],
      ["clearing", "messages"],
      ["skill-loom-gemba", "awareness"]
    ],
    "composes": [
      ["pulse", "alerts"],
      ["pulse", "cards"],
      ["pulse", "messages"]
    ],
    "consumes": [
      ["pulse", "alerts"],
      ["pulse", "cards"],
      ["pulse", "events"],
      ["pulse", "identity"],
      ["pulse", "logs"],
      ["pulse", "messages"],
      ["pulse", "metrics"],
      ["pulse", "monitors"],
      ["pulse", "persistence"],
      ["pulse", "pipelines"],
      ["pulse", "roles"],
      ["pulse", "security"],
      ["pulse", "services"],
      ["pulse", "spine"]
    ],
    "partOf": [
      ["pulse", "chorus"]
    ],
    "hosts": [
      ["awareness", "pulse-service"]
    ],
    "atStep": [
      ["awareness", "value-stream-step-directing"],
      ["streams", "value-stream-step-directing"]
    ],
    "inStream": [
      ["value-stream-step-directing", "value-stream-chorus"]
    ],
    "automates": [
      ["pipeline-pulse", "value-stream-chorus"]
    ],
    "hasStep": [
      ["pipeline-pulse", "pulse-step-1"]
    ],
    "forStep": [
      ["pulse-1-gemba", "pulse-step-1"]
    ],
    "callsSkill": [
      ["pulse-1-gemba", "skill-loom-gemba"]
    ],
    "ownedBy": [
      ["pulse", "principal-wren"],
      ["awareness", "principal-wren"],
      ["streams", "principal-wren"]
    ],
    "platform": [
      ["awareness", "false"],
      ["streams", "false"]
    ],
    "inLayer": [
      ["awareness", "layer-capability"],
      ["streams", "layer-capability"]
    ],
    "executor": [
      ["skill-loom-gemba", "deterministic"]
    ],
    "purpose": [
      ["awareness", "team state at a glance"],
      ["streams", "agent turns tailed"]
    ]
  }
}
FIXTURE
}

@test "pulse product radius returns exactly the AC1/AC2 measured domain set" {
  clean_fixture > "$DIR/clean.json"
  run "$PR" pulse --from-fixture "$DIR/clean.json"
  [ "$status" -eq 0 ]
  [[ "$output" =~ "owns         awareness" ]] || return 1
  [[ "$output" =~ "owns         streams" ]] || return 1
  [[ "$output" =~ "composes     alerts, cards, messages" ]] || return 1
  [[ "$output" =~ "consumes     alerts" ]] || return 1
  [[ "$output" =~ "persistence" ]] || return 1
  [[ "$output" =~ "identity" ]] || return 1
  [[ "$output" =~ "spine" ]] || return 1
  [[ "$output" =~ "pipelines" ]] || return 1
  [[ "$output" =~ "metrics" ]] || return 1
  [[ "$output" =~ "services" ]] || return 1
  # And verifies stale consumes are absent
  ! [[ "$output" =~ "code" ]] || return 1
  ! [[ "$output" =~ "knowledge" ]] || return 1
  ! [[ "$output" =~ "memory" ]] || return 1
  ! [[ "$output" =~ "tests" ]] || return 1
}

@test "NEGATIVE PROOF (#3734): removing one hasDomain edge causes product radius check to fail" {
  # Modify fixture to remove pulse's hasDomain awareness
  clean_fixture | python3 -c '
import json, sys
g = json.load(sys.stdin)
g["edges"]["hasDomain"] = [e for e in g["edges"]["hasDomain"] if not (e[0] == "pulse" and e[1] == "awareness")]
json.dump(g, open(sys.argv[1], "w"))
' "$DIR/missing_domain.json"

  run "$PR" pulse --from-fixture "$DIR/missing_domain.json"
  [ "$status" -eq 0 ]
  # awareness is no longer owned by pulse
  ! [[ "$output" =~ "owns         awareness" ]] || return 1

  # And if BOTH hasDomain edges are removed, product-radius flags product-no-domains
  clean_fixture | python3 -c '
import json, sys
g = json.load(sys.stdin)
g["edges"]["hasDomain"] = [e for e in g["edges"]["hasDomain"] if e[0] != "pulse"]
g["edges"]["partOf"] = [e for e in g["edges"]["partOf"] if e[1] != "pulse"]
json.dump(g, open(sys.argv[1], "w"))
' "$DIR/no_domains.json"

  run "$PR" pulse --from-fixture "$DIR/no_domains.json" --check
  [ "$status" -eq 2 ]
  [[ "$output" =~ "product-no-domains" ]] || return 1
}
