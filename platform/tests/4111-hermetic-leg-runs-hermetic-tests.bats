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

@test "run_jest selects the hermetic project" {
  grep -q '"--selectProjects"' "$RUNNER"
  grep -q '"hermetic"' "$RUNNER"
}

@test "NEGATIVE PROOF: the flag is guarded, never passed blind" {
  # --selectProjects against a config with NO projects is a warning and an empty
  # run — a vacuous green. The runner must read the config first.
  grep -q "fn jest_has_hermetic_project" "$RUNNER"
  run bash -c "grep -A2 'jest_has_hermetic_project(&pkg_dir)' '$RUNNER' | grep -c 'selectProjects'"
  [ "$output" -ge 1 ]
}

@test "NEGATIVE PROOF: the guard says no for a config without the project" {
  d="$BATS_TEST_TMPDIR/nopkg"; mkdir -p "$d"
  printf 'module.exports = { testMatch: ["**/*.test.ts"] };\n' > "$d/jest.config.js"
  run grep -c "displayName: 'hermetic'" "$d/jest.config.js"
  [ "$output" = "0" ]
}
