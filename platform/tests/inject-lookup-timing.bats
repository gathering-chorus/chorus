#!/usr/bin/env bats
# @domain: hooks
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
