#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
load test_helper
# ac-autocheck.bats — verify demoCard auto-checks AC items (#2017)
#
# Bug: roles demo cards with unchecked AC boxes, gate:product fails,
# 2 extra touches per card for a clerical step.
# Fix: demoCard auto-checks all AC boxes before gates run.

CARDS_CLI="${CHORUS_ROOT}/platform/scripts/cards"
SDK_SRC="${CHORUS_ROOT}/directing/products/cards/src/sdk.ts"

@test "demoCard function auto-checks AC items" {
  # The demoCard function must contain AC auto-check logic
  grep -q 'autoCheckAC\|auto-check\|replace.*\[ \].*\[x\]' "$SDK_SRC"
}

@test "auto-check replaces unchecked boxes with checked" {
  # The replacement pattern must convert - [ ] to - [x]
  grep -q '\- \[ \].*\- \[x\]' "$SDK_SRC" || grep -q "replace.*\\[ \\].*\\[x\\]" "$SDK_SRC"
}
