#!/usr/bin/env bash
# test-gate-test-type-negative.sh — #3677 commit-moment negative suite for the
# @test-type header gate (#3442). The gate has blocked real commits for months
# with NO fixture proving it still can — the exact hollow-gate class (#3734)
# the werk-code contract exists to close.
#
# HERMETIC: fixture git repo in a tempdir; the gate runs against ITS staged set.

set -uo pipefail
trap '_rc=$?; if [ $_rc -eq 0 ]; then echo "=== Results: $PASS passed, 0 failed ==="; else echo "=== Results: $PASS passed, $FAIL failed ==="; fi' EXIT

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
GATE="$SCRIPT_DIR/gate-test-type.sh"
PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); }
bad()  { FAIL=$((FAIL+1)); echo "FAIL: $1"; }
TMP=$(mktemp -d)
cleanup() { rm -rf "$TMP"; }

[ -x "$GATE" ] || { bad "gate-test-type.sh missing/not executable — the gate itself is gone"; cleanup; exit 1; }

mkrepo() {
  R="$TMP/repo-$RANDOM"; mkdir -p "$R/platform/api/tests"
  git -C "$R" init -q
  git -C "$R" -c user.email=t@t -c user.name=t commit -q --allow-empty -m base
  echo "$R"
}

# 1. NEGATIVE PROOF: a STAGED test file with NO @test-type header is REFUSED.
R=$(mkrepo)
printf 'it("x",()=>{expect(1).toBe(1)});\n' > "$R/platform/api/tests/rogue.test.ts"
git -C "$R" add -A
(cd "$R" && GATE_ROOT="$R" "$GATE" staged) >/dev/null 2>&1 && bad "headerless staged test must be refused" || ok

# 2. The same file WITH a declaration passes.
R=$(mkrepo)
printf '// @test-type: unit\nit("x",()=>{expect(1).toBe(1)});\n' > "$R/platform/api/tests/ok.test.ts"
git -C "$R" add -A
(cd "$R" && GATE_ROOT="$R" "$GATE" staged) >/dev/null 2>&1 && ok || bad "declared staged test must pass"

# 3. A junk declaration does not count as one.
R=$(mkrepo)
printf '// @test-type: banana\nit("x",()=>{});\n' > "$R/platform/api/tests/junk.test.ts"
git -C "$R" add -A
(cd "$R" && GATE_ROOT="$R" "$GATE" staged) >/dev/null 2>&1 && bad "junk test-type must be refused" || ok

# 4. Non-test staged files are ignored (no false positives).
R=$(mkrepo)
printf 'export const x = 1;\n' > "$R/platform/api/src-file.ts"
git -C "$R" add -A
(cd "$R" && GATE_ROOT="$R" "$GATE" staged) >/dev/null 2>&1 && ok || bad "non-test file must not trip the gate"

cleanup
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
