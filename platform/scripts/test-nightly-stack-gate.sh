#!/usr/bin/env bash
# test-nightly-stack-gate.sh — guard for #3557 (nightly stack-gate).
#
# Live-stack suites (real HTTP / deploy / health probe) can't pass headless
# without the stack, so the nightly must SKIP them (not FAIL) when the stack is
# down — otherwise it prints "N env failures" that aren't regressions. This test
# is itself HERMETIC: it sources the runner's functions and forces the stack
# probe, so it needs no live stack and is deterministic everywhere.
#
# Run directly (not via Claude hook-intercepted Bash).

set -uo pipefail
trap '_rc=$?; if [ $_rc -eq 0 ]; then echo "=== Results: $PASS passed, 0 failed ==="; else echo "=== Results: $PASS passed, $FAIL failed ==="; fi' EXIT

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); }
bad()  { FAIL=$((FAIL+1)); echo "FAIL: $1"; }

# Source the runner — the dispatch guard returns on source, so we get functions only.
source "$SCRIPT_DIR/nightly-suites.sh"

# 1. needs-stack classification — the live-stack families MUST gate.
for p in test-api-health.sh test-agent-state.sh 3369-deep-health-resilience.bats \
         3405-deep-health-probe-truthfulness.bats alert-delivery.bats alert-suppress.bats \
         ac4-chorus-deploy-rollback.bats ac-3270-deploy-live-main.bats \
         ac-3232-deploy-verify-running.bats ac2-deploy-daemon-card.bats; do
  _needs_stack "/x/$p" && ok || bad "$p should be needs-stack"
done

# 2. hermetic suites MUST NEVER gate — gating one would HIDE a real regression.
for p in test-no-quick-flag.sh test-hardcoded-bin-paths.sh test-deploy-invariance.sh \
         test-skip-gates.sh test-demo.sh test-quality-health.sh; do
  _needs_stack "/x/$p" && bad "$p must stay hermetic (never gated)" || ok
done

# 3. stack DOWN -> a needs-stack suite is SKIPPED, not run, not failed.
_STACK_PROBE=down
line=$(run_one shell /x/test-api-health.sh silas)
case "$line" in
  *"|skip|"*) ok ;;
  *) bad "stack-down needs-stack suite should skip, got: $line" ;;
esac

# 4. a skip line carries status 'skip' (so notify_results' fail-count excludes it).
case "$line" in *"|fail|"*) bad "skip must not be status=fail" ;; *) ok ;; esac

echo "stack-gate: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
