#!/usr/bin/env bash
# @test-type: unit — runs the shipped gc-pre-mint-names query against two fixtures; no store, no network
#
# #4316 — the pre-mint names the seed accepts are counted as debt by a governance
# check (Wren, 2026-09-25). A check that measures must be shown red on data built
# to have the debt and 0 on data without it (#3734). The query is read from the
# shipped checks file, so a renamed or deleted check fails here.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKS="$ROOT/designing/data/governance-check-instances.ttl"
FIX="$ROOT/platform/tests/fixtures"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
pass=0; fail=0
sparql --data "$CHECKS" --results TSV \
  'PREFIX c: <https://jeffbridwell.com/chorus#> SELECT ?q WHERE { c:gc-pre-mint-names c:checkQuery ?q }' \
  | tail -n +2 | python3 -c 'import sys,json; s=sys.stdin.read().strip(); print(json.loads(s) if s.startswith("\"") else s)' > "$TMP/q.rq"
grep -q SELECT "$TMP/q.rq" || { echo "FAIL gc-pre-mint-names: query not found in the checks file"; exit 1; }
rows() { sparql --data "$1" --query "$TMP/q.rq" --results TSV | tail -n +2 | grep -c . || true; }
red=$(rows "$FIX/pre-mint-4316-bad.ttl"); green=$(rows "$FIX/pre-mint-4316-good.ttl")
[ "$red" = "2" ] && { echo "PASS red on the bad fixture: 2 pre-mint rows"; pass=$((pass+1)); } || { echo "FAIL bad fixture: got $red, want 2"; fail=$((fail+1)); }
[ "$green" = "0" ] && { echo "PASS green on the good fixture"; pass=$((pass+1)); } || { echo "FAIL good fixture: got $green, want 0"; fail=$((fail+1)); }
# the list the seed accepts and the debt the check counts are the same 19 today
n=$(grep -vcE '^\s*(#|$)' "$ROOT/designing/schemas/pre-mint-names.txt")
declared=$(sparql --data "$CHECKS" --results TSV 'PREFIX c: <https://jeffbridwell.com/chorus#> SELECT ?n WHERE { c:gc-pre-mint-names c:provenRedRows ?n }' | tail -n +2 | tr -dc 0-9)
[ "$n" = "$declared" ] && { echo "PASS the seed's list and the check's proven count agree: $n"; pass=$((pass+1)); } || { echo "FAIL pre-mint-names.txt has $n, the check was proven on $declared"; fail=$((fail+1)); }
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
