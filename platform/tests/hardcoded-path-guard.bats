#!/usr/bin/env bats
# @test-type: unit — static suite-wide grep for hardcoded local paths; no external deps.
# #3528 — REGRESSION GUARD. No test file may hardcode an absolute local path
# (/Users/<name>/...). Such a path is green on the author's machine and silent-red
# on the CI runner — the "works on my machine" rot that kept quality.yml red and
# dismissed for 10 days (RCA 2026-06-20: 100% test-rot, zero product regressions).
# Roots must derive RELATIVELY via `load test_helper` → $CHORUS_ROOT. This guard
# FAILS if any hardcoded local path reappears, so the rot cannot return — the thing
# that makes the sweep STICK (Wren's AC ask).

setup() {
  REPO_ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
}

@test "no test file hardcodes an absolute /Users/<name>/ path (use \$CHORUS_ROOT)" {
  cd "$REPO_ROOT"
  # exclude this guard itself (it names the forbidden pattern in its docs) and
  # node_modules (#3665 — vendored readmes/typings legitimately mention /Users/
  # paths; the guard governs OUR test files, not dependency docs. This false-fired
  # the moment platform/tests grew a node_modules for the cucumber tier.)
  # #4004 — match a REAL account's home, not the shape of one. The rot this
  # guard exists to stop is "green on the author's machine": a path under a home
  # that actually exists here. A synthetic placeholder like /Users/x/ inside a
  # fixture (Kade's #3989 bats embeds fake `ps` output whose command lines are
  # absolute by nature) belongs to no account, cannot be machine-specific, and
  # dereferences to nothing — flagging it taught the team to edit fixture data to
  # appease a guard, which is how a guard loses its meaning.
  local homes; homes=$(ls /Users 2>/dev/null | grep -vE '^(Shared|Guest)$' | paste -sd'|' -)
  [ -n "$homes" ] || homes="$(basename "$HOME")"
  bad=$(grep -rlE "/Users/($homes)/" platform/tests/ 2>/dev/null \
          | grep -v 'features/step_definitions/' \
          | grep -v 'node_modules/' \
          | grep -v 'fixtures/' \
          | grep -v 'hardcoded-path-guard.bats' || true)
  # fixtures/ exempted (#3904): harvested-evidence snapshots (launchctl JSON)
  # legitimately CONTAIN real paths — the guard governs test CODE, not captured
  # data. Test code keeps deriving roots from $CHORUS_ROOT/BATS_TEST_DIRNAME.
  if [ -n "$bad" ]; then
    echo "Hardcoded absolute local paths found in:"
    echo "$bad" | sed 's/^/  - /'
    echo "Fix: 'load test_helper' then use \$CHORUS_ROOT/... instead of the absolute path."
    false
  fi
}

# #4004 NEGATIVE PROOF (#3734) — narrowing the pattern to real accounts is only
# safe if the guard still REDS on the state it exists to catch. These two drive
# the matcher directly over a scratch tree: a real home must be caught, and the
# synthetic placeholder must not be. Without the first, the narrowing could have
# silently disarmed the guard and every check would still have been green.
@test "the matcher CATCHES a real local home (the rot this guard exists to stop)" {
  local homes; homes=$(ls /Users 2>/dev/null | grep -vE '^(Shared|Guest)$' | paste -sd'|' -)
  [ -n "$homes" ] || homes="$(basename "$HOME")"
  local scratch="$BATS_TEST_TMPDIR/real"
  mkdir -p "$scratch"
  printf 'load "%s/CascadeProjects/chorus/platform/tests/test_helper"\n' "$HOME" > "$scratch/offender.bats"
  run grep -rlE "/Users/($homes)/" "$scratch"
  [ "$status" -eq 0 ]
  [[ "$output" == *"offender.bats"* ]]
}

@test "the matcher IGNORES a synthetic placeholder home inside fixture data" {
  local homes; homes=$(ls /Users 2>/dev/null | grep -vE '^(Shared|Guest)$' | paste -sd'|' -)
  [ -n "$homes" ] || homes="$(basename "$HOME")"
  local scratch="$BATS_TEST_TMPDIR/fake"
  mkdir -p "$scratch"
  # the exact shape of Kade's #3989 fixture: fake `ps` output, absolute by nature
  echo '99901 1 05:33 node /Users/x/CascadeProjects/chorus/src/cli.ts add foo' > "$scratch/fixture.bats"
  run grep -rlE "/Users/($homes)/" "$scratch"
  [ "$status" -ne 0 ]
}

# #3528 — the /Users/ pattern above MISSES the werk-path form that actually bit us:
# athena-tree.test.ts hardcoded os.homedir()+'chorus-werk/wren-2940', not /Users/...
# Green on the author's werk, silent-red on CI. Extends the guard to TS/jest tests.
@test "no TS/jest test hardcodes a chorus-werk/<role>-<card> werk path" {
  cd "$REPO_ROOT"
  bad=$(grep -rlE 'chorus-werk/[a-z]+-[0-9]+' --include='*.test.ts' --include='*.spec.ts' platform directing 2>/dev/null | grep -v node_modules || true)
  if [ -n "$bad" ]; then
    echo "Hardcoded werk paths in TS tests:"
    echo "$bad" | sed 's/^/  - /'
    echo "Fix: path.resolve(__dirname,'../../..') or \$CHORUS_ROOT."
    false
  fi
}
