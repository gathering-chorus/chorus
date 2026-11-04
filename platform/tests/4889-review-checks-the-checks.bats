#!/usr/bin/env bats
# @test-type: unit — the code review checks that each AC's check tests that AC
# @domain: pipelines
# @card: #4889 · owner: silas
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

@test "AC8: the code review is told to check each AC's check against its AC" {
  grep -rqi "each AC's check\|check really tests\|check tests that AC" "$ROOT/platform/services/werk-demo/src/lib.rs" "$ROOT/skills" 2>/dev/null || return 1
}
