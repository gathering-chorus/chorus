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
PRE_MINT="$ROOT/designing/schemas/pre-mint-names.txt"   # #4316 — the same list the seed guard reads
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
# #4316 — the seed's IRI guard is checked row by row, not only kind by kind:
# this test passed on 2026-09-25 while the guard refused 'tool', because the
# guard read the hand table only. Now: a kind must be in the table or claimed,
# every subject must be in the chorus namespace, and a HAND-TABLE kind's
# subjects must follow its mint convention (bare slug, or <kind>-slug). Claimed
# kinds keep their pre-mint names (seed_iri_ok_with, athena-model lib.rs).
awk '/^const KINDS: /{on=1; next} on && /^\];/{exit} on' "$KINDS_SRC" \
  | grep -oE '^\s*\("[a-z0-9-]+", *"[A-Za-z]+", *(true|false)' | tr -d ' "(' > "$TMP/table"
while IFS=: read -r k f; do
  [ -n "$k" ] || continue
  c=$(class_of "$k")
  entry=$(grep "^$k," "$TMP/table" || true)
  if [ -z "$entry" ] && ! grep -qx "$c" "$TMP/claimed"; then
    echo "FAIL $k: no hand-table entry and no domain claims $c — the seed leg will refuse it"; fail=$((fail+1)); continue
  fi
  bad=0
  subs=$(riot --output=ntriples "$ROOT/$f" 2>/dev/null | awk -v C="<https://jeffbridwell.com/chorus#$c>" '$2=="<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>" && $3==C {print $1}' | sort -u)
  for s in $subs; do
    case "$s" in "<https://jeffbridwell.com/chorus#"*) ;; *) echo "FAIL $k: $s is outside the chorus namespace"; bad=$((bad+1)); continue ;; esac
    [ -n "$entry" ] || continue
    local_name=${s#<https://jeffbridwell.com/chorus#}; local_name=${local_name%>}
    grep -qx "$k:$local_name" "$PRE_MINT" && continue   # named before the mint table, listed by name
    if [ "${entry##*,}" = "true" ]; then
      printf '%s' "$local_name" | grep -qE '^[a-z0-9-]+$' || { echo "FAIL $k: $s breaks the bare-slug convention"; bad=$((bad+1)); }
    else
      case "$local_name" in "$k-"?*) ;; *) echo "FAIL $k: $s does not start with $k-"; bad=$((bad+1)) ;; esac
    fi
  done
  label="claimed, pre-mint names kept"; [ -n "$entry" ] && label="mint table"
  if [ "$bad" -eq 0 ]; then echo "PASS $k ($label): $(printf '%s\n' "$subs" | grep -c .) row(s)"; pass=$((pass+1)); else fail=$((fail+1)); fi
done < <(grep -vE '^\s*(#|$)' "$MANIFEST")

echo "=== Results: $pass passed, $fail failed ==="
[ "$fail" -eq 0 ]
