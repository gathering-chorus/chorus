#!/usr/bin/env bats
# @test-type: integration — runs a real cargo clippy compile
# #3710 — split out of clippy-cognitive-complexity.bats. That file declares
# itself a hermetic source guard and its other cases are (they grep Cargo.toml
# and the ratchet baseline). These two shell out to `cargo clippy`, which needs a
# Rust toolchain and a populated CARGO_HOME — not hermetic, and they were failing
# on any environment without one. @test-type is per-FILE, so the honest split is
# a second file rather than re-tiering the static asserts out of the unit lane.
load test_helper

ROOT="${CHORUS_ROOT}"

@test "chorus-hooks cargo clippy emits cognitive_complexity warnings" {
  cd "$ROOT/platform/services/chorus-hooks"
  run bash -c "cargo clippy --bin chorus-hook-shim 2>&1 | grep -c 'cognitive_complexity'"
  [ "$status" -eq 0 ]
  # At least one warning expected (spike found 13 hits)
  [ "$output" -ge 1 ]
}

@test "warn-level does not fail clippy build" {
  cd "$ROOT/platform/services/chorus-hooks"
  run bash -c "cargo clippy --bin chorus-hook-shim 2>&1; echo EXIT=\$?"
  echo "$output" | grep -q "EXIT=0" || (echo "expected exit 0 (warn doesn't fail), got: $output" && false)
}

