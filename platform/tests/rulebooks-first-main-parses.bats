#!/usr/bin/env bats
# @test-type: unit — main's demo still reads the three gate skills once they carry fail-items
# @domain: pipelines
# @card: #4894 · owner: silas
# Main's werk-demo (platform/services/werk-demo/src/lib.rs here) feeds each skill whole as the
# system prompt and puts its own {result, findings} contract after it. So each skill must
# open with its frontmatter (name, fail-items) and give no output format of its own that
# would contradict that contract. Reads skills/gate-arch, gate-code and gate-quality SKILL.md.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

# prints why a skill would not read cleanly under main's contract, nothing when it would
skill_problems() {
  head -1 "$1" | grep -qx -- '---' || echo "no frontmatter"
  sed -n '2,/^---$/p' "$1" | grep -qE '^name: gate-' || echo "no name"
  sed -n '2,/^---$/p' "$1" | grep -qE '^fail-items: *[0-9]' || echo "no fail-items"
  grep -qE 'Output ONLY|"result" *:|\{"item"' "$1" && echo "gives its own output format"
  return 0
}

@test "main's demo still appends its own output contract after the skill" {
  grep -q 'Output ONLY a single JSON object' "$ROOT/platform/services/werk-demo/src/lib.rs" || return 1
}

@test "each gate skill opens with its frontmatter and gives no output format of its own" {
  for g in arch code quality; do
    out="$(skill_problems "$ROOT/skills/gate-$g/SKILL.md")"
    [ -z "$out" ] || { echo "gate-$g: $out"; return 1; }
  done
}

@test "negative proof: no fail-items, or a format of its own, is caught" {
  f="$BATS_TEST_TMPDIR/SKILL.md"
  printf -- '---\nname: gate-x\n---\nbody\n' > "$f"
  [[ "$(skill_problems "$f")" == *"no fail-items"* ]] || return 1
  printf -- '---\nname: gate-x\nfail-items: 1\n---\nEach finding: `{"item": 1}`\n' > "$f"
  [[ "$(skill_problems "$f")" == *"own output format"* ]] || return 1
}
