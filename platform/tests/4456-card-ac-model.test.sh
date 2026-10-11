#!/usr/bin/env bash
# @test-type: unit — validates fixtures against the shipped CardAC shape with Jena shacl; no store, no network
# @domain: cards
#
# #4456 — a CardAC row is one acceptance criterion and the check borg-prove runs
# for it. Card text is never executed: the only checks are http, sparql, test,
# or none (unmeasured, never met). Each rule is shown RED on a row built to break
# it (#3734) and green on rows that keep it. Shapes are read from the shipped file.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIX="$ROOT/platform/tests/fixtures"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
pass=0; fail=0
command -v shacl >/dev/null || { echo "FAIL: Jena shacl not on PATH"; exit 1; }
SHAPES="$ROOT/roles/wren/ontology/board-3654.ttl"

focus_nodes() { # $1 data file → focus nodes with a violation, one per line
  shacl validate --shapes "$SHAPES" --data "$1" > "$TMP/report.ttl" 2>"$TMP/err"
  sparql --data "$TMP/report.ttl" --results TSV '
    PREFIX sh: <http://www.w3.org/ns/shacl#>
    SELECT DISTINCT ?f WHERE { ?r a sh:ValidationResult ; sh:focusNode ?f }' \
    | tail -n +2 | sed 's|<https://jeffbridwell.com/chorus#||; s|>||' | sort -u
}

good="$(focus_nodes "$FIX/card-ac-4456.ttl")"
if [ -z "$good" ] && ! grep -q . "$TMP/err"; then echo "PASS http, sparql, test and none rows conform"; pass=$((pass+1));
else echo "FAIL good rows should conform: $good"; cat "$TMP/err"; fail=$((fail+1)); fi

cat "$FIX/card-ac-4456.ttl" "$FIX/card-ac-4456-bad.ttl" > "$TMP/bad.ttl"
bad="$(focus_nodes "$TMP/bad.ttl")"
for row in bad-no-card bad-shell bad-http-no-expect bad-none-with-target bad-index-zero bad-no-owner; do
  if grep -qx "$row" <<<"$bad"; then echo "PASS $row is refused"; pass=$((pass+1)); else echo "FAIL $row should be refused"; fail=$((fail+1)); fi
done
# the good rows stay good next to the bad ones (a shape that refuses everything proves nothing)
if ! grep -q '^ac-4456-' <<<"$bad"; then echo "PASS the good rows are not refused alongside"; pass=$((pass+1)); else echo "FAIL a good row was refused: $(grep '^ac-4456-' <<<"$bad")"; fail=$((fail+1)); fi
# the cards domain claims the class, so the door serves it
if grep -q 'chorus:cards chorus:definesVocabulary chorus:Card, chorus:CardAC' "$ROOT/roles/wren/ontology/clearing-domains-3860.ttl"; then echo "PASS the cards domain claims CardAC"; pass=$((pass+1)); else echo "FAIL the cards domain does not claim CardAC"; fail=$((fail+1)); fi

echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
