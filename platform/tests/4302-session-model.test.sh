#!/usr/bin/env bash
# @test-type: unit — validates two fixtures against the shipped Session / SessionRun / Presence shapes with Jena shacl; no store, no network
#
# #4302 — the session model, step 1. Every rule gates writes, so each must be
# shown RED on data built to break it (#3734) and green on data that keeps it.
# The shapes are read from the shipped ontology files, never copied here, so a
# change to a shipped rule is what this test exercises. A rule that is renamed
# or deleted drops out of the report and this test fails on the missing row.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ONT="$ROOT/roles/silas/ontology"
FIX="$ROOT/platform/tests/fixtures"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
pass=0; fail=0
command -v shacl >/dev/null || { echo "FAIL: Jena shacl not on PATH"; exit 1; }

cat "$ONT/session-4202.ttl" "$ONT/session-model-4302.ttl" > "$TMP/shapes.ttl"

report() { # $1 fixture → focus<TAB>rule, one per violation
  cat "$FIX/session-model-4302-common.ttl" "$FIX/$1" > "$TMP/data.ttl"
  shacl validate --shapes "$TMP/shapes.ttl" --data "$TMP/data.ttl" > "$TMP/$1.report" 2>"$TMP/$1.err"
  # A SPARQL rule is reported under its node shape, so each row is named by the
  # constraint whose sh:message matches the result's message; a property rule by
  # its property shape.
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

good=$(report session-model-4302-good.ttl)
if [ -z "$good" ] && ! grep -q . "$TMP/session-model-4302-good.ttl.err"; then
  echo "PASS good fixture conforms (live session, two linked runs, one proven presence, and a closed pre-#4302 session with no role or run)"; pass=$((pass+1))
else
  echo "FAIL good fixture should conform:"; printf '%s\n' "$good"; cat "$TMP/session-model-4302-good.ttl.err"; fail=$((fail+1))
fi

report session-model-4302-bad.ttl > "$TMP/bad.tsv"
expect "$TMP/bad.tsv" sA OpenSessionHasOneLiveRun
expect "$TMP/bad.tsv" sB OpenSessionActsAsARole
expect "$TMP/bad.tsv" rC LiveRunHasOnePresence
expect "$TMP/bad.tsv" pD ReachableNeedsADelivery
expect "$TMP/bad.tsv" pD PresenceShape-comment
expect "$TMP/bad.tsv" rE EndedRunSaysWhy
expect "$TMP/bad.tsv" rF SessionRunShape-ownedBy
n=$(grep -c . "$TMP/bad.tsv")
if [ "$n" = "7" ]; then echo "PASS bad fixture has exactly the 7 violations it was built with"; pass=$((pass+1));
else echo "FAIL bad fixture: $n violations, want 7:"; cat "$TMP/bad.tsv"; fail=$((fail+1)); fi

echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
