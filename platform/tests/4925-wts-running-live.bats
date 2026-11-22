#!/usr/bin/env bats
# @test-type: unit — chorus-werk-test-state's running block over planted counts and failures; no Loki
# @domain: tests
# @card: #4925 · owner: kade
# Jeff 11-22 08:32: /wts on a running nightly said "29 suites, 0 red so far" while 3 cases had
# already failed. The running block counts the run's own case events and names each failure.
# Round 2 (review): counts come from Loki's metric query (never a 5,000-line page), and an
# unreadable answer reads UNMEASURED, never a crash.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

wts() { # wts <python expr over module m>
  python3 -c '
import importlib.machinery, importlib.util, json, sys
l = importlib.machinery.SourceFileLoader("wts", sys.argv[1])
m = importlib.util.module_from_spec(importlib.util.spec_from_loader("wts", l)); l.exec_module(m)
print(eval(sys.argv[2]))' "$ROOT/platform/scripts/chorus-werk-test-state" "$1"
}

@test "NEGATIVE PROOF: a planted failed case shows by file, case and reason, never as 0 failed" {
  run wts '"\n".join(m.running_block({"passed": 14000, "failed": 1, "skipped": 2}, [{"file":"platform/scripts/test-role-state-spine.sh","case":"t","reason":"a role-state write still lands"}]))'
  [[ "$output" == *"14,000 passed · 1 failed · 2 skipped"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"✗ platform/scripts/test-role-state-spine.sh   1 failed"* ]] || { echo "$output"; return 1; }
  [[ "$output" == *"a role-state write still lands"* ]] || { echo "$output"; return 1; }
}

@test "a case reported failed twice (a retry) is listed once" {
  run wts '"\n".join(m.running_block({"failed": 2}, [{"file":"a.bats","case":"x","reason":"r"},{"file":"a.bats","case":"x","reason":"r"}]))'
  [[ "$output" == *"a.bats   1 failed"* ]] || { echo "$output"; return 1; }
}

@test "NEGATIVE PROOF: an unreachable or unreadable Loki is UNMEASURED (None), never a crash or 0" {
  run env LOKI_URL=http://127.0.0.1:9 bash -c "$(declare -f wts); ROOT='$ROOT'; wts 'm.live_events(\"2026-11-22T07:24:00\")'"
  [ "$status" -eq 0 ] || { echo "crashed: $output"; return 1; }
  [ "$output" = "None" ] || { echo "$output"; return 1; }
}
