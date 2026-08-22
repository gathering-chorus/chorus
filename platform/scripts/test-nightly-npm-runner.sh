#!/usr/bin/env bash
# test-nightly-npm-runner.sh — #3974 REDIRECT GUARD (was: #3606 own-runner tests
# against the retired run_one_attempt walker).
#
# The invariant lives in the runner now: werk-test's npm lane runs jest where
# jest exists and the package's OWN `npm test` (mcp-server: node:test) where it
# does not — and a package with NEITHER is a loud fail, never a vacuous green
# (the #3606 blank-summary class). This suite guards that the invariant exists
# where it moved to, and that the old vacuous-green branch cannot return.

set -uo pipefail
trap '_rc=$?; if [ $_rc -eq 0 ]; then echo "=== Results: $PASS passed, 0 failed ==="; else echo "=== Results: $PASS passed, $FAIL failed ==="; fi' EXIT

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
MAIN="$SCRIPT_DIR/../services/werk-test/src/main.rs"
PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); }
bad()  { FAIL=$((FAIL+1)); echo "FAIL: $1"; }

# 1. run_jest falls back to the package's own runner when jest is absent.
grep -q 'run_npm_test(werk, pkg)' "$MAIN" && ok || bad "run_jest must fall back to run_npm_test"

# 2. The no-jest branch no longer returns a vacuous green.
sed -n '/fn run_jest(/,/^}/p' "$MAIN" | grep -q 'return (true, Vec::new());' \
  && bad "the vacuous-green no-jest branch is back in run_jest" || ok

# 3. A package with neither jest nor a test script FAILS LOUD.
sed -n '/fn run_npm_test(/,/^}/p' "$MAIN" | grep -q 'FAIL LOUD' && ok || bad "missing-runner must fail loud"

# 4. Per-case results flow from the package runner's TAP output.
sed -n '/fn run_npm_test(/,/^}/p' "$MAIN" | grep -q 'parse_bats_cases' && ok || bad "own-runner output must yield per-case results"

# 5. NEGATIVE PROOF (#3734): check 2's grep DOES fire on the old shape.
TMP=$(mktemp -d)
printf 'fn run_jest(w: &str) {\n    if missing {\n        return (true, Vec::new());\n    }\n}\n' > "$TMP/old.rs"
sed -n '/fn run_jest(/,/^}/p' "$TMP/old.rs" | grep -q 'return (true, Vec::new());' \
  && ok || bad "guard cannot see the vacuous-green shape it exists to block"
rm -rf "$TMP"

[ "$FAIL" -eq 0 ] && exit 0 || exit 1
