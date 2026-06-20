#!/usr/bin/env bats
# @test-type: unit — static source/shape guard, hermetic
# pulse-rolestate-retired.bats — #2632
#
# Asserts that the pulse role-state HTTP endpoint and its store wrapper
# have been retired. Background: surfaced by wren in #2629 PM-lens probe
# (2026-04-30) — pulse had a parallel writer for role-state with its own
# SQLite table, but grep across the codebase found ZERO callers. The
# four-surface defense in #2629 named the duplication; this card retires
# the dead path per eliminate-vs-manage + single-implementation invariant.

# Default to the repo root the test file lives in (works in any worktree)
CHORUS_ROOT="${CHORUS_ROOT:-$(cd "${BATS_TEST_DIRNAME}/../.." && pwd)}"
PULSE_DIR="$CHORUS_ROOT/platform/pulse"

@test "no setRoleState references remain in pulse production code" {
  matches=$(grep -rn "setRoleState" "$PULSE_DIR/src" 2>/dev/null \
    | grep -v -E '\.test\.|^\s*//|^\s*\*|retired|deprecated|removed|#2632' \
    || true)
  if [ -n "$matches" ]; then
    echo "Found setRoleState references in pulse production code:"
    echo "$matches"
    false
  fi
}

@test "no getRoleState references remain in pulse production code" {
  matches=$(grep -rn "getRoleState" "$PULSE_DIR/src" 2>/dev/null \
    | grep -v -E '\.test\.|^\s*//|^\s*\*|retired|deprecated|removed|#2632' \
    || true)
  if [ -n "$matches" ]; then
    echo "Found getRoleState references in pulse production code:"
    echo "$matches"
    false
  fi
}

@test "no /api/role-state route definitions in pulse service" {
  matches=$(grep -n "/api/role-state" "$PULSE_DIR/src/service.ts" 2>/dev/null \
    | grep -v -E '^\s*//|deprecated|removed|retired|#2632' \
    || true)
  if [ -n "$matches" ]; then
    echo "Found /api/role-state route refs in pulse service:"
    echo "$matches"
    false
  fi
}

@test "no role_state table CREATE in pulse schema" {
  matches=$(grep -n "CREATE TABLE.*role_state\|CREATE TABLE IF NOT EXISTS role_state" "$PULSE_DIR/src/store.ts" 2>/dev/null \
    | grep -v -E '^\s*//|deprecated|removed|retired|#2632' \
    || true)
  if [ -n "$matches" ]; then
    echo "Found role_state CREATE TABLE in pulse store:"
    echo "$matches"
    false
  fi
}

@test "no Role state describe block in pulse store tests" {
  matches=$(grep -n "describe.*[Rr]ole [Ss]tate" "$PULSE_DIR/src/store.test.ts" 2>/dev/null \
    || true)
  if [ -n "$matches" ]; then
    echo "Found 'Role state' describe block in store.test.ts (#2632 retired):"
    echo "$matches"
    false
  fi
}
