#!/usr/bin/env bats
# #2602 — clippy::cognitive_complexity warn-level enabled in chorus-hooks + chorus-inject
# Per #2601 spike: 13 known cog-complexity hits in chorus-hooks bin (top 65/57/47).
# This card enables the lint at warn-level so existing pre-commit cargo clippy
# surfaces them. NOT pre-commit gate (warn != error); also feeds #2527 drift lane.

ROOT="${CHORUS_ROOT_FOR_TEST:-/Users/jeffbridwell/CascadeProjects/chorus}"

@test "chorus-hooks Cargo.toml enables clippy::cognitive_complexity warn" {
  run grep -E '^cognitive_complexity\s*=\s*"warn"' "$ROOT/platform/services/chorus-hooks/Cargo.toml"
  [ "$status" -eq 0 ]
}

@test "chorus-inject Cargo.toml enables clippy::cognitive_complexity warn" {
  run grep -E '^cognitive_complexity\s*=\s*"warn"' "$ROOT/platform/services/chorus-inject/Cargo.toml"
  [ "$status" -eq 0 ]
}

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
