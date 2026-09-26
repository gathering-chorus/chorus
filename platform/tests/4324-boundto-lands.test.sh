#!/usr/bin/env bash
# @test-type: unit — reads the shipped seed and model files; no store, no network
#
# #4324 — every boundTo in the seed files names a row that exists. From 09-26
# 06:30 every model land stopped at its seed step: "boundTo → adr-051 exists
# neither in the store nor in this batch". Checked against the files, the way
# the seed's own referential check reads its batch. Negative proof: a fixture
# with one dangling boundTo is caught.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
pass=0; fail=0
command -v riot >/dev/null || { echo "FAIL: Jena riot not on PATH (without it nothing here is checked)"; exit 1; }
dangling() { # $@ = extra files; prints boundTo targets with no subject
  riot --output=nt "$ROOT"/designing/data/*.ttl "$ROOT"/roles/*/ontology/*.ttl "$@" 2>/dev/null > "$TMP/all.nt"
  awk '$2=="<https://jeffbridwell.com/chorus#boundTo>" {print $3}' "$TMP/all.nt" | sort -u > "$TMP/targets"
  awk '{print $1}' "$TMP/all.nt" | sort -u > "$TMP/subjects"
  comm -23 "$TMP/targets" "$TMP/subjects"
}
n=$(dangling | grep -c . || true)
[ "$n" = "0" ] && { echo "PASS every boundTo in the seed files names a row that exists"; pass=$((pass+1)); } || { echo "FAIL $n boundTo target(s) with no row:"; dangling; fail=$((fail+1)); }
[ "$(riot --output=nt "$ROOT"/designing/data/*.ttl 2>/dev/null | grep -c '<https://jeffbridwell.com/chorus#boundTo>')" -ge 11 ] && { echo "PASS the governance checks' 11+ boundTo edges were checked (not an empty set)"; pass=$((pass+1)); } || { echo "FAIL fewer boundTo edges than the governance checks carry: the check read nothing"; fail=$((fail+1)); }
printf '@prefix c: <https://jeffbridwell.com/chorus#> .\nc:gc-probe c:boundTo c:adr-999 .\n' > "$TMP/bad.ttl"
dangling "$TMP/bad.ttl" | grep -q "adr-999" && { echo "PASS negative proof: a boundTo to a missing ADR is caught"; pass=$((pass+1)); } || { echo "FAIL a dangling boundTo was not caught"; fail=$((fail+1)); }
echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
