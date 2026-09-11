#!/usr/bin/env bats
# @test-type: unit — reads the jest config and the runner source; no live service
#
# #4111 — the hermetic leg must run the hermetic project, not both.
#
# platform/api's jest config declares two projects, `hermetic` and
# `integration`. Bare jest runs BOTH. Inside act there is no live stack, so on
# run 29 every *.integration.test.ts failed on a service it could not reach:
#
#     jest:platform/api   150 failed of 2355, 15 suites
#     hermetic project alone: 2025 passed, 0 failed, 21s
#
# None of those 150 were about the code. The registry-unreachable fallback made
# it worse by running the FULL package suite, so a registry outage rendered as a
# wall of red indistinguishable from a broken build.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  CFG="$ROOT/platform/api/jest.config.js"
  RUNNER="$ROOT/platform/services/werk-test/src/main.rs"
}

@test "the jest config still declares the two projects this split depends on" {
  grep -q "displayName: 'hermetic'" "$CFG"
  grep -q "displayName: 'integration'" "$CFG"
}

@test "the hermetic project excludes the integration tier" {
  # If this stops being true the flag below buys nothing — the hermetic project
  # would itself be running needs-stack tests.
  run bash -c "sed -n '/displayName: .hermetic./,/^};/p' '$CFG' | grep -c 'integration'"
  [ "$output" -ge 1 ]
}

@test "run_jest selects the hermetic project only when the integration tier is OFF (#4139)" {
  # #4111 passed --selectProjects hermetic unconditionally; #4139 made the flag
  # decide: RUN_INTEGRATION=true → bare jest (both projects), else hermetic only.
  LIB="$ROOT/platform/services/werk-test/src/lib.rs"
  grep -q 'werk_test::jest_project_args(jest_has_hermetic_project(&pkg_dir), run_integration)' "$RUNNER"
  grep -q 'std::env::var("RUN_INTEGRATION")' "$RUNNER"
  run bash -c "sed -n '/^pub fn jest_project_args/,/^}/p' '$LIB'"
  [[ "$output" == *'has_hermetic_project && !run_integration'* ]]
  [[ "$output" == *'"--selectProjects"'* ]]
}

@test "NEGATIVE PROOF: with the tier ON the runner passes no --selectProjects (the #4111 state)" {
  # the state #4111 could not separate: stack up, RUN_INTEGRATION=true, and
  # jest still told to run hermetic only. The pure fn's own unit tests
  # (lib.rs integration_tier_4139) prove both branches; here: the runner has
  # no other path to the flag.
  # code only — the doc comment above run_jest_with still names the flag
  run bash -c "grep -c 'arg(\"--selectProjects\")' '$RUNNER'"
  [ "$output" -eq 0 ]
}

@test "NEGATIVE PROOF: the guard says no for a config without the project" {
  grep -q "fn jest_has_hermetic_project" "$RUNNER"
  run bash -c "grep -A8 'fn jest_has_hermetic_project' '$RUNNER' | grep -c 'displayName'"
  [ "$output" -ge 1 ]
}

@test "by hand: with RUN_INTEGRATION=true bare jest lists the integration files (skip-if-absent: needs node_modules)" {
  J="$ROOT/platform/api/node_modules/.bin/jest"
  [ -x "$J" ] || skip "platform/api/node_modules absent in this tree"
  # the werk lane exports RUN_INTEGRATION=true to every child when the stack is
  # up (#4102) — clear it explicitly for the "off" half or the proof is hollow
  run bash -c "cd '$ROOT/platform/api' && env RUN_INTEGRATION=true '$J' --listTests 2>/dev/null | grep -c 'integration.test.ts'"
  [ "$output" -ge 1 ]
  run bash -c "cd '$ROOT/platform/api' && env -u RUN_INTEGRATION '$J' --listTests 2>/dev/null | grep -c 'integration.test.ts'"
  [ "$output" -eq 0 ]
}
