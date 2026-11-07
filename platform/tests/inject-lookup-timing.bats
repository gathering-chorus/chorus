#!/usr/bin/env bats
# @domain: memory — per-prompt context, the domain context-inject-latency-spec guards (#4334)
# @test-type: unit
# #4815 — context.inject.response names each lookup's own time, so a slow prompt
# says which lookup made it slow. Negative proof (#3734): a fixture lookup that
# sleeps 3 s is named the slowest.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/platform/services/chorus-hooks/target/$(id -un)}"
}

@test "AC1+AC3: each lookup's ms is logged by name, and a 3 s fixture lookup is named slowest" {
  run cargo test --manifest-path "$ROOT/platform/services/chorus-hooks/Cargo.toml" --bin chorus-hooks -q -- a_three_second_lookup_is_named_slowest no_lookups_names_no_slowest
  [ "$status" -eq 0 ]
  [[ "$output" == *"2 passed"* ]] || return 1
}

# #4835 — the unit proofs behind the speed fix: forty prompts share one Loki read,
# the memory files are read once per minute, and the Boston offset is asked once
# per minute instead of a `date` fork per log line. Negative proofs: with sharing
# off every caller reads, an empty Loki answer is never kept, the OS path forks.
@test "#4835: shared Loki read, cached memory corpus, cached Boston offset (with negative proofs)" {
  run cargo test --manifest-path "$ROOT/platform/services/chorus-hooks/Cargo.toml" --bin chorus-hooks -q -- \
    concurrent_callers_share_one_read ttl_zero_reads_every_time memory_match_needs_two_keywords offset_cache_4835 a_hung_refresh_does_not_block_other_callers
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"6 passed"* ]] || return 1
}

# #4839 AC1 — the prompt route names each serial step's ms (turn-ended, cycle, clock,
# classifier, inject, pattern, guard, decision) and the slowest, so time outside the
# context lookups is named. Negative proof: a slow step outside inject is the one named.
@test "#4839 AC1: route steps named by ms, slowest called out (negative proof included)" {
  run cargo test --manifest-path "$ROOT/platform/services/chorus-hooks/Cargo.toml" --bin chorus-hooks -q -- route_steps_4839
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"2 passed"* ]] || return 1
}
