#!/usr/bin/env bats
# @test-type: unit — the code review checks that each AC's check tests that AC
# @domain: pipelines
# @card: #4889 · owner: silas
# Reads skills/gate-code/SKILL.md, the skill the demo's code step runs.
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"

# ok when a code-review skill tells the reviewer to check each AC's check against its AC,
# as a numbered item that fails the review (it is in the skill's fail-items)
checks_the_checks() {
  grep -qiE "^1\. \*\*Does each AC's check test that AC\?" "$1" && grep -qE '^fail-items: *1' "$1"
}

@test "AC8: the code review is told to check each AC's check against its AC, and a miss fails it" {
  checks_the_checks "$ROOT/skills/gate-code/SKILL.md" || { head -12 "$ROOT/skills/gate-code/SKILL.md"; return 1; }
}

@test "negative proof: a skill without the item, or with it as a note only, is caught" {
  f="$BATS_TEST_TMPDIR/SKILL.md"
  printf -- '---\nfail-items: 1-2\n---\n1. **Is the code correct?**\n' > "$f"
  ! checks_the_checks "$f" || return 1
  printf -- "---\nfail-items: 2\n---\n1. **Does each AC's check test that AC?**\n" > "$f"
  ! checks_the_checks "$f" || return 1
}
