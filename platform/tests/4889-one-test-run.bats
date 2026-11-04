#!/usr/bin/env bats
# @test-type: unit — a round's tests run once
# @domain: pipelines
# @card: #4889 · owner: silas
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

# how many places run the card's tests for one round: the pair's hand-off and cicd
test_runs() {
  local p="$1" n=0
  grep -q 'name: skill-werk-test-pipeline' "$p/cicd.yaml" && n=$((n+1))
  grep -q 'werk-test\|cicd-test\|local.proof' "$p/pair.yaml" "$ROOT/platform/scripts/pipeline-start" 2>/dev/null \
    && grep -q 'local proof\|local_proof\|werk-test .*--scope' "$ROOT/platform/scripts/pipeline-start" && n=$((n+1))
  echo "$n"
}

@test "AC3: one test run per round" {
  [ "$(test_runs "$ROOT/platform/pipelines")" -eq 1 ] || { test_runs "$ROOT/platform/pipelines"; return 1; }
}
