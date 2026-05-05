#!/usr/bin/env bash
# test-hardcoded-bin-paths.sh — regression guard for #2734.
#
# After ~/.chorus/bin/ becomes the canonical deploy location, operational
# wrappers (scripts, hooks, skills) should resolve chorus-* binaries via
# PATH — not hardcode target/release/ paths. Test files are exempt: they
# legitimately test specific build outputs.
#
# This test enforces a baseline: count current hardcoded references in
# non-test files, fail if the count grows. Migrating the existing
# references can happen incrementally; preventing regression is the
# load-bearing part.
#
# Run directly (not via Claude hook-intercepted Bash).

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"

# Files that legitimately reference target/release (tests, build scripts,
# tarpaulin coverage config, the install primitive itself).
EXCLUDE_PATTERN='target/|node_modules/|\.consumed$|chorus\.log$|\.pre-rewrite|/tests/|test-|_test\.|\.test\.|\.bats$|tarpaulin\.toml|build-signed\.sh|chorus-bin-install$|test-hardcoded-bin-paths\.sh'

count_hardcoded() {
  cd "$REPO_ROOT"
  grep -rln "target/release/chorus-hook-shim\|target/release/chorus-inject\|target/release/chorus-hooks" 2>/dev/null \
    | grep -vE "$EXCLUDE_PATTERN" \
    | wc -l \
    | tr -d ' '
}

# Baseline — recorded the day this card lands (#2734). Migration of the
# 27 existing callers is incremental — any DECREASE is fine and the test
# tells you to lower the baseline when count drops. Any INCREASE fails:
# new hardcoded paths must not creep in. As callers migrate to PATH
# resolution, they often keep target/release/ as a fallback, which still
# counts (the file matches the grep). Lower the baseline only when a file
# stops referencing target/release/ entirely.
BASELINE=27

current=$(count_hardcoded)

if [ "$current" -gt "$BASELINE" ]; then
  echo "FAIL: hardcoded target/release/ chorus-* paths in non-test files: $current (baseline: $BASELINE)"
  echo "New offenders:"
  cd "$REPO_ROOT"
  grep -rln "target/release/chorus-hook-shim\|target/release/chorus-inject\|target/release/chorus-hooks" 2>/dev/null \
    | grep -vE "$EXCLUDE_PATTERN" \
    | head -30
  exit 1
fi

if [ "$current" -lt "$BASELINE" ]; then
  echo "PASS: $current hardcoded paths (baseline was $BASELINE — drop the baseline to $current)"
  exit 0
fi

echo "PASS: $current hardcoded paths (baseline $BASELINE, no regression)"
exit 0
