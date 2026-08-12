#!/bin/bash
# @test-type: unit
# #3839 — NEGATIVE PROOF (#3734) for the bug that landed and did not run.
#
# THE FAILURE, exactly: chorus-model-deploy's instance leg deduped its TTL list
# by reading the array back — `case " ${INSTANCE_SET[*]} " in ...` — with the
# array still EMPTY on the first iteration. Under `set -u` in bash 3.2 that is an
# unbound-variable error, so the deploy died at line 518 on canonical while
# passing in the werk, which runs a newer bash. #3839 merged (edf508c9) and did
# not deploy: landed, not live.
#
# This file proves BOTH directions, because a check that only shows the fix
# working cannot tell you the bug was ever real:
#   1. the OLD pattern still dies under bash 3.2 + set -u   (the violation)
#   2. the NEW pattern survives AND actually dedups          (the fix)
#   3. the deploy script contains no reachable instance of the old pattern
set -uo pipefail
ROOT="${CHORUS_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
DEPLOY="$ROOT/platform/scripts/chorus-model-deploy.sh"
fail=0
ok()   { echo "  PASS: $1"; }
bad()  { echo "  FAIL: $1" >&2; fail=1; }

echo "-- 3839: the deploy must survive bash 3.2 --"
echo "   (running under $(/bin/bash --version | head -1))"

# 1. THE VIOLATION — the old pattern, run for real. If this ever starts passing,
#    the platform changed and this whole test is measuring nothing; say so.
if /bin/bash -c 'set -u; a=(); case " ${a[*]} " in *x*) ;; *) ;; esac' 2>/dev/null; then
  bad "the OLD pattern did NOT fail — this bash tolerates an empty array under set -u, so this test can no longer detect the bug it exists for"
else
  ok "the old pattern dies under bash 3.2 + set -u (the bug is real and reachable)"
fi

# 2. THE FIX — same job, string-based, must survive AND dedup.
out=$(/bin/bash -c '
  set -u
  KINDS=("a:/tmp/one.ttl" "b:/tmp/one.ttl" "c:/tmp/two.ttl")
  seen=""; SET=()
  for e in "${KINDS[@]}"; do
    f="${e#*:}"
    case "$seen" in *"|$f|"*) ;; *) SET+=("$f"); seen="${seen}|$f|" ;; esac
  done
  echo "${#SET[@]}"
' 2>&1)
if [ "$out" = "2" ]; then
  ok "the new pattern survives set -u and dedups 3 entries to 2 distinct files"
else
  bad "the new pattern gave '$out', expected '2'"
fi

# 3. The deploy itself must not carry the fatal shape. Anchored to the ARRAY
#    read specifically — a comment that merely names the pattern must not be able
#    to satisfy or trip this check.
if grep -nE '^[^#]*\$\{INSTANCE_SET\[\*\]\}' "$DEPLOY" >/dev/null 2>&1; then
  bad "chorus-model-deploy.sh still reads \${INSTANCE_SET[*]} outside a comment:"
  grep -nE '^[^#]*\$\{INSTANCE_SET\[\*\]\}' "$DEPLOY" >&2
else
  ok "no reachable \${INSTANCE_SET[*]} read in chorus-model-deploy.sh"
fi

# 4. And the whole instance leg must parse under 3.2 — the crash was a runtime
#    one, but a parse failure would be the same class of "works here, not there".
if /bin/bash -n "$DEPLOY" 2>/dev/null; then
  ok "chorus-model-deploy.sh parses under bash 3.2"
else
  bad "chorus-model-deploy.sh does not parse under bash 3.2"
fi

[ "$fail" -eq 0 ] && echo "-- 3839 deploy/bash32: all checks pass --"
exit "$fail"
