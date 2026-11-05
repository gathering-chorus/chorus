#!/usr/bin/env bats
# @test-type: unit — this card's diff is the five rulebook files, byte-identical to #4889's
# @domain: pipelines
# @card: #4894 · owner: silas
# What Jeff sees: #4889's reviews read rules from main, so the rules land first, unchanged.
# The skill files make main's demo smoke every Claude role, which dagu's shared account
# cannot do, so this card also carries that fix (werk-demo src/lib.rs session_smoke_step:
# unmeasured off a chorus-<role> account, proven in tests/steps_4502.rs).
# The five: platform/quality/depcruise/boundaries.cjs, platform/quality/semgrep/quality.yml,
# skills/gate-arch/SKILL.md, skills/gate-code/SKILL.md, skills/gate-quality/SKILL.md
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
FILES="platform/quality/depcruise/boundaries.cjs platform/quality/semgrep/quality.yml skills/gate-arch/SKILL.md skills/gate-code/SKILL.md skills/gate-quality/SKILL.md"

# the ref holding #4889's copy, or nothing once #4889 has landed and its branch is gone
ref_4889() {
  for r in silas/4889 origin/silas/4889; do git -C "$ROOT" rev-parse -q --verify "$r" >/dev/null && { echo "$r"; return; }; done
}

# prints each file of $2 (a tree root) that differs from ref $1, nothing when all match
differs() {
  for f in $FILES; do
    cmp -s <(git -C "$2" show "$1:$f" 2>/dev/null) "$2/$f" || echo "$f"
  done
  return 0
}

@test "the five rulebook files are byte-identical to #4889's" {
  r="$(ref_4889)"
  [ -n "$r" ] || skip "unmeasured: #4889's branch is gone (it has landed)"
  out="$(differs "$r" "$ROOT")"
  [ -z "$out" ] || { echo "differ from $r: $out"; return 1; }
}

@test "this card changes nothing but the five files, their checks, and main's session-smoke fix" {
  git -C "$ROOT" rev-parse -q --verify origin/main >/dev/null || skip "unmeasured: no origin/main"
  extra="$(git -C "$ROOT" diff --name-only origin/main...HEAD | grep -vxF -f <(tr ' ' '\n' <<<"$FILES"; echo platform/tests/rulebooks-first-identical.bats; echo platform/tests/rulebooks-first-main-parses.bats; echo platform/services/werk-demo/src/lib.rs; echo platform/services/werk-demo/tests/steps_4502.rs) || true)"
  [ -z "$extra" ] || { echo "outside the rulebooks: $extra"; return 1; }
}

@test "negative proof: one changed byte is caught" {
  r="$(ref_4889)"
  [ -n "$r" ] || skip "unmeasured: #4889's branch is gone (it has landed)"
  d="$BATS_TEST_TMPDIR/t"; mkdir -p "$d"; git -C "$d" init -q
  for f in $FILES; do mkdir -p "$d/$(dirname "$f")"; cp "$ROOT/$f" "$d/$f"; done
  printf 'x' >> "$d/skills/gate-code/SKILL.md"
  ( cd "$d" && git fetch -q "$ROOT" "$r:refs/x" )
  out="$(differs refs/x "$d")"
  [ "$out" = "skills/gate-code/SKILL.md" ] || { echo "got: $out"; return 1; }
}
