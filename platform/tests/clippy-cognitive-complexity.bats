#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
load test_helper
# #2602 — clippy::cognitive_complexity warn-level enabled in chorus-hooks + chorus-inject
# Per #2601 spike: 13 known cog-complexity hits in chorus-hooks bin (top 65/57/47).
# This card enables the lint at warn-level so existing pre-commit cargo clippy
# surfaces them. NOT pre-commit gate (warn != error); also feeds #2527 drift lane.

ROOT="${CHORUS_ROOT_FOR_TEST:-${CHORUS_ROOT}}"

@test "chorus-hooks Cargo.toml enables clippy::cognitive_complexity warn" {
  run grep -E '^cognitive_complexity\s*=\s*"warn"' "$ROOT/platform/services/chorus-hooks/Cargo.toml"
  [ "$status" -eq 0 ]
}

@test "chorus-inject Cargo.toml enables clippy::cognitive_complexity warn" {
  run grep -E '^cognitive_complexity\s*=\s*"warn"' "$ROOT/platform/services/chorus-inject/Cargo.toml"
  [ "$status" -eq 0 ]
}

@test "clippy-ratchet baseline records cognitive_complexity (enforcement, not noise)" {
  run grep -E '"clippy::cognitive_complexity"' "$ROOT/.clippy-baseline.json"
  [ "$status" -eq 0 ]
}

@test "clippy-ratchet runs and passes against current baseline" {
  cd "$ROOT"
  run bash platform/scripts/clippy-ratchet.sh
  [ "$status" -eq 0 ]
  echo "$output" | grep -qi "PASS"
}

@test "pre-commit wires clippy-ratchet on Rust file changes" {
  run grep -F "clippy-ratchet" "$ROOT/platform/hooks/pre-commit"
  [ "$status" -eq 0 ]
}
