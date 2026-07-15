#!/usr/bin/env bash
# test-cross-product-coupling.sh — regression guard for #3612 (GOVERN, #3564 AC3).
#
# Jeff's invariants (#3564, 2026-06-22): be EXTRA careful about coupling (4/5);
# gathering is a product with NO runtime dependency on chorus, and the #3564
# program must REDUCE chorus↔gathering coupling and add NONE (6). The reverse
# coupling that exists today (chorus platform code referencing gathering's
# runtime surface — invariant 7, the UNTANGLE backlog) is frozen as a baseline;
# this guard fails CI when a commit ADDS a new reference.
#
# Markers of chorus→gathering runtime coupling in platform/ code:
#   - jeff-bridwell-personal-site   (gathering's repo/filesystem path)
#   - localhost:3000 / 127.0.0.1:3000  (gathering app — browser/session surface,
#     never a programmatic dependency, DEC-093)
#   - com.gathering.*               (gathering's launchd services)
#
# Same shape as test-hardcoded-bin-paths.sh (#2734): git grep (tracked files
# only, #3556 — hermetic across machines), non-test files, baseline ratchet.
# Migrating the existing referrers is incremental UNTANGLE work; preventing
# regression is the load-bearing part.

set -uo pipefail

# #2856 — canonical results line on EXIT for the nightly-suites.sh consumer.
trap '_rc=$?; if [ $_rc -eq 0 ]; then echo "=== Results: 1 passed, 0 failed ==="; else echo "=== Results: 0 passed, 1 failed ==="; fi' EXIT

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"

MARKERS='jeff-bridwell-personal-site|localhost:3000[^0-9]|127\.0\.0\.1:3000[^0-9]|com\.gathering\.'
# Tests may legitimately mention gathering (realm-isolation tests mint
# gathering-realm tokens to prove they're REFUSED). Docs describe the boundary.
# platform/logs/ are runtime artifacts, not code.
EXCLUDE_PATTERN='target/|node_modules/|/tests/|test-|_test\.|\.test\.|\.bats$|\.md$|platform/logs/|test-cross-product-coupling\.sh'

count_coupling() {
  cd "$REPO_ROOT"
  git grep -lE "$MARKERS" -- 'platform/' 2>/dev/null \
    | grep -vE "$EXCLUDE_PATTERN" \
    | wc -l \
    | tr -d ' '
}

# Baseline — recorded when #3612 landed the guard (2026-07-15). These are the
# invariant-7 referrers the UNTANGLE program burns down. Any DECREASE is
# progress (lower the baseline). Any INCREASE fails: new chorus→gathering
# coupling must not creep in (#3564 invariant: reduce coupling, add NONE).
BASELINE=103

current=$(count_coupling)

if [ "$current" -gt "$BASELINE" ]; then
  echo "FAIL: chorus→gathering coupling references in platform/ non-test files: $current (baseline: $BASELINE)"
  echo "The #3564 program must add NO new cross-product coupling. New offenders:"
  cd "$REPO_ROOT"
  git grep -lE "$MARKERS" -- 'platform/' 2>/dev/null \
    | grep -vE "$EXCLUDE_PATTERN" \
    | head -30
  exit 1
fi

if [ "$current" -lt "$BASELINE" ]; then
  echo "PASS: $current coupling references (baseline was $BASELINE — drop the baseline to $current)"
  exit 0
fi

echo "PASS: $current coupling references (baseline $BASELINE, no regression)"
exit 0
