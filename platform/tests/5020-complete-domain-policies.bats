#!/usr/bin/env bats
# @test-type: contract — the plan, its shape and the land's dry run are hermetic; one case runs the referential-integrity row over a fixture dataset.
# @domain: policies
# @card: #5020 · owner: kade
# What Jeff sees: "complete domain" is policy rows, not prose: one store, one generated API
# door, no side-door readers or writers, an owner, tests, referential integrity, and its rows
# conforming to their shapes. Each applies to every domain and runs as `--domain <name>`.
#
# AC2: land-5020-rows adds the seven rows (no change, no delete), each conforming to the
# PolicyShape (with its new appliesTo), each a check naming {domain} with a measured baseline
# and a fixture in the tree. Referential integrity runs on its own: over a dataset with no
# other policy, no #4049 rows and no #5019 rows, it counts a dangling link and nothing else.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
PLAN="$ROOT/designing/domains/policies/policy-checks-5020.json"
LAND="$ROOT/platform/scripts/land-5020-rows"
PV="$ROOT/platform/scripts/policy-validate"
SHAPE="$ROOT/roles/wren/ontology/policies-4077.ttl"
RULES="domain-has-owner domain-has-tests domain-no-side-door domain-one-door domain-one-store domain-referential-integrity domain-shacl-conforms"

setup() { T="$BATS_TEST_TMPDIR"; }

@test "AC2: the plan is the seven complete-domain rules, each a {domain} check with a baseline and a fixture on disk" {
  run python3 - "$PLAN" "$ROOT" "$RULES" <<'PY'
import json, os, sys
rows = json.load(open(sys.argv[1]))
assert sorted(r["name"] for r in rows) == sys.argv[3].split(), sorted(r["name"] for r in rows)
for r in rows:
    assert "{domain}" in r["checkSpec"], r["name"]
    assert "appliesTo" not in r, r["name"]  # every domain
    assert r["checkKind"] in ("sparql", "command") and r["status"] == "ratchet", r["name"]
    assert r["baseline"].isdigit(), r["name"]
    assert os.path.isfile(os.path.join(sys.argv[2], r["violationFixture"])), r["violationFixture"]
print("ok")
PY
  [ "$status" -eq 0 ] && [ "$output" = ok ] || { echo "$output"; return 1; }
}

@test "AC2: every planned row conforms to the PolicyShape" {
  run python3 "$LAND" --ttl "$T/rows.ttl"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  printf '@prefix c: <https://jeffbridwell.com/chorus#> .\nc:principal-silas a c:Principal .\n' >> "$T/rows.ttl"
  run shacl validate --shapes "$SHAPE" --data "$T/rows.ttl"
  echo "$output" | grep -q "sh:conforms *true" || { echo "$output" | grep resultMessage | head; return 1; }
}

@test "AC2: the PolicyShape takes appliesTo as Domain links, several allowed" {
  printf '@prefix c: <https://jeffbridwell.com/chorus#> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\nc:principal-kade a c:Principal .\nc:x a c:Domain . c:y a c:Domain .\nc:policy-two a c:Policy ; rdfs:label "r" ; rdfs:comment "c" ; c:ownedBy c:principal-kade ; c:appliesTo c:x , c:y .\n' > "$T/ok.ttl"
  run shacl validate --shapes "$SHAPE" --data "$T/ok.ttl"
  echo "$output" | grep -q "sh:conforms *true" || { echo "$output"; return 1; }
}

@test "AC2 NEGATIVE: appliesTo naming something that is not a Domain breaks the shape" {
  printf '@prefix c: <https://jeffbridwell.com/chorus#> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\nc:principal-kade a c:Principal .\nc:policy-bad a c:Policy ; rdfs:label "r" ; rdfs:comment "c" ; c:ownedBy c:principal-kade ; c:appliesTo c:principal-kade .\n' > "$T/bad.ttl"
  run shacl validate --shapes "$SHAPE" --data "$T/bad.ttl"
  echo "$output" | grep -q "sh:conforms *false" || { echo "$output"; return 1; }
}

@test "AC2: the land's dry run adds the seven rows, changes and deletes nothing, writes nothing" {
  printf '{"policies": [{"name": "unrelated", "comment": "c"}]}\n' > "$T/live.json"
  printf '#!/bin/sh\necho "$*" >> %s/runner.log\n' "$T" > "$T/runner"; chmod +x "$T/runner"
  LAND_LIVE_JSON="$T/live.json" CHORUS_RUNNER="$T/runner" run python3 "$LAND"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q 'door writes 7 add, 0 change, 0 delete' || { echo "$output"; return 1; }
  [ ! -e "$T/runner.log" ]
}

@test "AC2: referential integrity runs on its own: one dangling link in x, none in y" {
  python3 - "$PLAN" "$T/p.json" <<'PY'
import json, sys
r = [r for r in json.load(open(sys.argv[1])) if r["name"] == "domain-referential-integrity"]
json.dump({"data": r}, open(sys.argv[2], "w"))
PY
  export POLICIES_JSON="$T/p.json" POLICY_DATA="$ROOT/platform/tests/fixtures/5020/domain-referential-integrity.trig"
  run "$PV" --domain x
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q '^FAIL domain-referential-integrity 1 > 0' || { echo "$output"; return 1; }
  echo "$output" | grep -q 'chorus#gone' || return 1
  ! echo "$output" | grep -q 'chorus#here' || return 1
  run "$PV" --domain y
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}
