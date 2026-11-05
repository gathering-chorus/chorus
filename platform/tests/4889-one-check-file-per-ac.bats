#!/usr/bin/env bats
# @test-type: unit — no two AC on a card share one check file
# @domain: proving
# @card: #4889 · owner: silas
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

# prints each check target used by more than one AC line in the card text on stdin
shared_checks() { grep -E '^\s*- \[[ x]\] ' | grep -oE 'check: (after )?[a-z]+ [^ ]+' | sed 's/check: after /check: /' | sort | uniq -d; }

@test "AC9: #4889's own AC each name a different check file" {
  out="$(bash "$ROOT/platform/scripts/cards" view 4889 | shared_checks)"
  [ -z "$out" ] || { echo "shared: $out"; return 1; }
}

@test "AC9 negative proof: a card with two AC on one check file is caught" {
  out="$(printf -- '- [x] a check: test t/x.bats\n- [x] b check: test t/x.bats\n' | shared_checks)"
  [ "$out" = "check: test t/x.bats" ] || { echo "$out"; return 1; }
}

@test "AC9: the demo's validate refuses a card whose AC share a check file or name none" {
  # the refusal's own tests, negative proofs included (tests/ac_checks_4889.rs)
  run env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$BATS_TEST_TMPDIR/target" cargo test -q \
    --manifest-path "$ROOT/platform/services/werk-demo/Cargo.toml" --test ac_checks_4889
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"7 passed"* ]] || { echo "$output"; return 1; }
}
