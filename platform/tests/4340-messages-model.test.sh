#!/usr/bin/env bash
# @test-type: unit — validates two fixtures against the shipped Context / Conversation / Channel / Message shapes with Jena shacl; no store, no network
#
# #4340 — the other half of the session model. Every rule gates writes, so each
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

good=$(report messages-4340-good.ttl)
if [ -z "$good" ] && ! grep -q . "$TMP/messages-4340-good.ttl.err"; then
  echo "PASS good fixture conforms (a peer nudge in a session, a machine alert with no principal yet, a channel that reaches a presence, one delivery that landed and one that failed)"; pass=$((pass+1))
else
  echo "FAIL good fixture should conform:"; printf '%s\n' "$good"; cat "$TMP/messages-4340-good.ttl.err"; fail=$((fail+1))
fi

report messages-4340-bad.ttl > "$TMP/bad.tsv"
expect "$TMP/bad.tsv" mA MessageRecordShape-senderName
expect "$TMP/bad.tsv" mB MessageRecordShape-messageKind
expect "$TMP/bad.tsv" mC MessageRecordShape-sentBy
expect "$TMP/bad.tsv" dD DeliveredNeedsAPresence
expect "$TMP/bad.tsv" dE DeliveryShape-deliveryOutcome
expect "$TMP/bad.tsv" dF DeliveryShape-deliveryOf
n=$(grep -c . "$TMP/bad.tsv")
if [ "$n" = "6" ]; then echo "PASS bad fixture has exactly the 6 violations it was built with"; pass=$((pass+1));
else echo "FAIL bad fixture: $n violations, want 6:"; cat "$TMP/bad.tsv"; fail=$((fail+1)); fi

if sparql --data "$ONT/session-model-4302.ttl" --results TSV "PREFIX c: <https://jeffbridwell.com/chorus#> ASK { c:messages c:definesVocabulary c:Delivery }" | grep -q true; then
  echo "PASS Delivery is claimed by the messages domain"; pass=$((pass+1))
else echo "FAIL Delivery is not claimed by the messages domain"; fail=$((fail+1)); fi
# the old blank-node shape (sentBy Role, required) is gone from the source, and its removal from the store is staged
if grep -q "chorus:MessageShape a sh:NodeShape" "$ROOT/roles/wren/ontology/clearing-domains-3860.ttl"; then echo "FAIL the retired MessageShape is still in clearing-domains-3860.ttl"; fail=$((fail+1)); else echo "PASS the retired MessageShape is out of the source"; pass=$((pass+1)); fi
if grep -q '"retire_subject": "https://jeffbridwell.com/chorus#MessageShape"' "$ROOT/designing/schemas/model-retirements.jsonl"; then echo "PASS its retirement is staged, so the deploy removes it from the store"; pass=$((pass+1)); else echo "FAIL MessageShape retirement not staged"; fail=$((fail+1)); fi

echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
