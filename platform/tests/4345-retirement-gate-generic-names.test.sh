#!/usr/bin/env bash
# @test-type: unit — runs retirement-gate.sh on a fixture tree; no git, no live repo
# @domain: version-control — picked by its owner, Kade, from the crawler's candidates (#4538)
#
# #4345 — deleting chorus-awake/src/main.rs was blocked because 13 tests mention
# SOME main.rs. A generic name is matched by its last three path parts; a script
# is still matched by base name. Both directions are proven.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GATE="$ROOT/platform/scripts/retirement-gate.sh"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
pass=0; fail=0
mkdir -p "$T/platform/tests"
echo '# locks chorus-inject/src/main.rs' > "$T/platform/tests/other-main.bats"
run_gate() { CHORUS_ROOT="$T" RETGATE_DELETED="$1" bash "$GATE" 2>"$T/err"; echo $?; }
[ "$(run_gate platform/services/chorus-awake/src/main.rs)" = 0 ] && { echo "PASS a test naming another crate's main.rs does not block"; pass=$((pass+1)); } || { echo "FAIL blocked: $(cat "$T/err")"; fail=$((fail+1)); }
echo 'run platform/services/chorus-awake/src/main.rs' > "$T/platform/tests/awake.bats"
[ "$(run_gate platform/services/chorus-awake/src/main.rs)" = 1 ] && grep -q awake.bats "$T/err" && ! grep -q other-main.bats "$T/err" && { echo "PASS negative proof: a test naming the deleted main.rs by its path still blocks"; pass=$((pass+1)); } || { echo "FAIL not caught: $(cat "$T/err")"; fail=$((fail+1)); }
echo 'bash git-queue.sh' > "$T/platform/tests/q.bats"
[ "$(run_gate scripts/git-queue.sh)" = 1 ] && grep -q q.bats "$T/err" && { echo "PASS negative proof: a script is still caught by its base name"; pass=$((pass+1)); } || { echo "FAIL script not caught"; fail=$((fail+1)); }
# #4563 — deleting clearing-v2's sources/roles.ts was blocked by a test naming
# platform/api/src/handlers/context-roles.ts: the base name matched as a substring.
rm -f "$T"/platform/tests/*.bats
echo 'scan platform/api/src/handlers/context-roles.ts' > "$T/platform/tests/ctx.bats"
[ "$(run_gate directing/clearing-v2/src/sources/roles.ts)" = 0 ] && { echo "PASS a longer name that ends in roles.ts does not block"; pass=$((pass+1)); } || { echo "FAIL blocked: $(cat "$T/err")"; fail=$((fail+1)); }
echo 'import directing/clearing-v2/src/sources/roles.ts' > "$T/platform/tests/v2.bats"
[ "$(run_gate directing/clearing-v2/src/sources/roles.ts)" = 1 ] && grep -q v2.bats "$T/err" && ! grep -q ctx.bats "$T/err" && { echo "PASS negative proof: a test naming the deleted roles.ts still blocks"; pass=$((pass+1)); } || { echo "FAIL not caught: $(cat "$T/err")"; fail=$((fail+1)); }
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
