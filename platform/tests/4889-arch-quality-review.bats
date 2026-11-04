#!/usr/bin/env bats
# @test-type: unit — the architecture and quality review reads the diff, on every card, never the AC boxes
# @domain: pipelines
# @card: #4889 · owner: silas
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

@test "AC5: /gate-arch and /gate-quality review the diff on every card, and neither reads AC boxes" {
  for g in arch quality; do
    s="$ROOT/skills/gate-$g/SKILL.md"
    ! grep -qiE 'type:fix.*SKIP|SKIP.*fix cards|not applicable for bug fix' "$s" || { echo "$g skips fix cards"; return 1; }
    ! grep -qiE 'AC (items|boxes) (checked|ticked)|all AC|\[x\]' "$s" || { echo "$g reads AC boxes"; return 1; }
    grep -qi 'diff' "$s" || { echo "$g does not read the diff"; return 1; }
  done
}
