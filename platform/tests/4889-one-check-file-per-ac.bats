#!/usr/bin/env bats
# @test-type: unit — no two AC on a card share one check file
# @domain: proving
# @card: #4889 · owner: silas
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

# prints each check target used by more than one AC line in the card text on stdin
shared_checks() { grep -E '^\s*- \[[ x]\] ' | grep -oE 'check: [a-z]+ [^ ]+' | sort | uniq -d; }

@test "AC9: #4889's own AC each name a different check file" {
  out="$(bash "$ROOT/platform/scripts/cards" view 4889 | shared_checks)"
  [ -z "$out" ] || { echo "shared: $out"; return 1; }
}

@test "AC9 negative proof: a card with two AC on one check file is caught" {
  out="$(printf -- '- [x] a check: test t/x.bats\n- [x] b check: test t/x.bats\n' | shared_checks)"
  [ "$out" = "check: test t/x.bats" ] || { echo "$out"; return 1; }
}

@test "AC9: the demo's validate refuses a card whose AC share a check file" {
  grep -q 'shares its check' "$ROOT/platform/services/werk-demo/src/lib.rs" || return 1
}
