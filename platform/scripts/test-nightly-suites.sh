#!/bin/bash
# #2142 — Tests for nightly-suites.sh discovery.
# nightly-suites.sh --list-{npm,cargo,shell} emits one suite path per line.
# Discovery must cover every suite that should be in the overnight backstop.

set -u
CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
APP_ROOT="${APP_ROOT:-/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site}"
SCRIPT="${CHORUS_ROOT}/platform/scripts/nightly-suites.sh"
PASS=0; FAIL=0

p() { PASS=$((PASS+1)); echo "✅ $*"; }
f() { FAIL=$((FAIL+1)); echo "❌ $*"; }

contains() {
  # contains <list> <needle> — match whole line
  echo "$1" | grep -qxF "$2"
}

echo "--- npm discovery ---"
NPM=$(bash "$SCRIPT" --list-npm 2>&1)
contains "$NPM" "${APP_ROOT}"                             && p "finds app root"                  || f "npm missing app root; got: $NPM"
contains "$NPM" "${CHORUS_ROOT}/directing/clearing"       && p "finds clearing"                  || f "npm missing clearing; got: $NPM"
contains "$NPM" "${CHORUS_ROOT}/directing/products/cards" && p "finds cards"                     || f "npm missing cards"
contains "$NPM" "${CHORUS_ROOT}/platform/tests"           && p "finds platform/tests"            || f "npm missing platform/tests"
contains "$NPM" "${CHORUS_ROOT}/platform/workflow-engine" && p "finds workflow-engine"           || f "npm missing workflow-engine"
contains "$NPM" "${CHORUS_ROOT}/platform/chorus-sdk"      && p "finds chorus-sdk"                || f "npm missing chorus-sdk"

# Must exclude node_modules/ and packages without scripts.test
if echo "$NPM" | grep -q "node_modules"; then f "npm should skip node_modules; got: $NPM"; else p "skips node_modules"; fi
if contains "$NPM" "${CHORUS_ROOT}/platform/api"; then f "npm should skip platform/api (no test script)"; else p "skips platform/api (no test script)"; fi
if contains "$NPM" "${CHORUS_ROOT}/platform/pulse"; then f "npm should skip platform/pulse (no test script)"; else p "skips platform/pulse (no test script)"; fi

echo "--- cargo discovery ---"
CARGO=$(bash "$SCRIPT" --list-cargo 2>&1)
contains "$CARGO" "${CHORUS_ROOT}/platform/services/chorus-hooks"  && p "finds chorus-hooks"  || f "cargo missing chorus-hooks; got: $CARGO"
contains "$CARGO" "${CHORUS_ROOT}/platform/services/chorus-inject" && p "finds chorus-inject" || f "cargo missing chorus-inject"
if echo "$CARGO" | grep -q "target/"; then f "cargo should skip target/; got: $CARGO"; else p "skips target/"; fi

echo "--- shell discovery ---"
SHELL_T=$(bash "$SCRIPT" --list-shell 2>&1)
contains "$SHELL_T" "${CHORUS_ROOT}/platform/scripts/test-gate-route.sh"  && p "finds test-gate-route"  || f "shell missing test-gate-route"
contains "$SHELL_T" "${CHORUS_ROOT}/platform/scripts/test-skip-gates.sh"  && p "finds test-skip-gates"  || f "shell missing test-skip-gates"
contains "$SHELL_T" "${CHORUS_ROOT}/platform/scripts/test-daily-review.sh" && p "finds test-daily-review" || f "shell missing test-daily-review"

echo ""
echo "=== $PASS pass / $FAIL fail ==="
[ "$FAIL" -eq 0 ]
