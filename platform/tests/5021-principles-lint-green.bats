#!/usr/bin/env bats
# @test-type: contract — the principles lint over today's snapshot with this card's changes applied; no store, no network.
# @domain: principles
# @card: #5021 · owner: kade
# What Jeff sees: principles is the first domain that passes its own standard:
# `policy-validate --domain principles` is green.
#
# AC1: every complete-domain rule passes for principles once this card's changes are applied to
# what the store held today (the dangling links retired, the Domain row redeployed whole, the side
# doors gone from the tree). The fixture names: platform/tests/fixtures/5021/after.py,
# platform/tests/fixtures/5021/principles-2026-11-20.part-a{a..d}.nq.

# Shared setup, named so a change to it runs this suite: platform/tests/5021-helpers.bash
load 5021-helpers

setup() { build_5021; }

@test "AC1: policy-validate --domain principles is green after this card" {
  run lint after
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q '0 above baseline, 0 could not run (domain principles)' || { echo "$output"; return 1; }
  # every complete-domain rule ran, none skipped
  [ "$(echo "$output" | grep -c '^PASS domain-')" -eq 8 ] || { echo "$output"; return 1; }
}
