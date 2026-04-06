#!/usr/bin/env bats
# pulse-bar.bats — Tests for pulse bar wiring (#2267)
# What Jeff sees: GATE/PARTIAL/DOC tags on standards are manually assigned.
# After this card, tags reflect actual hook enforcement data with deny counts.

GEN_SCRIPT="/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/generate-standards-surface.sh"
OUTPUT_DIR="/tmp/pulse-bar-test"

setup() {
  mkdir -p "$OUTPUT_DIR"
}

teardown() {
  rm -rf "$OUTPUT_DIR"
}

# --- AC 2: Per-standard tags derived from actual execution ---

@test "standards with active hook enforcement show GATE tag" {
  run bash "$GEN_SCRIPT" --output-dir "$OUTPUT_DIR"
  [ "$status" -eq 0 ]
  grep -A2 "TDD Discipline" "$OUTPUT_DIR/chorus-standards.html" | grep -q "tag-gate"
}

@test "standards without hook enforcement show DOC tag" {
  run bash "$GEN_SCRIPT" --output-dir "$OUTPUT_DIR"
  [ "$status" -eq 0 ]
  grep -A2 "Simplest solution first" "$OUTPUT_DIR/chorus-standards.html" | grep -q "tag-doc"
}

# --- AC 4: Tooltip/detail shows deny count per gate ---

@test "enforced standards show deny count in tooltip" {
  run bash "$GEN_SCRIPT" --output-dir "$OUTPUT_DIR"
  [ "$status" -eq 0 ]
  # GATE-tagged standards should have title="module: N denies (7d)" on parent div
  grep -B1 "TDD Discipline" "$OUTPUT_DIR/chorus-standards.html" | grep -qE 'title=.*denies'
}

@test "hook architecture page shows per-module deny counts" {
  run bash "$GEN_SCRIPT" --output-dir "$OUTPUT_DIR"
  [ "$status" -eq 0 ]
  grep -q "denies" "$OUTPUT_DIR/chorus-hook-architecture.html"
}

@test "hook architecture header shows enforcement summary" {
  run bash "$GEN_SCRIPT" --output-dir "$OUTPUT_DIR"
  [ "$status" -eq 0 ]
  grep -q "modules actively enforcing" "$OUTPUT_DIR/chorus-hook-architecture.html"
}

@test "hook table rows have deny count tooltips" {
  run bash "$GEN_SCRIPT" --output-dir "$OUTPUT_DIR"
  [ "$status" -eq 0 ]
  # At least one table cell should have a title with deny count
  grep -qE 'td title="[0-9]+ denies' "$OUTPUT_DIR/chorus-hook-architecture.html"
}
