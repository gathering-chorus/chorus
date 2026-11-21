#!/usr/bin/env bats
# @test-type: contract — the undeclared-property rule over a fixture dataset, and the generator's templates; no store.
# @domain: policies
# @card: #5021 · owner: kade
# What Jeff sees: a row shows the fields its shape names, and no others: no empty "status" on
# a principle, and the lint names any stored property a shape does not declare.
#
# AC8: domain-properties-declared counts each (class, property) a domain's rows carry that the
# class's shape does not declare (the four Dublin Core stamps are CoreShape's rule, #4312). NEGATIVE:
# x's stray status is red (platform/tests/fixtures/5021/domain-properties-declared.trig), y is
# green. athena-gen serves and accepts status only where a shape declares it; the generated
# crates' own tests (a_row_serves_status_only_when_its_shape_declares_one,
# a_write_of_status_to_a_class_without_one_is_refused_off_model) prove it on rows.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
# Shared setup (status_tests_pass), named so a change to it runs this suite: platform/tests/5021-helpers.bash
load 5021-helpers
PV="$ROOT/platform/scripts/policy-validate"

setup() {
  T="$BATS_TEST_TMPDIR"
  python3 - "$ROOT" "$T/p.json" <<'PY'
import json, sys
r = [x for x in json.load(open(sys.argv[1] + "/designing/domains/policies/policy-checks-5021.json")) if x["name"] == "domain-properties-declared"]
json.dump({"data": r}, open(sys.argv[2], "w"))
PY
  export POLICIES_JSON="$T/p.json" POLICY_DATA="$ROOT/platform/tests/fixtures/5021/domain-properties-declared.trig"
}

@test "AC8 NEGATIVE: x's row with a property its shape does not declare is red; y is green" {
  run "$PV" --domain x
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q '^FAIL domain-properties-declared 1 > 0' || { echo "$output"; return 1; }
  echo "$output" | grep -q 'chorus#Thing https://jeffbridwell.com/chorus#status' || { echo "$output"; return 1; }
  run "$PV" --domain y
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "AC8: the Dublin Core stamps are CoreShape's rule, not undeclared properties" {
  run "$PV" --domain x
  ! echo "$output" | grep -q 'dc/terms' || { echo "$output"; return 1; }
}

@test "AC8: athena-gen serves and accepts status only where a shape declares it" {
  local t="$ROOT/designing/services/athena-gen/templates"
  ! grep -q 'row.insert("status"' "$t/store.rs" || return 1
  ! grep -q 'k == "status"' "$t/rules.rs" || return 1
  ! grep -q 'f == "status"' "$t/main.rs" || return 1
  for crate in shaping/domains/principles/api directing/products/pulse/api; do
    status_tests_pass "$crate" || return 1
  done
}

# the published contract says what is served: a class whose shape has no status carries none in
# its OpenAPI read schema; one that declares it (pulse's cards row) keeps it
@test "AC8: the generated OpenAPI read schema lists status only where the shape declares it" {
  python3 - "$ROOT" <<'PY' || return 1
import json, sys
r = sys.argv[1]
def props(f):
    d = json.load(open(r + f))
    return {n: s.get("properties", {}) for n, s in d["components"]["schemas"].items()}
p = props("/shaping/domains/principles/api/surfaces/Principle.openapi.json")
assert not any("status" in v for v in p.values()), "Principle's contract still lists status"
PY
}
