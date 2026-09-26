#!/usr/bin/env bash
# @test-type: unit — validates two fixtures against the shipped Context / Conversation / Channel / Message shapes with Jena shacl; no store, no network
#
# #4323 — the other half of the session model. Every rule gates writes, so each
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

good=$(report session-model-4323-good.ttl)
if [ -z "$good" ] && ! grep -q . "$TMP/session-model-4323-good.ttl.err"; then
  echo "PASS good fixture conforms (a boot context and a conversation on a live run, a nudge channel, a message over it, and an older message with no channel)"; pass=$((pass+1))
else
  echo "FAIL good fixture should conform:"; printf '%s\n' "$good"; cat "$TMP/session-model-4323-good.ttl.err"; fail=$((fail+1))
fi

report session-model-4323-bad.ttl > "$TMP/bad.tsv"
expect "$TMP/bad.tsv" cxA ContextShape-contextOf
expect "$TMP/bad.tsv" cxB StampedByTheDal
expect "$TMP/bad.tsv" cxC ContextShape-contextKind
expect "$TMP/bad.tsv" cvD ConversationShape-conversationOf
expect "$TMP/bad.tsv" chE ChannelShape-channelKind
expect "$TMP/bad.tsv" chF ChannelShape-ownedBy
expect "$TMP/bad.tsv" mG MessageShape-overChannel
n=$(grep -c . "$TMP/bad.tsv")
if [ "$n" = "7" ]; then echo "PASS bad fixture has exactly the 7 violations it was built with"; pass=$((pass+1));
else echo "FAIL bad fixture: $n violations, want 7:"; cat "$TMP/bad.tsv"; fail=$((fail+1)); fi

# The three classes are claimed, so athena-make will serve them.
for pair in "memory Context" "memory Conversation" "messages Channel"; do
  set -- $pair
  if sparql --data "$ONT/session-model-4302.ttl" --results TSV "PREFIX c: <https://jeffbridwell.com/chorus#> ASK { c:$1 c:definesVocabulary c:$2 }" | grep -q true; then
    echo "PASS $2 is claimed by the $1 domain"; pass=$((pass+1))
  else echo "FAIL $2 is not claimed by the $1 domain"; fail=$((fail+1)); fi
done

echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
