#!/usr/bin/env bats
# @test-type: unit
# @domain: ontology
# 4702-shape-drift.bats — platform/scripts/shape-drift lists the fields live rows
# carry that their shape neither lists nor ignores, so a shape is closed only
# when it reads clean (#4702 AC5). Fixture in the tool's own format; no store.

setup() {
  export SD="${BATS_TEST_DIRNAME}/../scripts/shape-drift"
  export DIR="$BATS_TEST_TMPDIR"
  cat > "$DIR/clean.json" << 'FIXTURE'
{
  "shapes": {
    "ProductShape": {"target": "Product",
      "paths": ["https://jeffbridwell.com/chorus#hasDomain", "http://www.w3.org/2000/01/rdf-schema#label"],
      "ignored": ["http://www.w3.org/1999/02/22-rdf-syntax-ns#type"]},
    "DomainShape": {"target": "Domain",
      "paths": ["https://jeffbridwell.com/chorus#atStep"], "ignored": []}
  },
  "rows": {
    "Product": {"https://jeffbridwell.com/chorus#hasDomain": 11,
                "http://www.w3.org/2000/01/rdf-schema#label": 11,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type": 11},
    "Domain": {"https://jeffbridwell.com/chorus#atStep": 64}
  }
}
FIXTURE
}

plant() {
  python3 -c '
import json, sys
d = json.load(open(sys.argv[1]))
exec(sys.argv[2])
json.dump(d, open(sys.argv[3], "w"))
' "$DIR/clean.json" "$1" "$DIR/planted.json"
}

@test "a shape whose rows carry only its paths and ignored fields reads clean, and --check passes" {
  run "$SD" --from-fixture "$DIR/clean.json" --check
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"ProductShape (Product): 0 field(s) off the shape"* ]] || return 1
  [[ "$output" == *"DomainShape (Domain): 0 field(s) off the shape"* ]] || return 1
}

@test "negative proof: a field on rows that the shape does not list is named with its row count" {
  plant 'd["rows"]["Domain"]["https://jeffbridwell.com/chorus#repoTarget"] = 44'
  run "$SD" --from-fixture "$DIR/planted.json" --check
  [ "$status" -eq 2 ] || return 1
  [[ "$output" == *"DomainShape (Domain): 1 field(s) off the shape"* ]] || return 1
  [[ "$output" == *"   44  chorus:repoTarget"* ]] || return 1
}

@test "negative proof: an ignored field stops counting only while it is ignored" {
  plant 'd["shapes"]["ProductShape"]["ignored"] = []'
  run "$SD" ProductShape --from-fixture "$DIR/planted.json" --check
  [ "$status" -eq 2 ] || return 1
  [[ "$output" == *"rdf:type"* ]] || return 1
}

@test "--json carries the same drift, and an unknown shape is refused" {
  plant 'd["rows"]["Product"]["https://jeffbridwell.com/chorus#purpose"] = 2'
  run "$SD" ProductShape --from-fixture "$DIR/planted.json" --json
  [ "$status" -eq 0 ] || return 1
  printf '%s' "$output" | python3 -c '
import json, sys
assert json.load(sys.stdin) == {"ProductShape": [{"field": "chorus:purpose", "rows": 2}]}' || return 1
  run "$SD" NoSuchShape --from-fixture "$DIR/clean.json"
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"no such shape"* ]] || return 1
}
