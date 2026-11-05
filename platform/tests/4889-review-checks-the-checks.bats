#!/usr/bin/env bats
# @test-type: unit — the code review checks that each AC's check tests that AC
# @domain: pipelines
# @card: #4889 · owner: silas
# Reads skills/gate-code/SKILL.md, the skill the demo's code step runs.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

# ok when a code-review skill tells the reviewer to check each AC's check against its AC,
# as a numbered item that fails the review (it is in the skill's fail-items)
checks_the_checks() {
  grep -qiE "^1\. \*\*Does each AC's check test that AC\?" "$1" && grep -qE '^fail-items: *1' "$1"
}

@test "AC8: the code review is told to check each AC's check against its AC, and a miss fails it" {
  checks_the_checks "$ROOT/skills/gate-code/SKILL.md" || { head -12 "$ROOT/skills/gate-code/SKILL.md"; return 1; }
}

@test "negative proof: a skill without the item, or with it as a note only, is caught" {
  f="$BATS_TEST_TMPDIR/SKILL.md"
  printf -- '---\nfail-items: 1-2\n---\n1. **Is the code correct?**\n' > "$f"
  ! checks_the_checks "$f" || return 1
  printf -- "---\nfail-items: 2\n---\n1. **Does each AC's check test that AC?**\n" > "$f"
  ! checks_the_checks "$f" || return 1
}

@test "negative proof, run through the review: an AC whose check names an unrelated test fails the code step" {
  # werk-demo's code step adds unrelated_checks to /gate-code's findings (src/lib.rs);
  # its tests plant an AC on another card's test and on a missing file, and both are flagged
  run env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$BATS_TEST_TMPDIR/target" cargo test -q \
    --manifest-path "$ROOT/platform/services/werk-demo/Cargo.toml" --test ac_checks_4889 \
    negative_proof_an_ac_whose_check
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$output" == *"2 passed"* ]] || { echo "the two planted cases did not run: $output"; return 1; }
}
