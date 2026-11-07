#!/usr/bin/env bats
# @test-type: unit — each proof step calls the skill that already exists; no copies, no new names
# @domain: pipelines
# @card: #4889 · owner: silas
# Reads platform/services/werk-demo/src/lib.rs and skills/gate-*/SKILL.md.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
SRC="$ROOT/platform/services/werk-demo/src/lib.rs"

# prints why a werk-demo source and skills dir break AC7, nothing when they hold
copy_problems() {
  grep -q 'pub fn review_system_prompt' "$1" && echo "werk-demo still writes its own review prompt"
  for g in arch quality code; do [ -f "$2/gate-$g/SKILL.md" ] || echo "no skills/gate-$g/SKILL.md"; done
  return 0
}

@test "AC7: the demo's own review prompt is gone; reviews come from the skills" {
  out="$(copy_problems "$SRC" "$ROOT/skills")"
  [ -z "$out" ] || { echo "$out"; return 1; }
}

@test "negative proof: a werk-demo that still defines its own prompt, or a missing skill, is caught" {
  d="$BATS_TEST_TMPDIR"; mkdir -p "$d/skills/gate-arch" "$d/skills/gate-code"
  touch "$d/skills/gate-arch/SKILL.md" "$d/skills/gate-code/SKILL.md"
  echo 'pub fn review_system_prompt() -> String { String::new() }' > "$d/lib.rs"
  out="$(copy_problems "$d/lib.rs" "$d/skills")"
  [[ "$out" == *"own review prompt"* && "$out" == *"gate-quality"* ]] || { echo "$out"; return 1; }
}
