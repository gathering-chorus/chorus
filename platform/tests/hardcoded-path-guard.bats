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
  # exclude this guard itself (it names the forbidden pattern in its docs)
  bad=$(grep -rlE '/Users/[A-Za-z0-9._-]+/' platform/tests/ | grep -v 'features/step_definitions/' 2>/dev/null \
          | grep -v 'hardcoded-path-guard.bats' || true)
  if [ -n "$bad" ]; then
    echo "Hardcoded absolute local paths found in:"
    echo "$bad" | sed 's/^/  - /'
    echo "Fix: 'load test_helper' then use \$CHORUS_ROOT/... instead of the absolute path."
    false
  fi
}
