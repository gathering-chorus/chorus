#!/usr/bin/env bash
# test-nightly-via-runner.sh — guard for the #3920 fold: the nightly's cargo
# lane executes via `werk-test --nightly` (ONE runner: nextest #3929,
# needs-stack typed skips #3919, per-case TestResult posts #3592), and the
# plain-`cargo test` second walker is RETIRED and cannot come back.
#
# HERMETIC: sources the runner's functions, stubs werk-test on PATH. No live
# stack, no real cargo.

set -uo pipefail
trap '_rc=$?; if [ $_rc -eq 0 ]; then echo "=== Results: $PASS passed, 0 failed ==="; else echo "=== Results: $PASS passed, $FAIL failed ==="; fi' EXIT

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); }
bad()  { FAIL=$((FAIL+1)); echo "FAIL: $1"; }

TMP=$(mktemp -d)
cleanup() { rm -rf "$TMP"; }
export NIGHTLY_FAIL_DIR="$TMP/failures"

source "$SCRIPT_DIR/nightly-suites.sh"

stub_werk_test() { # $1 = stub body
  mkdir -p "$TMP/bin"
  printf '#!/usr/bin/env bash\n%s\n' "$1" > "$TMP/bin/werk-test"
  chmod +x "$TMP/bin/werk-test"
  export PATH="$TMP/bin:$PATH"
}

# 1. NEGATIVE PROOF (#3734): a suite red under werk-test IS red in the nightly
#    summary via the same typed path — the |fail| verdict, no second vocabulary.
stub_werk_test 'echo "nightly-unit|cargo|chorus-hooks|pass|41 pass, 0 fail"
echo "nightly-unit|cargo|werk-test|fail|30 pass, 2 fail"
exit 1'
out=$(run_cargo_lane)
echo "$out" | grep -q 'SUITE|cargo|platform/services/werk-test|silas|fail|30 pass, 2 fail' \
  && ok || bad "red under werk-test must be red in the SUITE summary, got: $out"
echo "$out" | grep -q 'SUITE|cargo|platform/services/chorus-hooks|silas|pass|41 pass, 0 fail' \
  && ok || bad "green unit folds as pass, got: $out"

# 2. all-green run folds clean — no fail rows.
stub_werk_test 'echo "nightly-unit|cargo|chorus-hooks|pass|41 pass, 0 fail"
exit 0'
out=$(run_cargo_lane)
echo "$out" | grep -q '|fail|' && bad "green run must have no fail rows: $out" || ok

# 3. typed stack-down skip survives the fold verbatim (one skip vocabulary).
stub_werk_test 'echo "nightly-unit|cargo|chorus-api-client|pass|10 pass, 0 fail, 3 SKIPPED (stack-down, typed #3919)"
exit 0'
out=$(run_cargo_lane)
echo "$out" | grep -q 'SKIPPED (stack-down, typed #3919)' \
  && ok || bad "typed skip must survive into the SUITE line, got: $out"

# 4. werk-test refusing (registry down) is a LOUD red line, never silence.
stub_werk_test 'echo "refusing: tests domain unreachable" >&2
exit 1'
out=$(run_cargo_lane)
echo "$out" | grep -q 'SUITE|runner|.*|silas|fail|' \
  && ok || bad "a refused nightly run must fold as a red SUITE line, got: $out"

# 5. werk-test absent from PATH → loud red, cargo lane never silently vanishes.
out=$(PATH="/usr/bin:/bin" HOME="$TMP/nohome" run_cargo_lane)
echo "$out" | grep -q 'SUITE|runner|.*|fail|.*werk-test' \
  && ok || bad "missing werk-test must be a loud red naming the binary, got: $out"

# 6. a red unit persists its failure output so the red can explain itself (#3484).
stub_werk_test 'echo "nightly-unit|cargo|werk-test|fail|0 pass, 1 fail"
echo "test bad_case ... FAILED"
exit 1'
run_cargo_lane >/dev/null
_flog=$(_fail_log_path cargo "platform/services/werk-test")
[ -s "$_flog" ] && ok || bad "failing unit must persist its output at $_flog"

# 7. run_all routes cargo through the one runner: no `run_one cargo` walker loop.
body=$(sed -n '/^run_all()/,/^}/p' "$SCRIPT_DIR/nightly-suites.sh")
echo "$body" | grep -q 'run_cargo_lane' \
  && ok || bad "run_all must call run_cargo_lane"
echo "$body" | grep -q 'run_one cargo' \
  && bad "run_all still walks cargo suites itself (second walker)" || ok

# 8. GUARD: the plain `cargo test` invocation (the retired second walker,
#    old line 415) is gone from nightly-suites.sh.
_walker_guard() { grep -cE "cargo[ ]test" "$1"; }
n=$(_walker_guard "$SCRIPT_DIR/nightly-suites.sh")
[ "$n" -eq 0 ] && ok || bad "plain cargo-test invocation still present in nightly-suites.sh ($n hits) — the second walker is back"

# 9. NEGATIVE PROOF for the guard itself (#3734): a fixture WITH the violation
#    must make the guard fire. (Assembled from fragments so this test file
#    never contains the guarded string itself — the #3725 lesson.)
fixture="$TMP/violating-nightly.sh"
cp "$SCRIPT_DIR/nightly-suites.sh" "$fixture"
printf 'run_legacy() { %s %s --release; }\n' "cargo" "test" >> "$fixture"
n=$(_walker_guard "$fixture")
[ "$n" -gt 0 ] && ok || bad "guard did not fire on a fixture containing the retired walker"

# ── #3974: every lane folds through the ONE runner ──

# 10. Multi-kind lines fold with ONE owner rule: npm under directing → kade,
#     bats/shell under platform → silas, cargo under platform/services → silas.
stub_werk_test 'echo "nightly-unit|npm|directing/clearing|fail|10 pass, 3 fail"
echo "nightly-unit|npm|platform/mcp-server|pass|226 pass, 0 fail"
echo "nightly-unit|bats|platform/tests/guard.bats|pass|4 pass, 0 fail"
echo "nightly-unit|shell|platform/scripts/test-x.sh|fail|5 pass, 2 fail"
exit 1'
out=$(run_cargo_lane)
echo "$out" | grep -q 'SUITE|npm|directing/clearing|kade|fail|10 pass, 3 fail' \
  && ok || bad "npm fold + kade routing, got: $out"
echo "$out" | grep -q 'SUITE|bats|platform/tests/guard.bats|silas|pass|' \
  && ok || bad "bats fold + silas routing, got: $out"
echo "$out" | grep -q 'SUITE|shell|platform/scripts/test-x.sh|silas|fail|' \
  && ok || bad "shell fold, got: $out"

# 11. run_all has NO tier walkers left — one runner call, no run_one loops.
body=$(sed -n '/^run_all()/,/^}/p' "$SCRIPT_DIR/nightly-suites.sh")
echo "$body" | grep -qE 'run_one (npm|shell|bats|cucumber)' \
  && bad "run_all still walks a tier itself (second walker)" || ok

# 12. RETIREMENT GUARDS (#3557 allowlist + npm helpers): the stubs fail LOUD.
_needs_stack "/x/test-api-health.sh" 2>/dev/null && bad "_needs_stack must be retired" || ok
(_needs_stack "/x/anything" 2>&1 || true) | grep -q RETIRED && ok || bad "retired stub must say so"
_npm_jest_env "/x/platform/api" 2>/dev/null && bad "_npm_jest_env must be retired" || ok

# 13. The #3559 api-integration invariant moved INTO the runner: werk-test
#     sets RUN_INTEGRATION from the live stack probe.
grep -q 'RUN_INTEGRATION' "$SCRIPT_DIR/../services/werk-test/src/main.rs" \
  && ok || bad "runner must own the RUN_INTEGRATION gate"

cleanup
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
