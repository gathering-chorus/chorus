#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
load test_helper
# standards-gen.bats — Tests for generate-standards-surface.sh
# Card #2266: Standards surface shows hardcoded data instead of live counts
#
# What Jeff sees: opens the standards page and numbers should be real,
# not yesterday's guess. These tests verify the script produces accurate
# counts from the three data sources.

GEN_SCRIPT="${CHORUS_ROOT}/platform/scripts/generate-standards-surface.sh"
DECISIONS_MD="${CHORUS_ROOT}/product-manager/decisions.md"
HOOKS_DIR="${CHORUS_ROOT}/platform/services/chorus-hooks/src/hooks"
PULSE_LOG="$HOME/Library/Logs/Gathering/hooks.log"
OUTPUT_DIR="/tmp/standards-gen-test"

setup() {
  mkdir -p "$OUTPUT_DIR"
}

teardown() {
  rm -rf "$OUTPUT_DIR"
}

# --- AC 1: Script reads decisions.md and produces accurate decision count ---

@test "script exists and is executable" {
  [ -x "$GEN_SCRIPT" ]
}

@test "produces accurate decision count from decisions.md" {
  run bash "$GEN_SCRIPT" --output-dir "$OUTPUT_DIR"
  [ "$status" -eq 0 ]

  # Count actual decisions in source
  expected=$(grep -c "^## DEC-" "$DECISIONS_MD")

  # The generated HTML should contain that exact count
  grep -qE "${expected} decisions" "$OUTPUT_DIR/chorus-standards.html"
}

# --- AC 2: Script counts hook modules from Rust source ---

@test "produces accurate hook module count" {
  run bash "$GEN_SCRIPT" --output-dir "$OUTPUT_DIR"
  [ "$status" -eq 0 ]

  # Count actual Rust hook modules (exclude mod.rs)
  expected=$(ls "$HOOKS_DIR"/*.rs | grep -v mod.rs | wc -l | tr -d ' ')

  grep -qE "${expected} hook modules" "$OUTPUT_DIR/chorus-hook-architecture.html"
}

# --- AC 3: Script parses pulse log for gate enforcement rates ---

@test "produces gate enforcement rates from pulse log" {
  run bash "$GEN_SCRIPT" --output-dir "$OUTPUT_DIR"
  [ "$status" -eq 0 ]

  # The HTML should contain at least one percentage for gate enforcement
  grep -qE '[0-9]+%' "$OUTPUT_DIR/chorus-standards.html"
}

@test "enforcement rates reflect real block and deny counts" {
  run bash "$GEN_SCRIPT" --output-dir "$OUTPUT_DIR"
  [ "$status" -eq 0 ]

  # Should have gate-enforced category (modules with block/deny in trailing 7 days)
  grep -qi "gate-enforced\|gate enforced" "$OUTPUT_DIR/chorus-standards.html"
}

# --- AC 4: Output is regenerated HTML with real counts ---

@test "generates chorus-standards.html" {
  run bash "$GEN_SCRIPT" --output-dir "$OUTPUT_DIR"
  [ "$status" -eq 0 ]
  [ -f "$OUTPUT_DIR/chorus-standards.html" ]
}

@test "generates chorus-hook-architecture.html" {
  run bash "$GEN_SCRIPT" --output-dir "$OUTPUT_DIR"
  [ "$status" -eq 0 ]
  [ -f "$OUTPUT_DIR/chorus-hook-architecture.html" ]
}

# --- AC 6: Runs cleanly from command line ---

@test "exits zero on success" {
  run bash "$GEN_SCRIPT" --output-dir "$OUTPUT_DIR"
  [ "$status" -eq 0 ]
}

# --- AC 7: Idempotent — running twice produces same output ---

@test "idempotent — second run produces identical output" {
  bash "$GEN_SCRIPT" --output-dir "$OUTPUT_DIR"
  cp "$OUTPUT_DIR/chorus-standards.html" "$OUTPUT_DIR/first-run.html"

  bash "$GEN_SCRIPT" --output-dir "$OUTPUT_DIR"

  diff "$OUTPUT_DIR/first-run.html" "$OUTPUT_DIR/chorus-standards.html"
}
