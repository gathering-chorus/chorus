#!/usr/bin/env bash
# @test-type: unit — reads the seed manifest, athena-model's hand table and the model set's definesVocabulary claims from source; no store, no network
#
# #4302 — every kind the seed leg posts must be writable. athena-model knows a
# kind if it is in its hand table (KINDS) or if some domain in the model claims
# the class (definesVocabulary). A kind that is neither is refused at the seed
# leg, and the seed leg is where every model land has failed since 09-21:
#   unknown-kind: 'governance-check' — no domain claims a class that mints it
# Nothing checked this before a land. This does, from the committed sources.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MANIFEST="$ROOT/platform/config/instance-seed-manifest.txt"
MODEL_SET_SRC="$ROOT/platform/services/athena-deploy/src/lib.rs"
KINDS_SRC="$ROOT/platform/services/athena-model/src/lib.rs"
TMP="$(mktemp -d)"; trap 'rm -rf "$TMP"' EXIT
pass=0; fail=0

# the hand table: the first string of every tuple inside `const KINDS`
awk '/^const KINDS: /{on=1; next} on && /^\];/{exit} on' "$KINDS_SRC" \
  | grep -oE '^\s*\("[a-z0-9-]+"' | grep -oE '[a-z0-9-]+' | sort -u > "$TMP/hand"
[ -s "$TMP/hand" ] || { echo "FAIL: could not read the KINDS hand table from $KINDS_SRC (a moved table must fail, not pass)"; exit 1; }

# the model set: every file model_set() names, concatenated
awk '/pub fn model_set\(/{on=1} on && /^}/{exit} on' "$MODEL_SET_SRC" \
  | grep -oE '\{root\}/[^"]+\.ttl' | sed "s|{root}|$ROOT|" > "$TMP/files"
[ -s "$TMP/files" ] || { echo "FAIL: could not read model_set() from $MODEL_SET_SRC"; exit 1; }
while read -r f; do cat "$f"; echo; done < "$TMP/files" > "$TMP/model.ttl"
sparql --data "$TMP/model.ttl" --results TSV \
  'PREFIX c: <https://jeffbridwell.com/chorus#> SELECT DISTINCT ?cl WHERE { ?d c:definesVocabulary ?cl }' \
  | tail -n +2 | sed 's|.*#||; s|>||' | sort -u > "$TMP/claimed"

class_of() { # governance-check -> GovernanceCheck; a-p-i-surface -> APISurface (athena-model's derive_kind, inverted)
  printf '%s' "$1" | awk -F- '{ for (i=1;i<=NF;i++) printf "%s%s", toupper(substr($i,1,1)), substr($i,2) }'
}

kinds=$(grep -vE '^\s*(#|$)' "$MANIFEST" | cut -d: -f1 | sort -u)
[ -n "$kinds" ] || { echo "FAIL: no kinds read from $MANIFEST"; exit 1; }
for k in $kinds; do
  c=$(class_of "$k")
  if grep -qx "$k" "$TMP/hand"; then echo "PASS $k is in the hand table"; pass=$((pass+1))
  elif grep -qx "$c" "$TMP/claimed"; then echo "PASS $k is claimed ($c)"; pass=$((pass+1))
  else echo "FAIL $k: no hand-table entry and no domain claims $c — the seed leg will refuse it"; fail=$((fail+1)); fi
done

echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
