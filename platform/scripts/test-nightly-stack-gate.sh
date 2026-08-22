#!/usr/bin/env bash
# test-nightly-stack-gate.sh — #3974 RETIREMENT GUARD (was: #3557 allowlist tests).
#
# The filename allowlist this suite used to exercise is retired: hermeticity is
# DECLARED per test in the registry and `werk-test --nightly` derives typed
# skips from it (#3919). What remains to guard: the stubs fail LOUD (a
# resurrected caller is a red, never a silent wrong answer), _stack_up still
# serves the smoke tier, and the runner owns the api-integration gate (#3559).

set -uo pipefail
trap '_rc=$?; if [ $_rc -eq 0 ]; then echo "=== Results: $PASS passed, 0 failed ==="; else echo "=== Results: $PASS passed, $FAIL failed ==="; fi' EXIT

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); }
bad()  { FAIL=$((FAIL+1)); echo "FAIL: $1"; }

source "$SCRIPT_DIR/nightly-suites.sh"

# 1. The allowlist is GONE — no filename patterns left in _needs_stack.
grep -A4 '^_needs_stack()' "$SCRIPT_DIR/nightly-suites.sh" | grep -qE 'deep-health|alert-delivery|chorus-deploy' \
  && bad "the #3557 filename allowlist is back" || ok

# 2. The stubs refuse loudly, never answer the old question.
_needs_stack "/x/test-api-health.sh" 2>/dev/null && bad "_needs_stack answered — must be retired" || ok
(_needs_stack x 2>&1 || true) | grep -q RETIRED && ok || bad "_needs_stack must name its retirement"
_npm_jest_env "/x/platform/api" 2>/dev/null && bad "_npm_jest_env answered — must be retired" || ok
_npm_package_uses_jest "/x/pkg" 2>/dev/null && bad "_npm_package_uses_jest answered — must be retired" || ok

# 3. _stack_up survives — the smoke tier still needs a live-stack probe.
type _stack_up >/dev/null 2>&1 && ok || bad "_stack_up must survive for the smoke tier"

# 4. The invariants MOVED, not vanished: the runner declares typed skips (#3919)
#    and owns RUN_INTEGRATION (#3559).
grep -q 'needs_stack_files' "$SCRIPT_DIR/../services/werk-test/src/main.rs" && ok || bad "runner typed-skip selection missing"
grep -q 'RUN_INTEGRATION' "$SCRIPT_DIR/../services/werk-test/src/main.rs" && ok || bad "runner RUN_INTEGRATION gate missing"

# 5. NEGATIVE PROOF (#3734): a fixture that RESURRECTS the allowlist makes
#    check 1's grep fire (assembled from fragments — the #3725 self-match lesson).
TMP=$(mktemp -d)
printf '_needs_stack() {\n  case "$1" in\n    */%s*.bats) return 0 ;;\n  esac\n}\n' "deep-health" > "$TMP/n.sh"
grep -A4 '^_needs_stack()' "$TMP/n.sh" | grep -qE 'deep-health' && ok || bad "guard did not fire on a resurrected allowlist"
rm -rf "$TMP"

[ "$FAIL" -eq 0 ] && exit 0 || exit 1
