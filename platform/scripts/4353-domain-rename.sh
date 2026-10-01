#!/bin/bash
# #4353 step 4 — move the old-named Domain rows onto their bare names, in the store.
#
# Jeff 2026-10-01: "i want to remove not fix subdomain dependencies." Step 4 is
# the rows: 39 Domain rows named the subdomain way (cards-service, loom-principles,
# photos-domain …) live only in the store. The source files were rewritten in the
# same card; this moves what the store already holds, so nothing points at an old
# name afterwards.
#
# One rule for both kinds in the map (legacy<TAB>target<TAB>merge|rename):
#   - every triple that points AT the old name points at the new one (all graphs);
#   - the old row's own triples move to the new name, except a single-valued
#     property the new row already has: the new row's value wins (a merge keeps
#     its label, owner and step);
#   - then nothing with the old name is left.
#
# Usage:
#   4353-domain-rename.sh                 dry run: counts per name, no writes
#   4353-domain-rename.sh --apply         back up every triple naming an old row, then move
#   4353-domain-rename.sh --print-sparql  the update only (the test runs it on a fixture)
#
# Seams: RENAME_MAP (default designing/schemas/4353-domain-renames.tsv),
#        FUSEKI_DATASET_URL (default http://localhost:3030/pods).
set -euo pipefail
ROOT="${CHORUS_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
MAP="${RENAME_MAP:-$ROOT/designing/schemas/4353-domain-renames.tsv}"
DS="${FUSEKI_DATASET_URL:-http://localhost:3030/pods}"
C="https://jeffbridwell.com/chorus#"
RDFS="http://www.w3.org/2000/01/rdf-schema#"
MODE="${1:-dry}"

[ -r "$MAP" ] || { echo "FAIL no map at $MAP" >&2; exit 1; }

# Single-valued on a Domain row: when the target already has one, keep it.
SCALARS="<${RDFS}label> <${RDFS}comment> <${C}ownedBy> <${C}primaryStep> <${C}status> <${C}gaps> <${C}hasDesignDoc> <${C}diagram> <${C}atStep> <${C}atStream> <${C}partOf> <${C}belongsTo> <${C}builtBy>"

pairs() { grep -v '^legacy' "$MAP" | awk -F'\t' 'NF>=2 && $1!="" {print $1"\t"$2}'; }

update_for() {
  local l="<${C}$1>" t="<${C}$2>"
  cat <<EOF
DELETE { GRAPH ?g { ?s ?p $l } } INSERT { GRAPH ?g { ?s ?p $t } } WHERE { GRAPH ?g { ?s ?p $l } } ;
INSERT { GRAPH ?g { $t ?p ?o } } WHERE {
  GRAPH ?g { $l ?p ?o }
  FILTER NOT EXISTS { VALUES ?sp { $SCALARS } FILTER(?p = ?sp) GRAPH ?g2 { $t ?p ?any } }
} ;
DELETE WHERE { GRAPH ?g { $l ?p ?o } } ;
EOF
}

all_updates() { pairs | while IFS=$'\t' read -r l t; do update_for "$l" "$t"; done; }

count_for() {
  local l="<${C}$1>"
  echo "SELECT (COUNT(*) AS ?n) WHERE { { GRAPH ?g { $l ?p ?o } } UNION { GRAPH ?g { ?s ?p $l } } }"
}

if [ "$MODE" = "--print-sparql" ]; then all_updates; exit 0; fi

# shellcheck source=/dev/null
source "$ROOT/platform/scripts/fuseki-auth.sh"
q() {  # one SELECT → its single number
  curl -sf --max-time 60 "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -H 'Accept: text/csv' \
    --data-urlencode "query=$1" "$DS/query" | tail -1 | tr -d '\r'
}

total=0
while IFS=$'\t' read -r l t; do
  n=$(q "$(count_for "$l")") || { echo "FAIL store did not answer for $l" >&2; exit 1; }
  printf '%-24s -> %-16s %s triples\n' "$l" "$t" "$n"
  total=$((total + n))
done < <(pairs)
echo "total $total triples name an old row"

[ "$MODE" = "--apply" ] || { echo "dry run — nothing written (pass --apply)"; exit 0; }

BK="$ROOT/platform/backups/graph-retirements/4353-step4-$(date -u +%Y%m%dT%H%M%SZ).nq"
mkdir -p "$(dirname "$BK")"
VALS=$(pairs | awk -F'\t' -v c="$C" '{printf "<%s%s> ", c, $1}')
curl -sf --max-time 120 "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -H 'Accept: application/sparql-results+json' \
  --data-urlencode "query=SELECT ?g ?s ?p ?o WHERE { VALUES ?x { $VALS } { GRAPH ?g { ?x ?p ?o BIND(?x AS ?s) } } UNION { GRAPH ?g { ?s ?p ?x } } }" \
  "$DS/query" | python3 -c '
import json, sys
def term(b):
    if b["type"] == "uri": return "<%s>" % b["value"]
    if b["type"] == "bnode": return "_:%s" % b["value"]
    v = json.dumps(b["value"])
    if "xml:lang" in b: return "%s@%s" % (v, b["xml:lang"])
    if "datatype" in b: return "%s^^<%s>" % (v, b["datatype"])
    return v
for r in json.load(sys.stdin)["results"]["bindings"]:
    print(term(r["s"]), term(r["p"]), term(r["o"]), term(r["g"]), ".")
' > "$BK"
# A triple can name two old rows (code-domain consumes time-domain), so the
# backup is checked against the distinct count, not the per-name sum.
distinct=$(q "SELECT (COUNT(*) AS ?n) WHERE { SELECT DISTINCT ?g ?s ?p ?o WHERE { VALUES ?x { $VALS } { GRAPH ?g { ?x ?p ?o BIND(?x AS ?s) } } UNION { GRAPH ?g { ?s ?p ?x } } } }")
lines=$(sort -u "$BK" | wc -l | tr -d ' ')
[ "$lines" -eq "$distinct" ] || { echo "FAIL backup has $lines quads, the store has $distinct — nothing written" >&2; exit 1; }
echo "backup $BK ($lines quads)"

all_updates | curl -sf --max-time "${FUSEKI_WRITE_TIMEOUT:-300}" "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" \
  -H 'Content-Type: application/sparql-update' --data-binary @- "$DS/update" >/dev/null \
  || { echo "FAIL the update was refused; the backup is $BK" >&2; exit 1; }

left=0
while IFS=$'\t' read -r l t; do
  n=$(q "$(count_for "$l")"); left=$((left + n))
  [ "$n" = "0" ] || echo "LEFT $l still named by $n triples"
done < <(pairs)
[ "$left" -eq 0 ] || { echo "FAIL $left triples still name an old row" >&2; exit 1; }
echo "done: 0 triples name an old row"
