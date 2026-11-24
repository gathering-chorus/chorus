#!/usr/bin/env bats
# @test-type: integration — the read-the-code-first policy row's plan and its land script, against a stubbed door; no live services
# @domain: policies
#
# #5027 AC1 (Jeff 11-21 17:37: "this is a policy"): a row read-the-code-first in the policies
# domain states the rule. The row ships as designing/domains/policies/policy-checks-5027.json and
# werk-deploy applies it with platform/scripts/land-5027-rows after the land (#4889).
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
PLAN="$ROOT/designing/domains/policies/policy-checks-5027.json"

# the plan states the rule: the row, in the code gate, saying whole file, Read tool, shell read, callers
states_rule() {
  python3 - "$1" <<'PY'
import json, sys
rows = {r["name"]: r for r in json.load(open(sys.argv[1]))}
r = rows.get("read-the-code-first")
assert r, "no read-the-code-first row"
assert r.get("gate") == "code", r.get("gate")
c = r.get("comment", "")
for words in ("whole file", "Read tool", "shell read", "callers"):
    assert words in c, f"rule text misses '{words}'"
PY
}

@test "#5027 the plan holds the row and it states the rule" {
  states_rule "$PLAN"
}

@test "#5027 land-5027-rows plans the row as an ADD against a door without it, and SAME once it is there" {
  live="$BATS_TEST_TMPDIR/live.json"
  echo '{"policies": []}' > "$live"
  run env LAND_LIVE_JSON="$live" python3 "$ROOT/platform/scripts/land-5027-rows"
  test "$status" -eq 0 || { echo "$output"; return 1; }
  echo "$output" | grep -q "^ADD    policies/read-the-code-first"
  python3 - "$PLAN" "$live" <<'PY'
import json, sys
r = json.load(open(sys.argv[1]))[0]
json.dump({"policies": [dict(r)]}, open(sys.argv[2], "w"))
PY
  run env LAND_LIVE_JSON="$live" python3 "$ROOT/platform/scripts/land-5027-rows"
  echo "$output" | grep -q "^SAME   policies/read-the-code-first"
}

@test "#5027 NEGATIVE: a plan without the row, or with a rule that never says whole file, is caught" {
  bad="$BATS_TEST_TMPDIR/bad.json"
  echo '[{"name": "something-else", "gate": "code", "comment": "x"}]' > "$bad"
  if states_rule "$bad" 2>/dev/null; then echo "a plan without the row passed"; return 1; fi
  echo '[{"name": "read-the-code-first", "gate": "code", "comment": "read some of the file with the Read tool; a shell read is fine; callers"}]' > "$bad"
  if states_rule "$bad" 2>/dev/null; then echo "a rule without 'whole file' passed"; return 1; fi
}
