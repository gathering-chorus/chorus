#!/usr/bin/env bats
# @test-type: contract — runs each policy's own check over committed fixtures; no store, no network, no board.
# @domain: policies
# @card: #4964
# @owner: silas
#
# #4964 AC2 — negative proof (#3734): for every one of the 15 rules, its fixture (one real
# violation; the baseline forced to 0, so the fixture is baseline + 1) makes that rule's check
# count it and policy-validate exit 1; the same violation at baseline 1 passes; and the check
# is clean (exit 0) on a tree or data holding no violation. A check that cannot measure is
# exit 2, never a pass.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
CHECKS="$ROOT/designing/domains/policies/policy-checks-4964.json"
PV="$ROOT/platform/scripts/policy-validate"
COUNT="$ROOT/platform/scripts/policy-count"

setup() {
  T="$BATS_TEST_TMPDIR"
  export CHORUS_HOME="$ROOT"
  printf '@prefix c: <https://jeffbridwell.com/chorus#> .\n' > "$T/empty.trig"
  # an empty board and an empty policy list, for the clean edge of the two rules that read them
  printf '#!/bin/bash\n[ "$1" = list ] && printf "WIP (0):\\n\\nNext (0):\\n"\n' > "$T/empty-cards"; chmod +x "$T/empty-cards"
  printf '{"data": []}\n' > "$T/no-rows.json"
}

one_policy() {
  python3 - "$CHECKS" "$1" "$T/p.json" <<'PY'
import json, sys
rows = [r for r in json.load(open(sys.argv[1])) if r["name"] == sys.argv[2]]
assert rows, sys.argv[2]
r = dict(rows[0]); r["baseline"] = "0"
json.dump({"data": [r]}, open(sys.argv[3], "w"))
PY
  export POLICIES_JSON="$T/p.json"
}

field() { python3 -c "import json; print([r['$2'] for r in json.load(open('$CHECKS')) if r['name']=='$1'][0])"; }

# Named here so a change to any of them runs this suite (werk-test ties a file to the tests that name it).
FIXTURES="platform/tests/fixtures/4964/code-object-authz-and-secret-mask.sh
platform/tests/fixtures/4964/code-test-side-effect-isolation.sh
platform/tests/fixtures/4964/data-domain-is-punned-owl-class.trig
platform/tests/fixtures/4964/data-no-hardcoded-graph-mappings.sh
platform/tests/fixtures/4964/data-property-key-registered.trig
platform/tests/fixtures/4964/data-single-graph-multi-lens.sh
platform/tests/fixtures/4964/prod-blast-radius-before-pull.sh
platform/tests/fixtures/4964/prod-interactive-skill-5s-budget.sh
platform/tests/fixtures/4964/prod-private-by-default-graduation.trig
platform/tests/fixtures/4964/prod-service-design-html-first.sh
platform/tests/fixtures/4964/qual-domain-fitness-functions.trig
platform/tests/fixtures/4964/qual-golden-api-regression.sh
platform/tests/fixtures/4964/qual-negative-proof-for-gates.sh
platform/tests/fixtures/4964/qual-no-silent-pipeline-drops.sh
platform/tests/fixtures/4964/qual-warning-ratchet-downward.sh"

@test "the plan's fixtures are exactly the 15 named here, one per rule" {
  run python3 -c "import json; print('\\n'.join(sorted(r['violationFixture'] for r in json.load(open('$CHECKS')))))"
  [ "$status" -eq 0 ] && [ "$output" = "$FIXTURES" ] || { echo "$output"; return 1; }
}

@test "negative proof: every command check counts its fixture's violation (exit 1), passes at baseline 1, and is clean in an empty tree" {
  for name in $(python3 -c "import json; print(' '.join(r['name'] for r in json.load(open('$CHECKS')) if r['checkKind']=='command'))"); do
    one_policy "$name"
    red="$T/red-$name"; clean="$T/clean-$name"; mkdir -p "$red" "$clean"
    sh "$ROOT/$(field "$name" violationFixture)" "$red" >/dev/null
    (cd "$clean" && git init -q .)
    redenv="CARDS='$red/platform/scripts/cards' POLICY_COUNT_ROWS='$red/zz-policies.json'"
    cleanenv="CARDS='$T/empty-cards' POLICY_COUNT_ROWS='$T/no-rows.json'"
    run sh -c "cd '$red' && $redenv '$PV'"
    [ "$status" -eq 1 ] || { echo "$name did not go red: $output"; return 1; }
    [[ "$output" == *"FAIL $name 1 > 0"* ]] || { echo "$name: $output"; return 1; }
    sed -i '' 's/"baseline": "0"/"baseline": "1"/' "$T/p.json"
    run sh -c "cd '$red' && $redenv '$PV'"
    [ "$status" -eq 0 ] || { echo "$name failed at its own baseline: $output"; return 1; }
    run sh -c "cd '$clean' && $cleanenv '$PV'"
    [ "$status" -eq 0 ] || { echo "$name is not clean: $output"; return 1; }
  done
}

@test "negative proof: every SPARQL check counts its fixture's one violation and is clean on empty data" {
  for name in $(python3 -c "import json; print(' '.join(r['name'] for r in json.load(open('$CHECKS')) if r['checkKind']=='sparql'))"); do
    one_policy "$name"
    POLICY_DATA="$ROOT/$(field "$name" violationFixture)" run "$PV"
    [ "$status" -eq 1 ] || { echo "$name did not go red: $output"; return 1; }
    [[ "$output" == *"FAIL $name 1 > 0"* ]] || { echo "$name: $output"; return 1; }
    POLICY_DATA="$T/empty.trig" run "$PV"
    [ "$status" -eq 0 ] || { echo "$name is not clean: $output"; return 1; }
  done
}

@test "negative proof: what a rule allows is not counted (a stubbed osascript, a comparison, a sanctioned db, a WIP card with a blast-radius read)" {
  mkdir -p "$T/ok" && cd "$T/ok" && git init -q . && mkdir -p platform/tests platform/api/src
  printf 'osascript() { :; }\n@test "zz" {\n  osascript -e "beep"\n}\n' > platform/tests/zz.bats
  printf 'if (owner === "wren") { route(); }\n' > platform/api/src/zz-route.ts
  printf 'const db = open("messages.db"); const ix = open("index.db");\n' > platform/api/src/zz-db.ts
  git add -A
  for r in code-test-side-effect-isolation data-no-hardcoded-graph-mappings data-single-graph-multi-lens; do
    run "$COUNT" "$r"
    [ "$status" -eq 0 ] && [ -z "$output" ] || { echo "$r: $output"; return 1; }
  done
  printf '#!/bin/bash\ncase "$1" in list) printf "WIP (1):\\n  9998  zz [Silas|domain:a|domain:b]\\n" ;; view) echo "Blast radius: read" ;; esac\n' > "$T/read-cards"; chmod +x "$T/read-cards"
  CARDS="$T/read-cards" run "$COUNT" prod-blast-radius-before-pull
  [ "$status" -eq 0 ] && [ -z "$output" ] || { echo "$output"; return 1; }
}

@test "negative proof: a check that cannot measure is exit 2 (board down, door down), never a pass" {
  printf '#!/bin/bash\necho "ERROR: VIKUNJA_TOKEN not set" >&2; exit 1\n' > "$T/dead-cards"; chmod +x "$T/dead-cards"
  CARDS="$T/dead-cards" run "$COUNT" prod-blast-radius-before-pull
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
  POLICY_DOOR=http://127.0.0.1:9 run "$COUNT" qual-negative-proof-for-gates
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
  one_policy prod-blast-radius-before-pull
  run sh -c "CARDS='$T/dead-cards' '$PV'"
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
}

@test "negative proof: an unknown rule name is exit 2, not an empty (passing) count" {
  run "$COUNT" qual-no-such-rule
  [ "$status" -eq 2 ] || return 1
}
