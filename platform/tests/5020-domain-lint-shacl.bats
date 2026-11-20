#!/usr/bin/env bats
# @test-type: contract — Jena SHACL over a committed fixture dataset; no store, no network.
# @domain: policies
# @card: #5020 · owner: kade
# What Jeff sees: a domain whose rows break their shapes is red in its lint, so the shape
# and the policy push the same domain to green.
#
# AC5: domain-shacl-conforms counts each SHACL result on the domain's own rows. NEGATIVE: a
# fixture row of x breaking its shape turns x red and leaves y green, both when one domain is
# linted and in a full run's single pass over every domain (Silas: X fails X, not Y).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
PLAN="$ROOT/designing/domains/policies/policy-checks-5020.json"
PV="$ROOT/platform/scripts/policy-validate"
COUNT="$ROOT/platform/scripts/policy-count"
FX="$ROOT/platform/tests/fixtures/5020"
# Named here so a change to it runs this suite (werk-test ties a file to the tests that name it):
# platform/tests/fixtures/5020/domain-shacl-conforms.sh

setup() {
  T="$BATS_TEST_TMPDIR"
  export CHORUS_HOME="$ROOT"
  sh "$FX/domain-shacl-conforms.sh" "$T"
  export SHACL_DATA="$T/data.trig" SHACL_SHAPES="$T/shapes.ttl"
  python3 - "$PLAN" "$T/p.json" <<'PY'
import json, sys
r = [r for r in json.load(open(sys.argv[1])) if r["name"] == "domain-shacl-conforms"]
json.dump({"data": r}, open(sys.argv[2], "w"))
PY
  export POLICIES_JSON="$T/p.json"
}

@test "AC5 NEGATIVE: x's row breaking its shape turns x red, naming the row; y stays green" {
  run "$PV" --domain x
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q '^FAIL domain-shacl-conforms 1 > 0' || { echo "$output"; return 1; }
  echo "$output" | grep -q 'chorus#x-one' || { echo "$output"; return 1; }
  run "$PV" --domain y
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "AC5: the full run's single pass gives each domain its own results" {
  mkdir "$T/run"
  POLICY_RUN_DIR="$T/run" POLICY_RUN_ALL_DOMAINS=1 run "$COUNT" domain-shacl-conforms x
  [ "$status" -eq 0 ] && [ "$(printf '%s\n' "$output" | grep -c .)" -eq 1 ] || { echo "$output"; return 1; }
  [ -s "$T/run/domain-shacl.json" ]
  POLICY_RUN_DIR="$T/run" POLICY_RUN_ALL_DOMAINS=1 run "$COUNT" domain-shacl-conforms y
  [ "$status" -eq 0 ] && [ -z "$output" ] || { echo "$output"; return 1; }
  # the full run, the sum held at a 0 baseline: x's one result is the whole sum
  python3 -c "import json; d = json.load(open('$T/p.json')); d['data'][0]['baseline'] = '0'; json.dump(d, open('$T/p.json', 'w'))"
  POLICY_DOMAINS="x y" run "$PV"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q '^FAIL domain-shacl-conforms 1 > 0' || { echo "$output"; return 1; }
}

@test "AC5: control: the row given its label conforms, and x is green" {
  perl -pi -e 's/c:x-one a c:Thing \./c:x-one a c:Thing ; rdfs:label "x one" ./' "$T/data.trig"
  run "$PV" --domain x
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "AC5 NEGATIVE: x's Domain row breaking its shape turns x red (and the domains domain, which holds Domain rows); y stays green" {
  perl -pi -e 's/c:x-one a c:Thing \./c:x-one a c:Thing ; rdfs:label "x one" ./; s/c:x a c:Domain ; rdfs:label "x" \./c:x a c:Domain ./' "$T/data.trig"
  run "$PV" --domain x
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q '^FAIL domain-shacl-conforms 1 > 0' || { echo "$output"; return 1; }
  echo "$output" | grep -q 'chorus#x>' || { echo "$output"; return 1; }
  run "$PV" --domain domains
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  run "$PV" --domain y
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "AC5 NEGATIVE: a row outside any named graph is exit 2, never filed under a guessed domain" {
  printf '<https://jeffbridwell.com/chorus#x-two> <https://jeffbridwell.com/chorus#p> <urn:chorus:domains:y> .\n' >> "$T/data.trig"
  run "$PV" --domain y
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
}

@test "AC5 NEGATIVE: with riot and shacl missing the check is exit 2, never a pass" {
  mkdir "$T/bin" && ln -s "$(command -v python3)" "$T/bin/python3"
  PATH="$T/bin:/bin" run "$COUNT" domain-shacl-conforms x
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
}

@test "AC5 NEGATIVE: data SHACL cannot read is exit 2, never green" {
  echo 'not turtle {' > "$T/data.trig"
  run "$PV" --domain x
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
}
