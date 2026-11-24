#!/usr/bin/env bats
# @test-type: contract — runs policy-validate over fixture policies; no store, no door, no network.
# @domain: policies
# @card: #5033 · owner: wren
# What Jeff sees: `policy-validate --domain X --own` checks only what is about X. The 44
# whole-system checks a one-domain run used to repeat are left out, and the run says how many.
#
# AC1: --own keeps a check whose spec names {domain} (held at 0) or whose appliesTo names X
# (held at its baseline). NEGATIVE: a failing whole-system row does not fail it; a failing
# per-domain row does; a domain with no check of its own is exit 2, never a pass.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  PV="$REPO/platform/scripts/policy-validate"
  T="$BATS_TEST_TMPDIR"
  export POLICY_DOMAINS="x y z" TEXTFILE_DIR="$T/prom"
}

# rows: name|appliesTo (comma list, "" = every domain)|baseline|command
policies() {
  python3 - "$T/policies.json" "$@" <<'PY'
import json, sys
rows = []
for spec in sys.argv[2:]:
    name, applies, base, cmd = spec.split("|", 3)
    r = {"name": name, "label": f"rule of {name}", "comment": "c", "gate": "data", "status": "ratchet",
         "baseline": base, "checkKind": "command", "checkSpec": cmd}
    if applies:
        r["appliesTo"] = [f"chorus:{a}" for a in applies.split(",")]
    rows.append(r)
json.dump({"data": rows}, open(sys.argv[1], "w"))
PY
  # an own lint reads the whole rule set from POLICY_OWN_ROWS, never POLICIES_JSON
  export POLICIES_JSON="$T/policies.json" POLICY_OWN_ROWS="$T/policies.json"
}

ONLY_X='[ "{domain}" = x ] && echo "x row broken" || true'

@test "AC1: --own runs the per-domain and the attached rows, leaves the whole-system ones out, and says how many" {
  policies "system-a||0|true" "system-b||0|true" "per-domain||5|: {domain}" "for-x|x|0|true" "for-y|y|0|true"
  run "$PV" --domain x --own
  test "$status" -eq 0 || { echo "$output"; return 1; }
  echo "$output" | grep -q '^PASS per-domain 0 <= 0' || { echo "$output"; return 1; }
  echo "$output" | grep -q '^PASS for-x ' || { echo "$output"; return 1; }
  if echo "$output" | grep -q 'system-a\|system-b\|for-y'; then echo "$output"; return 1; fi
  echo "$output" | grep -q '2 checks, 0 above baseline, 0 could not run (domain x, own checks only: 2 whole-system left out)' || { echo "$output"; return 1; }
}

@test "AC1 NEGATIVE: a failing whole-system row fails the full lint and does not fail --own" {
  policies "system-red||0|echo broken" "per-domain||0|: {domain}"
  run "$PV" --domain x
  test "$status" -eq 1 || { echo "$output"; return 1; }
  run "$PV" --domain x --own
  test "$status" -eq 0 || { echo "$output"; return 1; }
}

@test "AC1 NEGATIVE: a failing per-domain row fails --own for its domain only, naming rule and violation" {
  policies "system-a||0|true" "per-domain||9|$ONLY_X"
  run "$PV" --domain x --own
  test "$status" -eq 1 || { echo "$output"; return 1; }
  echo "$output" | grep -q '^FAIL per-domain 1 > 0' || { echo "$output"; return 1; }
  echo "$output" | grep -q '  - x: x row broken' || { echo "$output"; return 1; }
  run "$PV" --domain y --own
  test "$status" -eq 0 || { echo "$output"; return 1; }
}

@test "AC1: an attached row keeps its own baseline under --own" {
  policies "for-x|x|1|echo one" "per-domain||0|: {domain}"
  run "$PV" --domain x --own
  test "$status" -eq 0 || { echo "$output"; return 1; }
  echo "$output" | grep -q '^PASS for-x 1 <= 1' || { echo "$output"; return 1; }
}

@test "AC1 NEGATIVE: a domain with no check of its own is exit 2, and --own without --domain is refused" {
  policies "system-a||0|true" "for-y|y|0|true"
  run "$PV" --domain x --own
  test "$status" -eq 2 || { echo "$output"; return 1; }
  echo "$output" | grep -q 'an empty lint is not a pass' || { echo "$output"; return 1; }
  run "$PV" --own
  test "$status" -eq 2 || { echo "$output"; return 1; }
}

@test "AC1: --violations prints one line per violation and nothing else, with the same exit codes" {
  policies "system-a||0|true" "per-domain||0|$ONLY_X"
  run "$PV" --domain x --own --violations
  test "$status" -eq 1 || { echo "$output"; return 1; }
  test "$output" = "per-domain: x: x row broken" || { echo "$output"; return 1; }
  run "$PV" --domain y --own --violations
  test "$status" -eq 0 || { echo "$output"; return 1; }
  test -z "$output" || { echo "$output"; return 1; }
}

@test "AC1 NEGATIVE (Kade): handed one gate's rows in POLICIES_JSON, the own lint still reads every rule" {
  policies "per-domain||0|$ONLY_X"
  # what the counts page does: POLICIES_JSON holds another gate's rows only
  echo '{"data": []}' > "$T/one-gate.json"
  POLICIES_JSON="$T/one-gate.json" run "$PV" --domain x --own
  test "$status" -eq 1 || { echo "$output"; return 1; }
  echo "$output" | grep -q '^FAIL per-domain 1 > 0' || { echo "$output"; return 1; }
}

@test "AC1 (Kade a): a per-domain check that only mentions policy-validate still runs; one that runs an own lint is skipped" {
  policies "mentions||0|echo see policy-validate for {domain}" "nested||0|echo policy-validate --domain {domain} --own"
  run "$PV" --domain x --own
  test "$status" -eq 1 || { echo "$output"; return 1; }
  echo "$output" | grep -q '^FAIL mentions 1 > 0' || { echo "$output"; return 1; }
  if echo "$output" | grep -q 'nested'; then echo "$output"; return 1; fi
}

@test "AC1: --prom on an own lint writes its own file, not the full one-domain lint's" {
  policies "per-domain||0|: {domain}"
  run "$PV" --domain y --own --prom
  test "$status" -eq 0 || { echo "$output"; return 1; }
  test -s "$T/prom/chorus_policy_domain_y_own.prom" || { ls "$T/prom"; return 1; }
  test ! -e "$T/prom/chorus_policy_domain_y.prom" || { ls "$T/prom"; return 1; }
}

@test "AC1 NEGATIVE: under --violations a check that cannot run is exit 2, never an empty pass" {
  policies "per-domain||0|exit 7"
  run "$PV" --domain x --own --violations
  test "$status" -eq 2 || { echo "$output"; return 1; }
}
