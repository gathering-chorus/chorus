#!/usr/bin/env bats
# @test-type: unit — runs the real contract checker on the real inventory and on planted fixtures; reads files only.
# @domain: cicd — the product domain this suite guards (#4334)
#
# #4333 — the werk-code contract (platform/config/werk-code-contract.tsv) names,
# for every enforcement mechanism, the fixture that proves it goes red. Editing
# the inventory runs this suite: the checker must still pass on the real file,
# and still go red on each planted violation.

setup() {
  ROOT="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}"
}

@test "the real inventory (platform/config/werk-code-contract.tsv) has no violations" {
  run bash "$ROOT/platform/scripts/werk-code-contract.sh"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q " 0 violations"
}

@test "the checker goes red on every planted violation (its own negative proofs)" {
  run bash "$ROOT/platform/scripts/test-werk-code-contract.sh"
  [ "$status" -eq 0 ]
  echo "$output" | grep -q ", 0 failed"
}
