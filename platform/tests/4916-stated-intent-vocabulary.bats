#!/usr/bin/env bats
# @test-type: unit
# @domain: autonomy
# @card: 4916
# Proves stated intent vocabulary exists and is valid JSON.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  VOCAB="$REPO/platform/config/stated-intent-vocabulary.json"
}

@test "stated-intent-vocabulary.json is valid JSON with commitments" {
  [ -f "$VOCAB" ]
  jq -e '.commitments | length > 0' "$VOCAB"
}
