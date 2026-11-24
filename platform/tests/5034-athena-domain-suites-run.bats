#!/usr/bin/env bats
# @test-type: integration — werk-test's browser-lane selection for the Athena domain page (cargo + playwright --list); no live services
# @domain: tests
# @card: #5034 AC5 · owner: wren (built by kade)
#
# The four athena-domain-*-5034 specs run in the test step of any card that changes
# platform/api/public/athena, the card's own new specs included (before #5034 a card's
# run played only the registry's specs, the crawl of main, so a new spec first ran at
# night after the land), and in the nightly, which tells them so (WERK_TEST_NIGHTLY=1)
# so they read the live discovery instead of the pre-land fixture.
# NEGATIVE: the cargo tests show the registry-only selection misses a card's spec and
# a card run's env carries no nightly flag; the list check fails if any spec is gone.
# Asserts are simple commands, never `[[ ]]` (bash 3.2 hollow-assert).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
WT="$ROOT/platform/services/werk-test"
SPECS="athena-domain-readable-5034 athena-domain-links-5034 athena-domain-true-5034 athena-domain-folds-5034"

cargo_wt() { env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$WT/target/$(id -un)" nice cargo test -q --manifest-path "$WT/Cargo.toml" "$@"; }

@test "#5034 AC5 a card that changes the athena page plays its own new specs in its test step" {
  run cargo_wt --lib card_ui_files_5034
  test "$status" -eq 0 || { echo "$output" | tail -20; return 1; }
  echo "$output" | grep -q "test result: ok. 2 passed" || { echo "$output" | tail -5; return 1; }
}

@test "#5034 AC5 the nightly ui leg tells the specs it is the nightly; a card run does not" {
  run cargo_wt --bin werk-test nightly_ui_env_5034
  test "$status" -eq 0 || { echo "$output" | tail -20; return 1; }
  echo "$output" | grep -q "test result: ok. 1 passed" || { echo "$output" | tail -5; return 1; }
}

@test "#5034 AC5 playwright selects all four specs from the werk root (the nightly runs the whole dir)" {
  cd "$ROOT"
  run npx --no-install playwright test --list
  test "$status" -eq 0 || { echo "$output" | tail -10; return 1; }
  for s in $SPECS; do
    echo "$output" | grep -q "$s.spec.cjs" || { echo "not selected: $s"; return 1; }
  done
}

@test "#5034 AC5 the specs' page lib reads the live discovery when the nightly says so" {
  cd "$ROOT"
  run node -e '
    process.env.WERK_TEST_NIGHTLY = "1";
    const lib = require("./proving/flows/lib/athena-page-5034.cjs");
    if (lib.LIVE_DISCOVERY !== true) { console.log("nightly reads the fixture"); process.exit(1); }'
  test "$status" -eq 0 || { echo "$output"; return 1; }
  run node -e '
    delete process.env.WERK_TEST_NIGHTLY; delete process.env.ATHENA_DISCOVERY;
    const lib = require("./proving/flows/lib/athena-page-5034.cjs");
    if (lib.LIVE_DISCOVERY !== false) { console.log("a card run skips the fixture"); process.exit(1); }'
  test "$status" -eq 0 || { echo "$output"; return 1; }
}
