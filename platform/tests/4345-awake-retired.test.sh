#!/usr/bin/env bash
# @test-type: unit — runs the retired-name stub against a stub chorus-principal; no live services
#
# #4345 — chorus-awake is folded into chorus-principal. A leftover call to the old
# name says the one command to use, every time, and (until strict) still reaches
# chorus-principal with the same arguments so a running session's hooks keep working.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STUB="$ROOT/platform/scripts/chorus-awake-retired"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
pass=0; fail=0
printf '#!/bin/sh\necho "principal got: $*"\n' > "$TMP/principal"; chmod +x "$TMP/principal"
out=$(CHORUS_PRINCIPAL_BIN="$TMP/principal" sh "$STUB" seen silas 2>"$TMP/err"); rc=$?
grep -q "chorus-awake is retired (#4345). Use: chorus-principal seen silas" "$TMP/err" && { echo "PASS the old name says the one command to use"; pass=$((pass+1)); } || { echo "FAIL no retirement line: $(cat "$TMP/err")"; fail=$((fail+1)); }
[ "$out" = "principal got: seen silas" ] && [ "$rc" = 0 ] && { echo "PASS it reaches chorus-principal with the same arguments"; pass=$((pass+1)); } || { echo "FAIL forwarded '$out' rc=$rc"; fail=$((fail+1)); }
out=$(CHORUS_AWAKE_RETIRED_STRICT=1 CHORUS_PRINCIPAL_BIN="$TMP/principal" sh "$STUB" off kade 2>/dev/null); rc=$?
[ "$rc" = 2 ] && [ -z "$out" ] && { echo "PASS negative proof: strict mode refuses and runs nothing"; pass=$((pass+1)); } || { echo "FAIL strict ran '$out' rc=$rc"; fail=$((fail+1)); }
# nothing in the tree still calls the old binary (comments, this card's own files and the gate test's fixture paths aside)
left=$(cd "$ROOT" && git grep -nE '\.chorus/bin/chorus-awake|services/chorus-awake/' -- ':!*.md' ':!*.html' ':!*.jsonl' ':!platform/scripts/chorus-awake-retired' ':!platform/tests/4345-awake-retired.test.sh' ':!platform/tests/4345-retirement-gate-generic-names.test.sh' | grep -vE ':[0-9]+:\s*(#|//)' || true)
[ -z "$left" ] && { echo "PASS no live caller of chorus-awake is left in the tree"; pass=$((pass+1)); } || { echo "FAIL callers left:"; echo "$left"; fail=$((fail+1)); }
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
