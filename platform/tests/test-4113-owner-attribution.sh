#!/usr/bin/env bash
# test-4113-owner-attribution.sh — #4113. Every role can be attributed a suite, and a
# path that cannot be attributed says so.
#
# WHY THIS EXISTS
# nightly-suites.sh owner_for() had three branches and none was "wren". Measured on the
# 2026-09-07 03:00 run: silas 374, kade 21, wren 0, of 395 suites. Wren's zero-red card
# was therefore green by construction — it could not have gone red if every line she
# owns were on fire — and Silas's could never go green. A function that cannot put a
# suite in Wren's column cannot be used to decide whether Wren's column is empty.
#
# NEGATIVE PROOF (#3734)
# It is not enough to check that owner_for returns "wren" for one path. The failure this
# guards is a mapping that QUIETLY LOSES a role, so this file proves the check itself
# goes RED against a mapping that has lost one — the old three-branch function is
# included verbatim as a fixture and shown to fail.
set -u

# The tree this file lives in — NOT $CHORUS_ROOT. A test that reads the env var tests
# whatever tree the shell happens to point at, which here meant a werk's test silently
# grading canonical's copy of the function and reporting on code that was not changed.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
NIGHTLY="$ROOT/platform/scripts/nightly-suites.sh"
CHORUS_ROOT="$ROOT"; APP_ROOT="${APP_ROOT:-$(cd "$ROOT/../jeff-bridwell-personal-site" 2>/dev/null && pwd)}"

pass=0; fail=0
ok()   { pass=$((pass+1)); echo "  ok   $1"; }
bad()  { fail=$((fail+1)); echo "  FAIL $1"; }

# Load ONLY the function under test — sourcing the whole nightly would run it.
eval "$(awk '/^owner_for\(\) \{/,/^\}/' "$NIGHTLY")"
if ! declare -f owner_for >/dev/null; then
  echo "FATAL: could not extract owner_for from $NIGHTLY" >&2; exit 2
fi

expect() { # expect <path> <owner>
  local got; got="$(owner_for "$1")"
  [ "$got" = "$2" ] && ok "$1 -> $2" || bad "$1 -> got '$got', want '$2'"
}

echo "== the map =="
expect "roles/wren/ontology/principles-instances-3749.ttl" wren
expect "platform/services/athena-make"                     wren
expect "platform/services/athena-model"                    wren
expect "roles/silas/adr/ADR-026.md"                        silas
expect "platform/services/werk-test"                       silas
expect "platform/scripts/nightly-suites.sh"                silas
expect "proving/scripts/tests/startup-sync-alert.test.sh"  silas
expect "roles/kade/ontology/practices.ttl"                 kade
expect "directing/clearing"                                kade

echo "== a path the map cannot decide says so =="
# platform/tests/*.bats owner is a fact about content, not directory. It must NOT be
# silently posted to a role — that is how 374 suites landed on one person.
expect "platform/tests/4102-revision-history.bats"         unowned
expect "some/path/nobody/declared"                         unowned

echo "== every role is reachable =="
reachable=""
for p in "roles/wren/x" "roles/silas/x" "roles/kade/x" "platform/services/athena-make"; do
  o="$(owner_for "$p")"; case "$reachable" in *"$o"*) ;; *) reachable="$reachable $o" ;; esac
done
for r in wren silas kade; do
  case "$reachable" in *"$r"*) ok "$r is reachable" ;; *) bad "$r is NOT reachable — its column can never be non-zero" ;; esac
done

echo "== NEGATIVE PROOF: the same check against a mapping that lost a role =="
# The pre-#4113 function, verbatim. If the assertions above can pass against this, they
# are not testing anything.
old_owner_for() {
  case "$1" in
    "$APP_ROOT"|"$APP_ROOT"/*)              echo "kade" ;;
    directing/*|"$CHORUS_ROOT"/directing/*) echo "kade" ;;
    platform/*|roles/*|"$CHORUS_ROOT"/platform/*|"$CHORUS_ROOT"/roles/*) echo "silas" ;;
    *)                                      echo "kade" ;;
  esac
}
neg=0
[ "$(old_owner_for "roles/wren/ontology/x.ttl")" = "wren" ] || neg=$((neg+1))
[ "$(old_owner_for "platform/tests/x.bats")" = "unowned" ] || neg=$((neg+1))
if [ "$neg" -eq 2 ]; then
  ok "the old three-branch map FAILS both checks (wren unreachable, no unowned state)"
else
  bad "the old map passed a check it should fail — these assertions prove nothing ($neg/2)"
fi

echo
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
