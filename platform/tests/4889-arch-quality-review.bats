#!/usr/bin/env bats
# @test-type: unit — the architecture and quality review reads the diff, on every card, never the AC boxes
# @domain: pipelines
# @card: #4889 · owner: silas
# Reads skills/gate-arch/SKILL.md and skills/gate-quality/SKILL.md.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

# prints why a review skill file breaks AC5, nothing when it holds
skill_problems() {
  grep -qiE 'type:fix.*SKIP|SKIP.*fix cards|not applicable for bug fix' "$1" && echo "skips fix cards"
  grep -qiE 'AC (items|boxes) (checked|ticked)|all AC|\[x\]' "$1" && echo "reads AC boxes"
  grep -qi 'diff' "$1" || echo "does not read the diff"
  return 0
}

@test "AC5: /gate-arch and /gate-quality review the diff on every card, and neither reads AC boxes" {
  for g in arch quality; do
    out="$(skill_problems "$ROOT/skills/gate-$g/SKILL.md")"
    [ -z "$out" ] || { echo "gate-$g: $out"; return 1; }
  done
}

@test "negative proof: a skill that skips fix cards, counts AC boxes, or never reads the diff is caught" {
  f="$BATS_TEST_TMPDIR/SKILL.md"
  printf 'Reviews the diff.\ntype:fix cards: SKIP this gate\n' > "$f"
  [[ "$(skill_problems "$f")" == *"skips fix cards"* ]] || return 1
  printf 'Reviews the diff.\nPass when all AC are ticked.\n' > "$f"
  [[ "$(skill_problems "$f")" == *"reads AC boxes"* ]] || return 1
  printf 'Reviews the card text only.\n' > "$f"
  [[ "$(skill_problems "$f")" == *"does not read the diff"* ]] || return 1
}
