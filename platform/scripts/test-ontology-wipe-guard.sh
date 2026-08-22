#!/usr/bin/env bash
# test-ontology-wipe-guard.sh — guard for #3601: no test may reach the LIVE
# ontology graph. athena-deploy defaults ONTOLOGY_GRAPH to urn:chorus:ontology
# (lib.rs:70), so one future test invoking a deploy surface without a test-graph
# override wipes the 34 live-only domains. wipe-guard-scan.sh turns that from
# a convention into a structural guard; this test proves the guard on both
# sides (#3734: green today AND red on a planted violation).
#
# HERMETIC: fixtures live in a tempdir; the repo scan runs read-only.

set -uo pipefail
trap '_rc=$?; if [ $_rc -eq 0 ]; then echo "=== Results: $PASS passed, 0 failed ==="; else echo "=== Results: $PASS passed, $FAIL failed ==="; fi' EXIT

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SCAN="$SCRIPT_DIR/wipe-guard-scan.sh"
PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); }
bad()  { FAIL=$((FAIL+1)); echo "FAIL: $1"; }

TMP=$(mktemp -d)
cleanup() { rm -rf "$TMP"; }

[ -x "$SCAN" ] || { bad "wipe-guard-scan.sh missing or not executable"; cleanup; exit 1; }

# Fixture fragments — the deploy-surface names are ASSEMBLED, never written
# whole in this file, so the scanner can never match its own guard test (#3725).
# (renamed chorus-model-deploy → athena-deploy-model in the #3561 sweep)
DEPLOY_WORD="$(printf '%s-%s-%s.sh' athena deploy model)"
LIVE_GRAPH="$(printf 'urn:%s:%s' chorus ontology)"

# 1. The repo as it stands is CLEAN — every current test isolates (07-01 audit).
out=$("$SCAN" "$ROOT" 2>&1); rc=$?
[ "$rc" -eq 0 ] && ok || bad "repo scan should pass today, got rc=$rc: $(echo "$out" | tail -3)"

# 2. NEGATIVE PROOF: a test invoking the deploy script with NO override is caught.
mkdir -p "$TMP/repo/platform/tests" "$TMP/repo/platform/scripts"
cat > "$TMP/repo/platform/tests/rogue.bats" <<EOF
@test "deploys" {
  bash "\$ROOT/platform/scripts/$DEPLOY_WORD"
}
EOF
out=$("$SCAN" "$TMP/repo" 2>&1); rc=$?
[ "$rc" -ne 0 ] && ok || bad "bare-default deploy fixture must FAIL the scan"
echo "$out" | grep -q "rogue.bats" && ok || bad "violation names the offending file, got: $out"

# 3. The same invocation WITH a test-graph override passes.
cat > "$TMP/repo/platform/tests/rogue.bats" <<EOF
@test "deploys isolated" {
  env ONTOLOGY_GRAPH="urn:chorus:test-\$\$" bash "\$ROOT/platform/scripts/$DEPLOY_WORD"
}
EOF
"$SCAN" "$TMP/repo" >/dev/null 2>&1 && ok || bad "override fixture must pass"

# 4. An override that names the LIVE graph explicitly is still a violation.
cat > "$TMP/repo/platform/tests/rogue.bats" <<EOF
@test "deploys live" {
  env ONTOLOGY_GRAPH="$LIVE_GRAPH" bash "\$ROOT/platform/scripts/$DEPLOY_WORD"
}
EOF
"$SCAN" "$TMP/repo" >/dev/null 2>&1 && bad "explicit live-graph override must FAIL" || ok

# 5. A jest test POSTing a Fuseki update against the live graph is caught.
mkdir -p "$TMP/repo/platform/api/tests"
cat > "$TMP/repo/platform/api/tests/rogue.test.ts" <<EOF
// @test-type: unit
it('updates', async () => {
  await fetch('http://localhost:3030/pods/update', { method: 'POST', body: 'CLEAR GRAPH <$LIVE_GRAPH>' });
});
EOF
out=$("$SCAN" "$TMP/repo" 2>&1); rc=$?
[ "$rc" -ne 0 ] && ok || bad "jest live-graph update fixture must FAIL"
rm -f "$TMP/repo/platform/api/tests/rogue.test.ts"

# 6. An explicit exemption WITH a reason is honored (loud escape hatch, never silent).
cat > "$TMP/repo/platform/tests/rogue.bats" <<EOF
# wipe-guard: exempt — read-only assertion against the live graph, no update issued
@test "reads live" {
  bash "\$ROOT/platform/scripts/$DEPLOY_WORD" --dry-run
}
EOF
"$SCAN" "$TMP/repo" >/dev/null 2>&1 && ok || bad "reasoned exemption must pass"

# 7. A bare exemption marker with NO reason does not count.
cat > "$TMP/repo/platform/tests/rogue.bats" <<EOF
# wipe-guard: exempt
@test "reads live" {
  bash "\$ROOT/platform/scripts/$DEPLOY_WORD"
}
EOF
"$SCAN" "$TMP/repo" >/dev/null 2>&1 && bad "reasonless exemption must FAIL" || ok

cleanup
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
