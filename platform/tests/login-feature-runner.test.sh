#!/usr/bin/env bash
# @test-type: unit — runs login-feature.test.sh on fixture features and a fixture bats file; no services
# @domain: identity
#
# #4367 — the feature runner must be able to go red for each reason it exists for:
# a case that fails, a case that isn't there, a scenario with no case, a scenario
# waiting on a card — and a scenario is green only when all its cases are green.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUN="$ROOT/platform/tests/login-feature.test.sh"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
pass=0; fail=0
cat > "$T/fx.bats" <<'B'
@test "passes" { true; }
@test "fails" { false; }
B
feat() { printf 'Feature: f\n%s\n' "$1" > "$T/f.feature"; bash "$RUN" "$T/f.feature" "$T" > "$T/out" 2>&1; echo $?; }
chk() { [ "$2" = "$3" ] && grep -qF -- "$4" "$T/out" && { echo "PASS $1"; pass=$((pass+1)); } || { echo "FAIL $1 (exit $2, want $3; out: $(tail -1 "$T/out"))"; fail=$((fail+1)); }; }
chk "a proven scenario is green"        "$(feat $'  Scenario: A\n    # proven by: fx.bats :: passes')" 0 "1 green, 0 red"
chk "NEGATIVE: a failing case is red"   "$(feat $'  Scenario: A\n    # proven by: fx.bats :: fails')"  1 "0 green, 1 red"
chk "NEGATIVE: a missing case is red"   "$(feat $'  Scenario: A\n    # proven by: fx.bats :: nope')"   1 "case not found once"
chk "NEGATIVE: no case at all is red"   "$(feat $'  Scenario: A')"                                    1 "no case proves it"
chk "NEGATIVE: waiting is red, by card (reported, not a pipeline block)" "$(feat $'  Scenario: A\n    # waiting on: #9999')" 0 "0 green, 1 red (waiting on #9999)"
chk "NEGATIVE: one red case reds the scenario" "$(feat $'  Scenario: A\n    # proven by: fx.bats :: passes\n    # proven by: fx.bats :: fails')" 1 "0 green, 1 red"
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
