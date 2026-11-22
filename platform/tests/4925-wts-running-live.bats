#!/usr/bin/env bats
# @test-type: unit — chorus-werk-test-state's running block over planted case events; no Loki
# @domain: tests
# @card: #4925 · owner: kade
# Jeff 11-22 08:32: /wts on a running nightly said "29 suites, 0 red so far" while 3 cases had
# already failed. The running block now reads the run's own case events and names each failure.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

block() { # stdin: a JSON list of events -> the running block
  python3 -c '
import importlib.machinery, importlib.util, json, sys
l = importlib.machinery.SourceFileLoader("wts", sys.argv[1])
m = importlib.util.module_from_spec(importlib.util.spec_from_loader("wts", l)); l.exec_module(m)
print("\n".join(m.running_block(json.load(sys.stdin))))' "$ROOT/platform/scripts/chorus-werk-test-state"
}

@test "NEGATIVE PROOF: a planted failed case shows by file, case and reason, never as 0 failed" {
  run block <<< '[{"event":"test.case.started","file":"a.bats","case":"x"},
    {"event":"test.case.passed","file":"a.bats","case":"x"},
    {"event":"test.case.failed","file":"platform/scripts/test-role-state-spine.sh","case":"test-role-state-spine.sh","reason":"a role-state write still lands"},
    {"event":"test.case.skipped","file":"b.bats","case":"y"}]'
  [[ "$output" == *"1 passed · 1 failed · 1 skipped"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"✗ platform/scripts/test-role-state-spine.sh   1 failed"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"a role-state write still lands"* ]] || { echo "$output"; return 1; }
}

@test "a case that failed then passed on retry counts once, by its last word" {
  run block <<< '[{"event":"test.case.failed","file":"a.bats","case":"x","reason":"r"},{"event":"test.case.passed","file":"a.bats","case":"x"}]'
  [[ "$output" == *"1 passed · 0 failed · 0 skipped"* ]] || { echo "$output"; return 1; }
}
