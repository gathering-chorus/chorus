#!/usr/bin/env bats
# @test-type: contract — runs each complete-domain row's own check over its committed fixture; no store, no network.
# @domain: policies
# @card: 5020 · owner: kade
# What Jeff sees: a domain that breaks a complete-domain rule comes up red in its lint, named
# by the rule, and a domain that keeps it stays green.
#
# AC3 NEGATIVE PROOF (#3734): a fixture domain x with a side-door writer is red, one with no
# owner is red, and one with a dangling link is red; in each fixture y keeps the rule and
# its lint passes. The other rows' fixtures are run the same way, so every row is shown red.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
PLAN="$ROOT/designing/domains/policies/policy-checks-5020.json"
PV="$ROOT/platform/scripts/policy-validate"
FX="$ROOT/platform/tests/fixtures/5020"

setup() {
  T="$BATS_TEST_TMPDIR"
  export CHORUS_HOME="$ROOT"
}

one_policy() {
  python3 - "$PLAN" "$1" "$T/p.json" <<'PY'
import json, sys
rows = [r for r in json.load(open(sys.argv[1])) if r["name"] == sys.argv[2]]
assert rows, sys.argv[2]
json.dump({"data": rows}, open(sys.argv[3], "w"))
PY
  export POLICIES_JSON="$T/p.json"
}

# x is red on the rule, y is green; prints the lint for x
red_for_x_only() {
  run "$PV" --domain x
  [ "$status" -eq 1 ] || { echo "x: $output"; return 1; }
  echo "$output" | grep -q "^FAIL $1 1 > 0" || { echo "x: $output"; return 1; }
  run "$PV" --domain y
  [ "$status" -eq 0 ] || { echo "y: $output"; return 1; }
}

@test "AC3 NEGATIVE: a side-door writer in x's store is red; x's own door, a test, the model and a lookalike name are not" {
  one_policy domain-no-side-door
  mkdir "$T/tree" && sh "$FX/domain-no-side-door.sh" "$T/tree"
  cd "$T/tree"
  red_for_x_only domain-no-side-door
  run "$PV" --domain x
  echo "$output" | grep -q 'platform/scripts/writer:1' || { echo "$output"; return 1; }
}

@test "AC3 NEGATIVE: a domain with no owner is red, and so is one with no Domain row" {
  one_policy domain-has-owner
  export POLICY_DATA="$FX/domain-has-owner.trig"
  red_for_x_only domain-has-owner
  run "$PV" --domain x
  echo "$output" | grep -q 'no owner' || { echo "$output"; return 1; }
  run "$PV" --domain nobody
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q 'no Domain row' || { echo "$output"; return 1; }
}

@test "AC3 NEGATIVE: a dangling link from x's rows is red" {
  one_policy domain-referential-integrity
  export POLICY_DATA="$FX/domain-referential-integrity.trig"
  red_for_x_only domain-referential-integrity
}

@test "a domain naming two stores is red" {
  one_policy domain-one-store
  export POLICY_DATA="$FX/domain-one-store.trig"
  red_for_x_only domain-one-store
}

@test "a domain with no generated API door is red" {
  one_policy domain-one-door
  export POLICY_DATA="$FX/domain-one-door.trig"
  red_for_x_only domain-one-door
}

@test "a domain no test names is red; a lookalike domain's test does not count" {
  one_policy domain-has-tests
  mkdir "$T/tree" && sh "$FX/domain-has-tests.sh" "$T/tree"
  cd "$T/tree"
  red_for_x_only domain-has-tests
}

@test "NEGATIVE: a lint whose check cannot run is exit 2, never green" {
  one_policy domain-has-owner
  export POLICY_DATA="$T/missing.trig"
  run "$PV" --domain x
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
}
