#!/usr/bin/env bats
# @test-type: contract — the same lint over today's snapshot as the store holds it; no store, no network.
# @domain: principles
# @card: #5021 · owner: kade
# What Jeff sees: the lint that is green after this card was red on the same rows before it,
# so the green is the fix, not a check that cannot fail.
#
# AC3 NEGATIVE PROOF: the same check, on today's rows (platform/tests/fixtures/5021/principles-2026-11-20.nq),
# is red: 14 dangling XP links and 3 missing Domain fields.

# Shared setup, named so a change to it runs this suite: platform/tests/5021-helpers.bash
load 5021-helpers

setup() { build_5021; }

@test "AC3: on today's rows the principles lint is red, naming the reds this card fixes" {
  run lint before
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q '^FAIL domain-referential-integrity 14 > 0' || { echo "$output"; return 1; }
  echo "$output" | grep -q '^FAIL domain-shacl-conforms 3 > 0' || { echo "$output"; return 1; }
  echo "$output" | grep -q 'chorus#xp-humanity' || { echo "$output"; return 1; }
}

@test "AC3: the two runs differ only by this card's changes (same snapshot, same rules)" {
  run lint after
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}
