#!/usr/bin/env bats
# @test-type: unit — after land: the next card's cicd runs are counted (AC10 is ticked only then)
# @domain: pipelines
# @card: #4889 · owner: silas
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

@test "AC10 (after land): the inventory names how the next card's runs are counted" {
  grep -q 'next card' "$ROOT/designing/docs/proof-layers-4889.md" || return 1
}
