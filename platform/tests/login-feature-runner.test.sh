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
@test "login: A" { true; }
@test "login: A — also checked" { true; }
B
GWT=$'\n    Given g\n    When w\n    Then t'
feat() { printf 'Feature: f\n%s\n' "$1" > "$T/f.feature"; bash "$RUN" "$T/f.feature" "$T" > "$T/out" 2>&1; echo $?; }
chk() { [ "$2" = "$3" ] && grep -qF -- "$4" "$T/out" && { echo "PASS $1"; pass=$((pass+1)); } || { echo "FAIL $1 (exit $2, want $3; out: $(tail -1 "$T/out"))"; fail=$((fail+1)); }; }
chk "a scenario with steps and its named cases is green" "$(feat "  Scenario: A$GWT")" 0 "1 green, 0 red"
chk "NEGATIVE: renaming the scenario reds it (drift guard)" "$(feat "  Scenario: A renamed$GWT")" 1 "no case is named \"login: A renamed\""
chk "NEGATIVE: renaming the scenario leaves its old cases orphaned" "$(feat "  Scenario: A renamed$GWT")" 1 "case \"login: A\" in fx.bats names no scenario"
chk "NEGATIVE: a scenario without a Then step is red" "$(feat $'  Scenario: A\n    Given g\n    When w')" 1 "no Then step"
chk "NEGATIVE: a scenario without a Given step is red" "$(feat $'  Scenario: A\n    When w\n    Then t')" 1 "no Given step"
chk "NEGATIVE: waiting is red, by card (reported, not a pipeline block)" "$(feat "  Scenario: A$GWT
  Scenario: W$GWT
    # waiting on: #9999")" 0 "1 green, 1 red (waiting on #9999)"
echo '@test "login: F" { false; }' >> "$T/fx.bats"
chk "NEGATIVE: a failing case reds its scenario" "$(feat "  Scenario: A$GWT
  Scenario: F$GWT")" 1 "1 green, 1 red"
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
