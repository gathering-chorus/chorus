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
# Discovery semantics (post-#2801): yield package dirs that own *.test.{ts,js}
# files via nearest-package-json walk AND have a real jest setup
# (scripts.test, `jest` key in package.json, or jest.config.*). Pre-#2142
# this gated on scripts.test only; #2142 dropped the gate; #2801 added the
# jest-setup requirement to keep the chorus root and other no-config dirs
# from getting yielded and producing nonsense "82 skipped" runs.
contains "$NPM" "${APP_ROOT}"                             && p "finds app root"                  || f "npm missing app root; got: $NPM"
contains "$NPM" "${CHORUS_ROOT}/directing/clearing"       && p "finds clearing"                  || f "npm missing clearing; got: $NPM"
contains "$NPM" "${CHORUS_ROOT}/directing/products/cards" && p "finds cards"                     || f "npm missing cards"
contains "$NPM" "${CHORUS_ROOT}/platform/api"             && p "finds platform/api"              || f "npm missing platform/api (post-#2142 should be included)"
contains "$NPM" "${CHORUS_ROOT}/platform/pulse"           && p "finds platform/pulse"            || f "npm missing platform/pulse (post-#2142 should be included)"
contains "$NPM" "${CHORUS_ROOT}/platform/workflow-engine" && p "finds workflow-engine"           || f "npm missing workflow-engine"
contains "$NPM" "${CHORUS_ROOT}/platform/chorus-sdk"      && p "finds chorus-sdk"                || f "npm missing chorus-sdk"

# Must exclude node_modules/ — and the chorus root itself, which has no
# jest setup and would silently swallow 260+ sub-package suites with
# transform errors (#2801 receipt: 82 skipped / 82 total nonsense line).
if echo "$NPM" | grep -q "node_modules"; then f "npm should skip node_modules; got: $NPM"; else p "skips node_modules"; fi
if contains "$NPM" "${CHORUS_ROOT}"; then f "npm should skip chorus root (#2801 — no jest setup); got: $NPM"; else p "skips chorus root (no jest setup)"; fi

# Note: platform/tests has scripts.test=cucumber-js + .feature files (BDD),
# not .test.ts. Current list_npm walks up from .test.ts only, so cucumber
# surfaces aren't discovered. Adding a list_cucumber tier is a separate
# enhancement card, not in #2801's scope (zero observed cucumber failures
# today; whole tier is silent).

echo "--- cargo discovery ---"
CARGO=$(bash "$SCRIPT" --list-cargo 2>&1)
contains "$CARGO" "${CHORUS_ROOT}/platform/services/chorus-hooks"  && p "finds chorus-hooks"  || f "cargo missing chorus-hooks; got: $CARGO"
contains "$CARGO" "${CHORUS_ROOT}/platform/services/chorus-inject" && p "finds chorus-inject" || f "cargo missing chorus-inject"
if echo "$CARGO" | grep -q "target/"; then f "cargo should skip target/; got: $CARGO"; else p "skips target/"; fi

echo "--- shell discovery ---"
SHELL_T=$(bash "$SCRIPT" --list-shell 2>&1)
# test-gate-route.sh was deleted; test-skip-gates.sh is the current gate-route check.
contains "$SHELL_T" "${CHORUS_ROOT}/platform/scripts/test-skip-gates.sh"  && p "finds test-skip-gates"  || f "shell missing test-skip-gates"
contains "$SHELL_T" "${CHORUS_ROOT}/platform/scripts/test-daily-review.sh" && p "finds test-daily-review" || f "shell missing test-daily-review"

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
