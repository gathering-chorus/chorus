#!/usr/bin/env bats
# @test-type: contract — runs the shipped policy row through policy-validate over fixture policies; no store, no door, no network.
# @domain: policies
# @card: #5033 · owner: wren
# What Jeff sees: a domain that reached zero stays there. One policy row,
# complete-domain-stays-complete, names the complete domains in its appliesTo; the quality
# gate and the hourly full run then run each named domain's own checks at zero.
#
# AC2: the row as shipped (designing/domains/policies/policy-checks-5033.json) runs the own
# lint of each domain it names. NEGATIVE: a violation in a named domain turns the full run
# red, naming the rule and the domain; the same violation in a domain it does not name
# leaves it green; an inner run that cannot measure is BROKEN (exit 2); the inner run never
# runs the row itself.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  PV="$REPO/platform/scripts/policy-validate"
  PLAN="$REPO/designing/domains/policies/policy-checks-5033.json"
  T="$BATS_TEST_TMPDIR"
  export POLICY_DOMAINS="x y z" TEXTFILE_DIR="$T/prom" CHORUS_HOME="$REPO"
  cd "$REPO"
}

# the shipped row attached to $1 (comma list), beside one per-domain check whose command is $2
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

ONLY_X='[ "{domain}" = x ] && echo "x row broken" || true'

@test "AC2: the shipped row is a runnable check at baseline 0 in the quality gate, attached to principles" {
  python3 - "$PLAN" <<'PY'
import json, sys
rows = json.load(open(sys.argv[1]))
assert len(rows) == 1, rows
r = rows[0]
assert r["name"] == "complete-domain-stays-complete", r["name"]
assert r["checkKind"] == "command" and str(r["baseline"]) == "0" and r["gate"] == "quality", r
assert "{domain}" in r["checkSpec"] and "--own" in r["checkSpec"] and "--violations" in r["checkSpec"], r["checkSpec"]
assert r["appliesTo"] == ["principles"], r["appliesTo"]
PY
}

@test "AC2: with every named domain clean, the full run and the quality gate pass the row" {
  policies x ': {domain}'
  run "$PV"
  test "$status" -eq 0 || { echo "$output"; return 1; }
  echo "$output" | grep -q '^PASS complete-domain-stays-complete 0 <= 0' || { echo "$output"; return 1; }
  run "$PV" --gate quality
  test "$status" -eq 0 || { echo "$output"; return 1; }
  echo "$output" | grep -q '^PASS complete-domain-stays-complete 0 <= 0' || { echo "$output"; return 1; }
}

@test "AC2 NEGATIVE: a violation in a named domain turns the full run red, naming the rule and the domain" {
  policies x "$ONLY_X"
  run "$PV"
  test "$status" -eq 1 || { echo "$output"; return 1; }
  echo "$output" | grep -q '^FAIL complete-domain-stays-complete 1 > 0 (quality)' || { echo "$output"; return 1; }
  # the summed per-domain row is still inside its own baseline: only the new row caught it
  echo "$output" | grep -q '^PASS per-domain 1 <= 99' || { echo "$output"; return 1; }
  # the full run's own output names the rule and the domain that broke it (code review, run 1)
  echo "$output" | grep -q '  rule: A complete domain stays complete' || { echo "$output"; return 1; }
  echo "$output" | grep -q '  - x: per-domain: x: x row broken' || { echo "$output"; return 1; }
}

@test "AC2 NEGATIVE: the same violation in a domain the row does not name leaves the row green" {
  policies y "$ONLY_X"
  run "$PV"
  test "$status" -eq 0 || { echo "$output"; return 1; }
  echo "$output" | grep -q '^PASS complete-domain-stays-complete 0 <= 0' || { echo "$output"; return 1; }
}

@test "AC2 NEGATIVE (Kade): the quality gate's run still counts a failing data-gate domain check" {
  # per-domain is a data-gate row; the outer run is --gate quality and never runs it itself
  policies x "$ONLY_X"
  run "$PV" --gate quality
  test "$status" -eq 1 || { echo "$output"; return 1; }
  echo "$output" | grep -q '^FAIL complete-domain-stays-complete 1 > 0 (quality)' || { echo "$output"; return 1; }
  if echo "$output" | grep -q '^PASS per-domain\|^FAIL per-domain'; then echo "$output"; return 1; fi
}

@test "AC2 NEGATIVE: an inner run that cannot measure is BROKEN, exit 2, never a pass" {
  policies x ": {domain}; exit 7"
  # the quality gate runs only the row (per-domain is a data-gate row), so exit 2 is the row's own
  run "$PV" --gate quality
  test "$status" -eq 2 || { echo "$output"; return 1; }
  echo "$output" | grep -q '^BROKEN complete-domain-stays-complete' || { echo "$output"; return 1; }
  # it is the inner check that broke, said in the row's reason, not an empty inner lint
  echo "$output" | grep -q 'BROKEN per-domain: exit 7' || { echo "$output"; return 1; }
  if echo "$output" | grep -q 'an empty lint'; then echo "$output"; return 1; fi
  echo "$output" | grep -q '0 checks, 0 above baseline, 1 could not run (gate quality)' || { echo "$output"; return 1; }
}

@test "After the land: the row script's dry run plans one ADD carrying appliesTo, then reads SAME once the door holds it" {
  ROWS="$REPO/platform/scripts/policy-rows-5033"
  echo '{"policies": []}' > "$T/live.json"
  LAND_LIVE_JSON="$T/live.json" run python3 "$ROWS"
  test "$status" -eq 0 || { echo "$output"; return 1; }
  echo "$output" | grep -q '^ADD    policies/complete-domain-stays-complete' || { echo "$output"; return 1; }
  echo "$output" | grep -q '"appliesTo": \["principles"\]' || { echo "$output"; return 1; }
  echo "$output" | grep -q 'door writes 1 add, 0 change, 0 delete; 0 same — nothing written (dry run)' || { echo "$output"; return 1; }
  # the door serves the link back as chorus:principles; the plan must read that as the same row
  python3 - "$PLAN" "$T/live.json" <<'PY'
import json, sys
r = dict(json.load(open(sys.argv[1]))[0]); r["appliesTo"] = ["chorus:principles"]
json.dump({"policies": [r]}, open(sys.argv[2], "w"))
PY
  LAND_LIVE_JSON="$T/live.json" run python3 "$ROWS"
  test "$status" -eq 0 || { echo "$output"; return 1; }
  echo "$output" | grep -q '^SAME   policies/complete-domain-stays-complete' || { echo "$output"; return 1; }
  # NEGATIVE: a row attached to a different domain is a CHANGE, never SAME
  python3 - "$PLAN" "$T/live.json" <<'PY'
import json, sys
r = dict(json.load(open(sys.argv[1]))[0]); r["appliesTo"] = ["chorus:policies"]
json.dump({"policies": [r]}, open(sys.argv[2], "w"))
PY
  LAND_LIVE_JSON="$T/live.json" run python3 "$ROWS"
  echo "$output" | grep -q '^CHANGE policies/complete-domain-stays-complete' || { echo "$output"; return 1; }
}

@test "AC2 NEGATIVE: the inner run never runs the row itself, attached or not" {
  policies x ': {domain}'
  run "$PV" --domain x --own
  test "$status" -eq 0 || { echo "$output"; return 1; }
  if echo "$output" | grep -q 'complete-domain-stays-complete'; then echo "$output"; return 1; fi
  # a full one-domain lint does run it, once
  run "$PV" --domain x
  test "$(echo "$output" | grep -c 'complete-domain-stays-complete')" -eq 1 || { echo "$output"; return 1; }
}
