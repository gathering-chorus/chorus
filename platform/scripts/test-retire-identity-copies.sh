#!/usr/bin/env bash
# Negative proofs for retire-identity-principal-copies.sh (#3785 phase 2 / #3734).
#
# The retire deletes Principal records; done wrong it is the 2026-08-06 lockout.
# Each guard is proven to REFUSE against the state it exists to catch — the
# fixtures ARE the violations, seeded into scratch graphs so the real script
# runs its real guards against them.
#
#   GUARD 1 (superset): identity holds a record NOT in security → the delete
#     would lose the only copy → MUST refuse (exit 1).
#   Control: identity is a strict subset of security → GUARD 1 passes (then
#     GUARD 2 refuses because the scratch owl-api check can't see flow-probe —
#     which is itself the ordering guard doing its job, asserted as exit 2).
set -u

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SCRIPT="$ROOT/platform/scripts/retire-identity-principal-copies.sh"
Q="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
U="${FUSEKI_UPDATE:-http://localhost:3030/pods/update}"
NS="https://jeffbridwell.com/chorus#"
SG="urn:chorus:test:retire:security-$$"
IG="urn:chorus:test:retire:identity-$$"
# shellcheck disable=SC1091
source "$ROOT/platform/scripts/fuseki-auth.sh"

seed() {  # seed <graph> <principal-localnames...>
  local g="$1"; shift
  local body="PREFIX chorus: <$NS> INSERT DATA { GRAPH <$g> {"
  for p in "$@"; do body="$body chorus:$p a chorus:Principal ."; done
  body="$body } }"
  curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X POST -H 'Content-Type: application/sparql-update' --data "$body" "$U" >/dev/null
}
wipe() { for g in "$SG" "$IG"; do curl -s "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X POST -H 'Content-Type: application/sparql-update' --data "DROP GRAPH <$g>" "$U" >/dev/null 2>&1 || true; done; }
trap wipe EXIT

pass=0; fail=0
ok() { if eval "$2"; then echo "PASS: $1"; pass=$((pass+1)); else echo "FAIL: $1"; fail=$((fail+1)); fi; }

run() { RETIRE_SECURITY_GRAPH="$SG" RETIRE_IDENTITY_GRAPH="$IG" \
        CHORUS_ROOT="$ROOT" bash "$SCRIPT" 2>&1; }

# --- VIOLATION: identity has an orphan (principal-ghost NOT in security).
wipe
seed "$SG" principal-a principal-b
seed "$IG" principal-a principal-b principal-ghost
OUT="$(run)"; RC=$?
ok "GUARD 1 REFUSES when identity holds a record security does not (exit 1)" "test $RC -eq 1"
ok "GUARD 1 names the orphan, not a count" "grep -q 'orphan: .*principal-ghost' <<<\"\$OUT\""

# --- CONTROL: identity ⊆ security → GUARD 1 passes; GUARD 2 then refuses
# because the scratch home is not what owl-api serves (the ordering guard).
wipe
seed "$SG" principal-a principal-b
seed "$IG" principal-a
OUT="$(run)"; RC=$?
ok "GUARD 1 PASSES for a strict subset (does not false-refuse)" "grep -q 'GUARD 1 ok' <<<\"\$OUT\""
ok "GUARD 2 then refuses — retire cannot run before phase 1 is served live (exit 2)" "test $RC -eq 2"

echo "=== retire proofs: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
