#!/usr/bin/env bash
# test-werk-code-contract.sh — guard for #3677: the werk-code contract check
# can actually distinguish the states it exists to separate (#3734). Each
# violation class is PLANTED in a fixture inventory and shown RED; the real
# inventory is shown structurally clean with its gaps NAMED, never hidden.

set -uo pipefail
trap '_rc=$?; if [ $_rc -eq 0 ]; then echo "=== Results: $PASS passed, 0 failed ==="; else echo "=== Results: $PASS passed, $FAIL failed ==="; fi' EXIT

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CHECK="$SCRIPT_DIR/werk-code-contract.sh"
PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); }
bad()  { FAIL=$((FAIL+1)); echo "FAIL: $1"; }
TMP=$(mktemp -d)
cleanup() { rm -rf "$TMP"; }

mkfix() { # $1=tsv body — builds a fixture root with a real fixture file
  mkdir -p "$TMP/root/platform/config" "$TMP/root/platform/tests"
  printf 'exists\n' > "$TMP/root/platform/tests/real.bats"
  printf '%s\n' "$1" > "$TMP/root/platform/config/werk-code-contract.tsv"
}

T=$(printf '\t')

# 1. The REAL inventory: structurally clean, gaps named (not a failure).
out=$("$CHECK" "$ROOT" 2>&1); rc=$?
[ "$rc" -eq 0 ] && ok || bad "real inventory should be structurally clean, rc=$rc"
echo "$out" | grep -q '^GAP:' && ok || bad "the gap ledger must be visible in output"
echo "$out" | grep -qE 'named gaps' && ok || bad "summary must count gaps"

# 2. NEGATIVE PROOF: unknown moment refuses.
mkfix "mech_a${T}teatime${T}platform/tests/real.bats${T}note"
"$CHECK" "$TMP/root" >/dev/null 2>&1 && bad "unknown moment must FAIL" || ok

# 3. NEGATIVE PROOF: a mechanism bound twice refuses (the placement rule).
mkfix "mech_a${T}edit${T}platform/tests/real.bats${T}x
mech_a${T}commit${T}platform/tests/real.bats${T}x"
"$CHECK" "$TMP/root" >/dev/null 2>&1 && bad "duplicate binding must FAIL" || ok

# 4. NEGATIVE PROOF: a phantom fixture path refuses — the hollow-row class.
mkfix "mech_a${T}edit${T}platform/tests/does-not-exist.bats${T}x"
out=$("$CHECK" "$TMP/root" 2>&1); rc=$?
[ "$rc" -ne 0 ] && ok || bad "phantom fixture must FAIL"
echo "$out" | grep -q 'hollow contract row' && ok || bad "refusal must name the hollow-row class"

# 5. A GAP row alone is NOT a failure — honesty is legal, hiding is not.
mkfix "mech_a${T}edit${T}GAP${T}named hole"
"$CHECK" "$TMP/root" >/dev/null 2>&1 && ok || bad "a named GAP must not fail the check"

# 6. The inventory file itself missing = LOUD failure (the contract can't vanish).
rm "$TMP/root/platform/config/werk-code-contract.tsv"
"$CHECK" "$TMP/root" >/dev/null 2>&1 && bad "missing inventory must FAIL LOUD" || ok

cleanup
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
