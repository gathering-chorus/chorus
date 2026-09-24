#!/usr/bin/env bash
# @test-type: unit — runs the three #4293 layer checks, read out of the checks file itself, against two fixtures; no store, no network
#
# #4293 — the CMDB layer rules. A check that gates must be shown RED on a violation
# (#3734), so each check runs twice: on a fixture built to break it (expect exactly
# the rows named below) and on the same data with every edge allowed (expect 0).
# The queries are read from designing/data/governance-check-instances.ttl, so a
# change to the shipped query is what this test exercises, not a copy of it.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKS="$ROOT/designing/data/governance-check-instances.ttl"
BAD="$ROOT/platform/tests/fixtures/cmdb-layers-4293-bad.ttl"
GOOD="$ROOT/platform/tests/fixtures/cmdb-layers-4293-good.ttl"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
pass=0; fail=0

query_of() {
  # the check's own checkQuery, as shipped
  sparql --data "$CHECKS" --results TSV \
    "PREFIX c: <https://jeffbridwell.com/chorus#> SELECT ?q WHERE { c:$1 c:checkQuery ?q }" \
    | tail -n +2 | python3 -c 'import sys,json; s=sys.stdin.read().strip(); print(json.loads(s) if s.startswith("\"") else s)'
}
rows_on() { # $1 query file, $2 data
  sparql --data "$2" --query "$1" --results TSV | tail -n +2 | grep -c . || true
}
check() { # $1 check id, $2 expected red rows on BAD
  q="$TMP/$1.rq"
  query_of "$1" > "$q"
  if ! grep -q "SELECT" "$q"; then echo "FAIL $1: query not found in the checks file (a renamed or deleted check must fail, never pass)"; fail=$((fail+1)); return; fi
  red=$(rows_on "$q" "$BAD"); green=$(rows_on "$q" "$GOOD")
  if [ "$red" = "$2" ]; then echo "PASS $1 red on the bad fixture: $red row(s)"; pass=$((pass+1)); else echo "FAIL $1 red on the bad fixture: got $red, want $2"; fail=$((fail+1)); fi
  if [ "$green" = "0" ]; then echo "PASS $1 green on the good fixture"; pass=$((pass+1)); else echo "FAIL $1 green on the good fixture: got $green rows"; fail=$((fail+1)); fi
}

check gc-layer-never-points-up 2
check gc-no-runtime-edge-into-build 1
check gc-domain-has-a-layer 1

echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
