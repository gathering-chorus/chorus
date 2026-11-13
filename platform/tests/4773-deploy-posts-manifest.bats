#!/usr/bin/env bats
# @test-type: unit — werk-deploy's manifest tests, each against its own in-process stub door
# @domain: cicd
# @card: #4773 · owner: borg-queen
# #4965 — this file used to start a python stub on :33774 and grep its log, but the Rust
# tests post to their own stub (start_stub_door), so the log never existed and every
# run was red ("[ -f $REQUESTS_LOG ] failed"); a test binary found by `find` in
# target/debug/deps could also be a stale build. The assertions now live in the Rust
# tests, run here by name through cargo.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
MANIFEST="$ROOT/platform/services/werk-deploy/Cargo.toml"
LIB="$ROOT/platform/services/werk-deploy/src/lib.rs"

rust_test() { cargo test --quiet --manifest-path "$MANIFEST" --lib -- --exact "manifest_4773::$1" 2>&1; }

@test "4773 AC3 guard: each manifest test posts to its own stub door, never :3360 or :3365" {
  for t in test_post_deployment_manifest_to_stub_door test_post_rollback_manifest_to_stub_door test_post_manifest_rejection_returns_error; do
    body="$(sed -n "/fn $t()/,/^    }/p" "$LIB")"
    [ -n "$body" ] || { echo "test $t is gone"; return 1; }
    printf '%s\n' "$body" | grep -q 'start_stub_door' || { echo "$t has no stub door"; return 1; }
    printf '%s\n' "$body" | grep -qE ':33(60|65)' && { echo "$t names a live door"; return 1; }
  done
  return 0
}

@test "4773 AC3: post_deployment_manifest posts a Deployment record with cdhash, commit, sha256, machine, deployer" {
  run rust_test test_post_deployment_manifest_to_stub_door
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"1 passed"* ]] || { echo "the test did not run: $output"; return 1; }
}

@test "4773 AC3 rollback: posting a rollback manifest carries rollbackOf" {
  run rust_test test_post_rollback_manifest_to_stub_door
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"1 passed"* ]] || { echo "the test did not run: $output"; return 1; }
}

@test "4773 AC3 NEGATIVE PROOF: a rejected manifest (422) is an error, never a success" {
  run rust_test test_post_manifest_rejection_returns_error
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"1 passed"* ]] || { echo "the test did not run: $output"; return 1; }
}
