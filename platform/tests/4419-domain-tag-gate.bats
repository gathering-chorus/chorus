#!/usr/bin/env bats
# @test-type: unit — a fixture git repo and the werk's crawler; no live service
# @domain: tests
# @card: 4419 · owner: kade
# What Jeff sees: a new file with no domain cannot be committed, so a card that
# touches it can always find its tests by domain.

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
GATE="$REPO/platform/scripts/gate-domain-tag.sh"
CRAWL="$REPO/platform/services/chorus-crawl/target/release/chorus-crawl"

setup() {
  [ -x "$CRAWL" ] || skip "chorus-crawl not built at $CRAWL"
  T="$(mktemp -d)"; git -C "$T" init -q .
  export DOMAIN_TAG_CRAWL="$CRAWL" CHORUS_VALID_DOMAINS="tests,messages,code"
}
teardown() { rm -rf "$T"; }

@test "NEGATIVE PROOF — a new untagged file is refused and named" {
  mkdir -p "$T/lib"; printf 'export const x = 1;\n' > "$T/lib/orphan.ts"
  git -C "$T" add lib/orphan.ts
  run bash -c "cd '$T' && '$GATE' staged"
  [ "$status" -eq 1 ] || return 1
  [[ "$output" == *"lib/orphan.ts"* ]] || return 1
}

@test "the same file with an @domain header passes" {
  mkdir -p "$T/lib"; printf '// @domain: messages\nexport const x = 1;\n' > "$T/lib/tagged.ts"
  git -C "$T" add lib/tagged.ts
  run bash -c "cd '$T' && '$GATE' staged"
  [ "$status" -eq 0 ] || return 1
}

@test "only ADDED code files are checked — docs and edits to old files pass" {
  printf '# notes\n' > "$T/README.md"
  git -C "$T" add README.md
  run bash -c "cd '$T' && '$GATE' staged"
  [ "$status" -eq 0 ] || return 1
}

@test "with no crawler that answers, the gate says so and lets the commit through" {
  mkdir -p "$T/lib"; printf 'export const x = 1;\n' > "$T/lib/orphan.ts"
  git -C "$T" add lib/orphan.ts
  run bash -c "cd '$T' && DOMAIN_TAG_CRAWL=/usr/bin/false PATH=/usr/bin:/bin '$GATE' staged"
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"not checked"* ]] || return 1
}

@test "the pre-commit hook runs the gate" {
  grep -q 'gate-domain-tag.sh" staged' "$REPO/platform/hooks/pre-commit" || return 1
}
