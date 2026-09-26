#!/usr/bin/env bash
# @test-type: unit — reads the seed manifest and the files it names; no store, no network
#
# #4324 — every boundTo the seed posts names a row that is already there. From
# 09-26 06:30 every model land stopped at its seed step: "boundTo → adr-051
# exists neither in the store nor in this batch".
#
# Reopened 09-26 17:00: the first version read every designing/data/*.ttl, so it
# passed with adr-instances.ttl on disk and NOT in the manifest, and the land's
# seed refused again. The seed reads the manifest, group by group, and checks a
# group's edges against the store plus that group's own batch. So does this:
# groups in manifest order, "store" = every earlier group's subjects.
# Negative proofs: the ADR line removed, and the ADR line moved after the
# governance checks, are both caught.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MANIFEST="$ROOT/platform/config/instance-seed-manifest.txt"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
pass=0; fail=0
command -v riot >/dev/null || { echo "FAIL: Jena riot not on PATH (without it nothing here is checked)"; exit 1; }
[ -f "$MANIFEST" ] || { echo "FAIL: no seed manifest at $MANIFEST"; exit 1; }

dangling() { # $1 = manifest; prints "group target" for each boundTo not yet seeded
  : > "$TMP/seen"; edges=0
  while IFS= read -r line; do
    case "$line" in ''|\#*) continue;; esac
    f="$ROOT/${line#*:}"
    riot --output=nt "$f" 2>/dev/null > "$TMP/g.nt"
    awk '{print $1}' "$TMP/g.nt" >> "$TMP/seen"; sort -u -o "$TMP/seen" "$TMP/seen"
    awk '$2=="<https://jeffbridwell.com/chorus#boundTo>" {print $3}' "$TMP/g.nt" | sort -u > "$TMP/t"
    edges=$((edges + $(grep -c . "$TMP/t")))
    comm -23 "$TMP/t" "$TMP/seen" | sed "s|^|${line%%:*} |"
  done < "$1"
  echo "$edges" > "$TMP/edges"
}

out=$(dangling "$MANIFEST")
[ -z "$out" ] && { echo "PASS every boundTo the seed posts names a row seeded before it"; pass=$((pass+1)); } || { echo "FAIL boundTo target(s) not yet seeded:"; echo "$out"; fail=$((fail+1)); }
[ "$(cat "$TMP/edges")" -ge 4 ] && { echo "PASS the governance checks' boundTo targets were checked ($(cat "$TMP/edges") distinct per group, not an empty set)"; pass=$((pass+1)); } || { echo "FAIL fewer boundTo targets than the governance checks carry: the check read nothing"; fail=$((fail+1)); }

grep -v '^a-d-r:' "$MANIFEST" > "$TMP/no-adr.txt"
dangling "$TMP/no-adr.txt" | grep -q "adr-051" && { echo "PASS negative proof: the manifest without the ADR line is caught"; pass=$((pass+1)); } || { echo "FAIL the manifest without the ADR line was not caught"; fail=$((fail+1)); }

{ grep -v '^a-d-r:' "$MANIFEST"; grep '^a-d-r:' "$MANIFEST"; } > "$TMP/late-adr.txt"
dangling "$TMP/late-adr.txt" | grep -q "^governance-check .*adr-051" && { echo "PASS negative proof: ADRs seeded after the checks that name them is caught"; pass=$((pass+1)); } || { echo "FAIL ADRs seeded after the governance checks was not caught"; fail=$((fail+1)); }

echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
