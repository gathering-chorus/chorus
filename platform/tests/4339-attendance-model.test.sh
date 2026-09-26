#!/usr/bin/env bash
# @test-type: unit — validates two fixtures against the shipped Context / Conversation / Channel / Message shapes with Jena shacl; no store, no network
#
# #4339 — who is in the room: Session attendedBy + lastAttendedAt, Presence focusedNow + checkedAt.
# is shown RED on a row built to break it (#3734) and green on rows that keep it.
# Shapes are read from the shipped files, never copied here. The runs these rows
# point at are #4302's good fixture, so a run is the real, valid thing.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ONT="$ROOT/roles/silas/ontology"
FIX="$ROOT/platform/tests/fixtures"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
pass=0; fail=0
command -v shacl >/dev/null || { echo "FAIL: Jena shacl not on PATH"; exit 1; }

cat "$ONT/session-4202.ttl" "$ONT/session-model-4302.ttl" "$ROOT/roles/wren/ontology/clearing-domains-3860.ttl" > "$TMP/shapes.ttl"

report() { # $1 fixture → focus<TAB>rule, one per violation
  cat "$FIX/session-model-4302-common.ttl" "$FIX/session-model-4302-good.ttl" "$FIX/$1" > "$TMP/data.ttl"
  shacl validate --shapes "$TMP/shapes.ttl" --data "$TMP/data.ttl" > "$TMP/$1.report" 2>"$TMP/$1.err"
  cat "$TMP/$1.report" "$TMP/shapes.ttl" > "$TMP/$1.both.ttl"
  sparql --data "$TMP/$1.both.ttl" --results TSV '
    PREFIX sh: <http://www.w3.org/ns/shacl#>
    SELECT ?focus ?rule WHERE {
      ?r a sh:ValidationResult ; sh:focusNode ?focus ; sh:sourceShape ?shape .
      OPTIONAL { ?r sh:resultMessage ?m . ?c sh:message ?m2 . FILTER(STR(?m) = STR(?m2)) }
      BIND(COALESCE(?c, ?shape) AS ?rule)
    }' | tail -n +2 | sed 's|<https://jeffbridwell.com/chorus#||g; s|>||g' | sort -u
}
expect() { # $1 report file, $2 focus, $3 rule
  if awk -F'\t' -v f="$2" -v r="$3" '$1==f && $2==r {found=1} END {exit !found}' "$1"; then echo "PASS $2 is refused by $3"; pass=$((pass+1));
  else echo "FAIL $2 should be refused by $3"; fail=$((fail+1)); fi
}

good=$(report session-model-4339-good.ttl)
if [ -z "$good" ] && ! grep -q . "$TMP/session-model-4339-good.ttl.err"; then
  echo "PASS good fixture conforms (Jeff attends s1 at a time; his terminal is on p1, read at a time)"; pass=$((pass+1))
else
  echo "FAIL good fixture should conform:"; printf '%s\n' "$good"; cat "$TMP/session-model-4339-good.ttl.err"; fail=$((fail+1))
fi

report session-model-4339-bad.ttl > "$TMP/bad.tsv"
expect "$TMP/bad.tsv" sA AttendedNeedsATime
expect "$TMP/bad.tsv" sB SessionShape-attendedBy
expect "$TMP/bad.tsv" pC FocusNeedsACheck
expect "$TMP/bad.tsv" pD PresenceShape-focusedNow
n=$(grep -c . "$TMP/bad.tsv")
if [ "$n" = "4" ]; then echo "PASS bad fixture has exactly the 4 violations it was built with"; pass=$((pass+1));
else echo "FAIL bad fixture: $n violations, want 4:"; cat "$TMP/bad.tsv"; fail=$((fail+1)); fi

echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
