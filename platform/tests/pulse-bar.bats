#!/usr/bin/env bats
# @test-type: unit — hermetic source guard
load test_helper
# pulse-bar.bats — Tests for pulse bar wiring (#2267)
# What Jeff sees: GATE/PARTIAL/DOC tags on standards are manually assigned.
# After this card, tags reflect actual hook enforcement data with deny counts.
#
# #3710 — brings its own world. This suite needed BOTH the personal-site HTML
# checkout (for the template) and the hooks metrics API live on :3340 (for the
# deny counts), so it only passed on Jeff's machine. What's under test is the
# SUBSTITUTION — given metrics, does the page get the right tags and tooltips —
# so metrics come from a fixture and the template is written here. Same
# assertions, no live service.

GEN_SCRIPT="${CHORUS_ROOT}/platform/scripts/generate-standards-surface.sh"
OUTPUT_DIR="${BATS_TEST_TMPDIR:-/tmp}/pulse-bar-test"
TEMPLATE_DIR="${BATS_TEST_TMPDIR:-/tmp}/pulse-bar-template"
METRICS_FILE="${BATS_TEST_TMPDIR:-/tmp}/pulse-bar-metrics.json"

setup() {
  mkdir -p "$OUTPUT_DIR" "$TEMPLATE_DIR"
  # The GATE/DOC tag itself is CURATED in the page, not generated — the script
  # only decorates an existing tag-gate with its deny tooltip. So the fixture
  # carries the curated tags, and the assertions check that generation preserves
  # them and adds the tooltip for the module that actually denied (tdd_gate).
  cat > "$METRICS_FILE" <<'JSON'
{
  "totalModules": 2,
  "enforcedModules": 1,
  "totalDecisions": 3,
  "modules": {
    "tdd_gate":    { "allow": 10, "deny": 588, "warn": 0 },
    "memory_gate": { "allow":  5, "deny":   0, "warn": 2 }
  }
}
JSON
  cat > "$TEMPLATE_DIR/chorus-standards.html" <<'HTML'
<!DOCTYPE html><html><body>
<div class="date">PLACEHOLDER</div>
<div class="card"><div class="number">0</div><div class="label">Decisions</div></div>
<div class="card"><div class="number">0</div><div class="label">Feedback Rules</div></div>
<div class="card"><div class="number">0</div><div class="label">Stories</div></div>
<div class="bar"><span class="bar-instrumented" style="width: 0%"></span>
<span class="bar-partial" style="width: 0%"></span>
<span class="bar-documented" style="width: 0%"></span></div>
<ul class="legend"><li>gate-enforced 0%</li><li>partial 0%</li><li>doc-only 0%</li></ul>
<div class="standard"><h3>TDD Discipline</h3><span class="tag tag-gate">GATE</span></div>
<div class="standard"><h3>Simplest solution first</h3><span class="tag tag-doc">DOC</span></div>
</body></html>
HTML
  cat > "$TEMPLATE_DIR/chorus-hook-architecture.html" <<'HTML'
<!DOCTYPE html><html><body>
<div class="date">PLACEHOLDER</div>
<p>0 hook modules</p>
<p>0 modules actively enforcing</p>
<table><tr><td>tdd_gate</td></tr><tr><td>memory_gate</td></tr></table>
</body></html>
HTML
}

teardown() {
  rm -rf "$OUTPUT_DIR" "$TEMPLATE_DIR" "$METRICS_FILE"
}

run_gen() {
  run bash "$GEN_SCRIPT" \
    --output-dir "$OUTPUT_DIR" \
    --template-dir "$TEMPLATE_DIR" \
    --metrics-file "$METRICS_FILE"
}

# --- AC 2: Per-standard tags derived from actual execution ---

@test "standards with active hook enforcement show GATE tag" {
  run_gen
  [ "$status" -eq 0 ]
  grep -A2 "TDD Discipline" "$OUTPUT_DIR/chorus-standards.html" | grep -q "tag-gate"
}

@test "standards without hook enforcement show DOC tag" {
  run_gen
  [ "$status" -eq 0 ]
  grep -A2 "Simplest solution first" "$OUTPUT_DIR/chorus-standards.html" | grep -q "tag-doc"
}

# --- AC 4: Tooltip/detail shows deny count per gate ---

@test "enforced standards show deny count in tooltip" {
  run_gen
  [ "$status" -eq 0 ]
  # GATE-tagged standards should have title="module: N denies (7d)" on parent div
  grep -B1 "TDD Discipline" "$OUTPUT_DIR/chorus-standards.html" | grep -qE 'title=.*denies'
}

@test "hook architecture page shows per-module deny counts" {
  run_gen
  [ "$status" -eq 0 ]
  grep -q "denies" "$OUTPUT_DIR/chorus-hook-architecture.html"
}

@test "hook architecture header shows enforcement summary" {
  run_gen
  [ "$status" -eq 0 ]
  grep -q "modules actively enforcing" "$OUTPUT_DIR/chorus-hook-architecture.html"
}

@test "hook table rows have deny count tooltips" {
  run_gen
  [ "$status" -eq 0 ]
  # At least one table cell should have a title with deny count
  grep -qE 'td title="[0-9]+ denies' "$OUTPUT_DIR/chorus-hook-architecture.html"
}
