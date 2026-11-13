#!/usr/bin/env bats
# @test-type: unit — runs one jest file against a design doc on disk; no services
# @domain: awareness
# @card: #4965 · owner: kade
# roles/silas/docs/attention-architecture.test.ts sits in no TS package, so werk-test
# has no runner for it (cicd 034x5EFk test-scope). This suite runs it with clearing's
# jest. It had read a path the doc left in #1829: 5/5 ENOENT before the fix.

WERK="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
JEST_DIR="$WERK/directing/clearing"

# doc_jest <dir holding roles/silas/docs/attention-architecture.test.ts> — jest's output; its exit is jest's
doc_jest() {
  local cfg
  cfg='{"rootDir":"'"$1"'/roles/silas/docs","testEnvironment":"node","testMatch":["**/attention-architecture.test.ts"],"transform":{"^.+\\.ts$":["'"$JEST_DIR"'/node_modules/ts-jest",{"diagnostics":false,"tsconfig":{"esModuleInterop":true}}]}}'
  (cd "$JEST_DIR" && npx jest --ci --config "$cfg" 2>&1)
}

@test "the attention-architecture doc test runs and its 5 cases pass" {
  run doc_jest "$WERK"
  [ "$status" -eq 0 ] || { echo "$output" | tail -20; return 1; }
  [[ "$output" == *"Tests:       5 passed, 5 total"* ]] || { echo "$output" | tail -5; return 1; }
}

@test "NEGATIVE PROOF: with the doc missing, the same test fails" {
  t="$BATS_TEST_TMPDIR/w"; mkdir -p "$t/roles/silas/docs"
  cp "$WERK/roles/silas/docs/attention-architecture.test.ts" "$t/roles/silas/docs/"
  run doc_jest "$t"
  [ "$status" -ne 0 ] || { echo "passed with no doc: $output"; return 1; }
  [[ "$output" == *"ENOENT"* ]] || { echo "$output" | tail -5; return 1; }
}
