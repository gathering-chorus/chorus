#!/usr/bin/env bats
# @test-type: unit — platform/scripts/land-5034-rows against a fixture door (LAND_LIVE_JSON); no store, no runner
# @domain: domains
# @card: #5034 AC4 · owner: wren
#
# The Athena domain page's Layer line read "none recorded" for principles (Jeff's phone,
# 2026-11-24). platform/scripts/land-5034-rows writes the one edge that fills it:
# principles inLayer layer-capability. This holds what the script plans, offline:
#   - no layer on the row      → one change, the layer edge, and nothing else;
#   - the edge already there   → nothing to write (a second deploy is a no-op);
#   - NEGATIVE: a different layer on the row is a change, so the compare can tell the
#     right layer from a wrong one; and a dry run never calls the runner.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
LAND="$ROOT/platform/scripts/land-5034-rows"

setup() {
  T="$(mktemp -d)"
  # a runner that records being called: a dry run must leave no mark
  printf '#!/bin/bash\necho called >> "%s/runner.log"\n' "$T" > "$T/runner"
  chmod +x "$T/runner"
}
teardown() { rm -rf "$T"; }

plan_with() {
  printf '{"domains":[{"name":"principles","label":"principles","inLayer":%s}]}\n' "$1" > "$T/live.json"
  LAND_LIVE_JSON="$T/live.json" CHORUS_RUNNER="$T/runner" run python3 "$LAND"
}

@test "#5034 no layer on the principles row: the plan is one change, the layer edge" {
  plan_with '""'
  test "$status" -eq 0 || { echo "$output"; return 1; }
  echo "$output" | grep -q 'CHANGE domains/principles  {"inLayer": "layer-capability"}' || { echo "$output"; return 1; }
  echo "$output" | grep -q '0 add, 1 change, 0 delete; 0 same' || { echo "$output"; return 1; }
  test "$(echo "$output" | grep -c -E '^(ADD|CHANGE|DELETE) ')" -eq 1 || { echo "more than one write planned: $output"; return 1; }
}

@test "#5034 the edge already there, in the form the door serves it: nothing to write" {
  # the door serves this edge as "layer-capability" (read live 11-24 15:57, after the write)
  plan_with '"layer-capability"'
  echo "$output" | grep -q 'SAME   domains/principles' || { echo "$output"; return 1; }
  echo "$output" | grep -q '0 add, 0 change, 0 delete; 1 same' || { echo "$output"; return 1; }
}

@test "#5034 NEGATIVE: a different layer is a change, and a dry run never calls the runner" {
  plan_with '"layer-build"'
  echo "$output" | grep -q '0 add, 1 change, 0 delete; 0 same' || { echo "a wrong layer read as same: $output"; return 1; }
  echo "$output" | grep -q 'nothing written (dry run)' || { echo "$output"; return 1; }
  test ! -e "$T/runner.log" || { echo "the dry run called the runner"; return 1; }
}
