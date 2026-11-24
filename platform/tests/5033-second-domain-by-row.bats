#!/usr/bin/env bats
# @test-type: contract — the shipped policy row with a second domain attached, over fixture policies; no store, no door, no network.
# @domain: policies
# @card: #5033 · owner: wren
# What Jeff sees: a second complete domain is added by editing the row's appliesTo. No script
# changes, and no script names a domain for this rule.
#
# AC3: with two domains named on the row, the second one's violation is red. NEGATIVE: a
# policy-validate that carries its own list of complete domains is caught.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  PV="$REPO/platform/scripts/policy-validate"
  PLAN="$REPO/designing/domains/policies/policy-checks-5033.json"
  T="$BATS_TEST_TMPDIR"
  export POLICY_DOMAINS="x y z" TEXTFILE_DIR="$T/prom" CHORUS_HOME="$REPO"
  cd "$REPO"
}

policies() {
  python3 - "$PLAN" "$T/policies.json" "$1" "$2" <<'PY'
import json, sys
row = dict(json.load(open(sys.argv[1]))[0])
row["appliesTo"] = [f"chorus:{d}" for d in sys.argv[3].split(",") if d]
per = {"name": "per-domain", "label": "rule of per-domain", "comment": "c", "gate": "data", "status": "ratchet",
       "baseline": "99", "checkKind": "command", "checkSpec": sys.argv[4]}
json.dump({"data": [row, per]}, open(sys.argv[2], "w"))
PY
  export POLICIES_JSON="$T/policies.json" POLICY_OWN_ROWS="$T/policies.json"
}

ONLY_Y='[ "{domain}" = y ] && echo "y row broken" || true'

# prints every line of $1 that holds one of the quoted Domain names $2.. (a hard-coded list
# for this rule). Limit, named on the card: only the names passed are looked for.
names_a_domain() {
  f="$1"; shift
  for d in "$@"; do grep -nE "[\"']${d}[\"']" "$f" || true; done
}

@test "AC3: with only x on the row, y's violation is not this rule's; add y to the row and it is red" {
  policies x "$ONLY_Y"
  run "$PV"
  test "$status" -eq 0 || { echo "$output"; return 1; }
  policies x,y "$ONLY_Y"
  run "$PV"
  test "$status" -eq 1 || { echo "$output"; return 1; }
  echo "$output" | grep -q '^FAIL complete-domain-stays-complete 1 > 0' || { echo "$output"; return 1; }
  run "$PV" --domain y
  echo "$output" | grep -q '  - y: per-domain: y: y row broken' || { echo "$output"; return 1; }
}

@test "AC3: the two runs above used the same policy-validate; no script names a domain for this rule" {
  found="$(names_a_domain "$PV" principles policies cards roles)"
  test -z "$found" || { echo "policy-validate names a domain: $found"; return 1; }
  # the rule's name appears in no script but the one that writes its row
  hits="$(grep -rl 'complete-domain-stays-complete' "$REPO/platform/scripts" | sed "s|$REPO/||" | sort | tr '\n' ' ')"
  test "$hits" = "platform/scripts/policy-rows-5033 " || { echo "named in: $hits"; return 1; }
}

@test "AC3 NEGATIVE: a policy-validate that carries its own list of complete domains is caught" {
  cp "$PV" "$T/pv"
  printf '\nCOMPLETE = ["principles", "policies"]\n' >> "$T/pv"
  found="$(names_a_domain "$T/pv" principles policies cards roles)"
  test -n "$found" || { echo "a hard-coded list passed"; return 1; }
}
