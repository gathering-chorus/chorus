#!/usr/bin/env bats
# @test-type: unit
# #3799 AC1 — the compact success check must verify RECLAIM, not just that the
# Jena task set .finished (a failed/no-op compact finishes too). Negative proof
# (#3734): a compact that leaves the store the SAME size is a FAILURE, and the
# check is shown to reject it — the exact state today's check can't tell from success.

# the decision function under test, sourced from the maintenance script
SCRIPT="${BATS_TEST_DIRNAME}/../../building/products/convergence/fuseki-maintenance.sh"

load_fn() {
  # pull just the compact_reclaimed() function out of the script (no Fuseki needed)
  eval "$(awk '/^compact_reclaimed\(\)/{f=1} f{print} f&&/^}/{exit}' "$SCRIPT")"
}

@test "compact_reclaimed exists (script defines the honest check)" {
  load_fn
  declare -f compact_reclaimed >/dev/null
}

@test "same size before/after = NOT reclaimed = FAIL (the hollow case)" {
  load_fn
  run compact_reclaimed 498000000000 498000000000
  [ "$status" -ne 0 ]
}

@test "shrunk store = reclaimed = OK" {
  load_fn
  run compact_reclaimed 498000000000 62000000000
  [ "$status" -eq 0 ]
}

@test "grew (compact wrote a bloated copy) = FAIL" {
  load_fn
  run compact_reclaimed 400000000000 498000000000
  [ "$status" -ne 0 ]
}
