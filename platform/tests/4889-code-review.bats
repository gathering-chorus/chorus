#!/usr/bin/env bats
# @test-type: unit — the demo's one code review calls /code-review on the diff, never the AC boxes
# @domain: pipelines
# @card: #4889 · owner: silas
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
DEMO="$ROOT/platform/pipelines/demo.yaml"

@test "AC4: one code review step, and it runs the existing /code-review and /security-review on the whole branch" {
  [ "$(grep -c 'command: "werk-demo run-gate ${CARD} ${ROLE} code"' "$DEMO")" -eq 1 ] || return 1
  ! grep -q 'name: skill-werk-demo-review-[0-9]' "$DEMO" || { echo "the three peer prompts still run"; return 1; }
  # the step's own proof, negative proofs included: argv, cwd, verdict from findings
  run env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$BATS_TEST_TMPDIR/target" cargo test -q \
    --manifest-path "$ROOT/platform/services/werk-demo/Cargo.toml" --test code_step_4889 --test builtin_review_4889
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}
