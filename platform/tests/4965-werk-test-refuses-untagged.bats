#!/usr/bin/env bats
# @test-type: unit — werk-test's untagged-test rule, run by name through cargo; no door
# @domain: tests
# @card: #4965 · owner: kade
# #4965 AC6 (from #4935): werk-test refuses a changed test file with no @domain tag,
# naming the file (main.rs refuse_unlisted_domains → lib.rs untagged_test_files).
# Negative proof (#3734): a fixture with no tag is named; the same fixture tagged passes.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
MANIFEST="$ROOT/platform/services/werk-test/Cargo.toml"
MAIN="$ROOT/platform/services/werk-test/src/main.rs"

rust_test() { cargo test --quiet --manifest-path "$MANIFEST" --test untagged_domain_4965 -- --exact "$1" 2>&1; }

@test "AC6 guard: the refusal is wired in before the store is asked, and names the file" {
  body="$(sed -n '/^fn refuse_unlisted_domains/,/^}/p' "$MAIN")"
  [ -n "$body" ] || { echo "refuse_unlisted_domains is gone"; return 1; }
  printf '%s\n' "$body" | grep -q 'untagged_test_files' || { echo "the untagged rule is not called"; return 1; }
  printf '%s\n' "$body" | grep -q 'declares no @domain' || { echo "the refusal does not name the file"; return 1; }
  # the untagged check must come before fetch_domain_names: a store outage must not let one through
  u=$(printf '%s\n' "$body" | grep -n 'untagged_test_files' | head -1 | cut -d: -f1)
  s=$(printf '%s\n' "$body" | grep -n 'fetch_domain_names' | head -1 | cut -d: -f1)
  [ "$u" -lt "$s" ] || { echo "untagged check ($u) runs after the store read ($s)"; return 1; }
}

@test "AC6 NEGATIVE PROOF: a test file with no domain tag is named" {
  run rust_test a_test_with_no_domain_tag_is_named
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"1 passed"* ]] || { echo "the test did not run: $output"; return 1; }
}

@test "AC6: the same fixture with a valid tag passes" {
  run rust_test the_same_file_with_a_tag_passes
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"1 passed"* ]] || { echo "the test did not run: $output"; return 1; }
}
