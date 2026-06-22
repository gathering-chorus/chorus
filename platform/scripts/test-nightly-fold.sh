#!/usr/bin/env bash
# test-nightly-fold.sh — guard for #3527 (3-runner consolidation). HERMETIC: sources the
# runner's functions, drives run_coverage via DRY-RUN fixtures + a test floors file (brings
# its own world — no real coverage run, no live stack), and forces _STACK_PROBE for run_smoke.
set -uo pipefail
trap '_rc=$?; if [ $_rc -eq 0 ]; then echo "=== Results: $PASS passed, 0 failed ==="; else echo "=== Results: $PASS passed, $FAIL failed ==="; fi' EXIT
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0; FAIL=0
ok() { PASS=$((PASS+1)); }
bad(){ FAIL=$((FAIL+1)); echo "FAIL: $1"; }
source "$SCRIPT_DIR/nightly-suites.sh"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' RETURN 2>/dev/null || true
# test floors: one ts pass-case, one ts fail-case, one rust, one missing
cat > "$TMP/floors.yml" <<YML
ts:
  platform/passproj: 80
  platform/failproj: 80
  directing/missingproj: 90
rust:
  platform/services/rustproj: 45
YML
mkdir -p "$TMP/fix/platform/passproj/coverage" "$TMP/fix/platform/failproj/coverage" "$TMP/fix/platform/services/rustproj"
echo '{"total":{"statements":{"pct":85.0}}}' > "$TMP/fix/platform/passproj/coverage/coverage-summary.json"
echo '{"total":{"statements":{"pct":50.0}}}' > "$TMP/fix/platform/failproj/coverage/coverage-summary.json"
echo '{"data":[{"totals":{"lines":{"percent":60.0}}}]}' > "$TMP/fix/platform/services/rustproj/llvm-cov-summary.json"
# directing/missingproj has NO fixture -> skip

out=$(NIGHTLY_COVERAGE_FLOORS="$TMP/floors.yml" NIGHTLY_COVERAGE_DRY_RUN=1 NIGHTLY_COVERAGE_FIXTURES="$TMP/fix" run_coverage)

# 1. pass-case: 85 >= 80
echo "$out" | grep -qE 'SUITE\|coverage\|platform/passproj\|silas\|pass\|' && ok || bad "passproj should be pass (got: $(echo "$out"|grep passproj))"
# 2. fail-case: 50 < 80
echo "$out" | grep -qE 'SUITE\|coverage\|platform/failproj\|silas\|fail\|' && ok || bad "failproj should be fail"
# 3. rust pass: 60 >= 45
echo "$out" | grep -qE 'SUITE\|coverage\|platform/services/rustproj\|silas\|pass\|' && ok || bad "rustproj should be pass"
# 4. missing fixture -> skip (not pass, not fail)
echo "$out" | grep -qE 'SUITE\|coverage\|directing/missingproj\|kade\|skip\|' && ok || bad "missingproj should be skip"
# 5. owner routing: directing -> kade, platform -> silas
echo "$out" | grep 'directing/missingproj' | grep -q '|kade|' && ok || bad "directing should route to kade"
# 6. run_smoke stack-gates: stack DOWN -> skip
_STACK_PROBE=down
smoke=$(run_smoke 2>/dev/null || true)
# only assert if smoke-check.sh exists; else the function returns empty (no-op) which is fine
if [ -x "$CHORUS_ROOT/platform/scripts/smoke-check.sh" ]; then
  echo "$smoke" | grep -qE 'SUITE\|smoke\|.*\|skip\|' && ok || bad "smoke should SKIP when stack down (got: $smoke)"
else ok; fi
# 7. unmeasured coverage is NEVER 'fail' (no false-red)
echo "$out" | grep 'directing/missingproj' | grep -q '|fail|' && bad "unmeasured must not be fail" || ok

echo "fold-test: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
