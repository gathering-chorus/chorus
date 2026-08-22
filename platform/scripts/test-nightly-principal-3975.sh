#!/usr/bin/env bash
# test-nightly-principal-3975.sh — guard for #3975: the 03:00 runner has a
# modeled machine principal with LEAST privilege (tests graph only), the seed
# set provisions it, and the identity mint stays fail-closed when the
# credential is absent (the typed-skip path, never a silent drop).

set -uo pipefail
trap '_rc=$?; if [ $_rc -eq 0 ]; then echo "=== Results: $PASS passed, 0 failed ==="; else echo "=== Results: $PASS passed, $FAIL failed ==="; fi' EXIT

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PRINCIPALS="$ROOT/roles/silas/ontology/identity-principals-3613.ttl"
SCOPES="$ROOT/roles/silas/ontology/security-scopes-3689.ttl"
PASS=0; FAIL=0
ok()   { PASS=$((PASS+1)); }
bad()  { FAIL=$((FAIL+1)); echo "FAIL: $1"; }
TMP=$(mktemp -d)
cleanup() { rm -rf "$TMP"; }

# The grant block runs from the subject line to its terminating '.'.
nightly_scopes()         { sed -n '/^chorus:principal-nightly$/,/\./p' "$1" | grep -c 'hasScope'; }
nightly_nontest_scopes() { sed -n '/^chorus:principal-nightly$/,/\./p' "$1" | grep 'hasScope' | grep -vc 'urn:chorus:domains:tests'; }

# 1. The principal is modeled: service kind, no sign-in.
grep -q 'chorus:principal-nightly a chorus:Principal' "$PRINCIPALS" && ok || bad "principal-nightly missing from identity TTL"
sed -n '/^chorus:principal-nightly a chorus:Principal/,/ \.$/p' "$PRINCIPALS" | grep -q 'principalKind "service"' && ok || bad "nightly must be a service principal"
sed -n '/^chorus:principal-nightly a chorus:Principal/,/ \.$/p' "$PRINCIPALS" | grep -q 'canSignIn "false"' && ok || bad "nightly must not sign in"

# 2. LEAST privilege: exactly one scope, and it is the tests graph.
[ "$(nightly_scopes "$SCOPES")" -eq 1 ] && ok || bad "nightly must hold exactly 1 scope, has $(nightly_scopes "$SCOPES")"
[ "$(nightly_nontest_scopes "$SCOPES")" -eq 0 ] && ok || bad "nightly holds a scope beyond the tests graph"

# 3. NEGATIVE PROOF (#3734): a widened grant makes checks 2 FAIL. Fixture
#    assembled from fragments so this file never matches the guard itself.
cp "$SCOPES" "$TMP/scopes.ttl"
printf 'chorus:principal-nightly\n    chorus:%s "urn:chorus:%s"^^xsd:anyURI .\n' "hasScope" "ontology" >> "$TMP/scopes.ttl"
[ "$(nightly_nontest_scopes "$TMP/scopes.ttl")" -gt 0 ] && ok || bad "guard did not fire on a widened-scope fixture"

# 4. Seed set provisions the nightly principal.
grep -qE 'AGENTS=.*nightly' "$SCRIPT_DIR/seed-css.sh" && ok || bad "seed-css.sh AGENTS set lacks nightly"

# 5. Fail-closed mint: no credential -> nonzero exit AND no value on stdout
#    (this IS the typed-skip path — the runner witnesses, never silently drops).
out=$(CHORUS_IDENTITY_DIR="$TMP/empty-identity" "$SCRIPT_DIR/chorus-identity-token" nightly 2>/dev/null); rc=$?
[ "$rc" -ne 0 ] && ok || bad "mint with no credential must exit nonzero"
[ -z "$out" ] && ok || bad "mint with no credential must print NOTHING on stdout"

# 6. The runner mints as the nightly principal (env-overridable, default nightly).
grep -q 'WERK_NIGHTLY_MINT_ROLE' "$ROOT/platform/services/werk-test/src/main.rs" && ok || bad "run_nightly lacks the nightly mint role"

cleanup
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
