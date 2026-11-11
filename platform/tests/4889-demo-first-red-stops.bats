#!/usr/bin/env bats
# @test-type: unit — a red review fails its own demo step
# @domain: pipelines
# @card: #4889 · owner: silas
# What Jeff sees (11-05 10:06): a red review wakes the owner; only an all-green demo reaches him.
# #4953 (11-11) runs the three reviews together (demo-reviews-parallel.bats proves the shape and
# that a red one fails the run before announce); this file keeps the step's own exit: a red
# review must fail its step, or dagu would carry on to announce.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

@test "a red review fails its own step, so the chain above really stops (B7: run 14's reds reached the go step)" {
  # the step's exit, not a planted one: werk-demo run-gate on a recorded fail exits 1, a pass or
  # a notes-only round exits 0 (tests/code_step_4889.rs asserts each exit)
  WD="$ROOT/platform/services/werk-demo"
  run env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$WD/target/$(id -un)" nice cargo test -q --manifest-path "$WD/Cargo.toml" --test code_step_4889
  [ "$status" -eq 0 ] || { echo "$output" | tail -20; return 1; }
  [[ "$output" == *"1 passed"* ]] || { echo "$output" | tail -5; return 1; }
}
