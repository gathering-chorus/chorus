#!/usr/bin/env bats
# @test-type: unit — each proof step calls the skill that already exists; no copies, no new names
# @domain: pipelines
# @card: #4889 · owner: silas
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
SRC="$ROOT/platform/services/werk-demo/src/lib.rs"

@test "AC7: the demo's own review prompt is gone; reviews come from the skills" {
  ! grep -q 'pub fn review_system_prompt' "$SRC" || { echo "werk-demo still writes its own review prompt"; return 1; }
  [ -f "$ROOT/skills/gate-arch/SKILL.md" ] && [ -f "$ROOT/skills/gate-quality/SKILL.md" ] || return 1
}
