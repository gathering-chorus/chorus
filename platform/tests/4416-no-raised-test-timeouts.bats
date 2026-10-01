#!/usr/bin/env bats
# @test-type: fitness — static repo scan, no live service
# @domain: cicd
# @card: 4416 · owner: kade
# 4416-no-raised-test-timeouts.bats — what Jeff sees: a slow endpoint shows up
# as a red test that names it, never as a longer nightly. A jest timeout above
# 5 s is refused unless the line (or the one above) says
# `// slow-by-design: <call> measured <N> s`. The ones still in the tree are
# baselined and may only shrink.

REPO="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
SCAN="$REPO/platform/scripts/raised-test-timeouts"
BASELINE="$REPO/platform/tests/4416-raised-test-timeouts-baseline.txt"

setup() { T="$(mktemp -d)"; }
teardown() { rm -rf "$T"; }

# `path:line: ms ms` → `path ms`, sorted, so line moves don't count as new.
normalise() { sed -E 's/:[0-9]+: ([0-9]+) ms$/ \1/' | sort; }

@test "no raised timeout in the tree beyond the baseline" {
  cd "$REPO"
  run "$SCAN" platform directing
  new="$(printf '%s\n' "$output" | grep -v '^$' | normalise | comm -23 - <(grep -v '^#' "$BASELINE" | sort))"
  if [ -n "$new" ]; then
    echo "raised timeouts with no slow-by-design reason (not in the baseline):"
    echo "$new"
    return 1
  fi
}

@test "NEGATIVE PROOF — a new test closing with }, 20000) and no reason is refused" {
  mkdir -p "$T/tests"
  printf "test('slow', async () => {\n  await fetch('x');\n}, 20000);\n" > "$T/tests/a.test.ts"
  run "$SCAN" "$T"
  [ "$status" -eq 1 ]
  [[ "$output" == *"a.test.ts:3: 20000 ms"* ]] || return 1
}

@test "NEGATIVE PROOF — jest.setTimeout and an inline hook timeout are refused" {
  mkdir -p "$T/tests"
  printf "jest.setTimeout(30000);\nafterAll(async () => { await close(); }, 15000);\n" > "$T/tests/b.test.ts"
  run "$SCAN" "$T"
  [ "$status" -eq 1 ]
  [[ "$output" == *"b.test.ts:1: 30000 ms"* ]] || return 1
  [[ "$output" == *"b.test.ts:2: 15000 ms"* ]] || return 1
}

@test "a raised timeout with a slow-by-design reason passes; 5 s and under passes" {
  mkdir -p "$T/tests"
  printf "test('ollama', async () => {\n  await embed();\n  // slow-by-design: ollama embed measured 8 s\n}, 12000);\ntest('ok', async () => {\n}, 5000);\n" > "$T/tests/c.test.ts"
  run "$SCAN" "$T"
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

@test "the card-tagged reason form, up to three lines above, passes" {
  mkdir -p "$T/tests"
  printf "// slow-by-design (#4417): runs only against the real board,\n// and pages through every card.\njest.setTimeout(30000);\n" > "$T/tests/e.test.ts"
  run "$SCAN" "$T"
  [ "$status" -eq 0 ] || return 1
}

@test "a call that is not a test timeout is not counted" {
  mkdir -p "$T/tests"
  printf "const r = enrichHit({ content: 'x' }, 1_000_000);\n" > "$T/tests/d.test.ts"
  run "$SCAN" "$T"
  [ "$status" -eq 0 ]
}

@test "the scan fails loudly when its roots are gone, never passes empty" {
  run "$SCAN" "$T/does-not-exist"
  [ "$status" -ne 0 ]
}
