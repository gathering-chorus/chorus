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
  bad=$(grep -rlE '/Users/[A-Za-z0-9._-]+/' platform/tests/ 2>/dev/null \
          | grep -v 'features/step_definitions/' \
          | grep -v 'node_modules/' \
          | grep -v 'hardcoded-path-guard.bats' || true)
  if [ -n "$bad" ]; then
    echo "Hardcoded absolute local paths found in:"
    echo "$bad" | sed 's/^/  - /'
    echo "Fix: 'load test_helper' then use \$CHORUS_ROOT/... instead of the absolute path."
    false
  fi
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
