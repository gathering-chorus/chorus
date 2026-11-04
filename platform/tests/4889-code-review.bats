#!/usr/bin/env bats
# @test-type: unit — the demo's one code review calls /code-review on the diff, never the AC boxes
# @domain: pipelines
# @card: #4889 · owner: silas
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
DEMO="$ROOT/platform/pipelines/demo.yaml"

@test "AC4: one code review step, and it runs the existing /code-review skill on the card's diff" {
  [ "$(grep -c 'name: skill-werk-demo-code-review$' "$DEMO")" -eq 1 ] || return 1
  ! grep -q 'name: skill-werk-demo-review-[0-9]' "$DEMO" || { echo "the three peer prompts still run"; return 1; }
  grep -A3 'name: skill-werk-demo-code-review$' "$DEMO" | grep -q 'code-review' || return 1
}
