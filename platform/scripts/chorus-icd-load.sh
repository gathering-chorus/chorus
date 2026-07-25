#!/usr/bin/env bash
# chorus-icd-load.sh — #3392: chorus owns loading its ICD model data.
#
# Seeds the ICD ontology + 9 instance TTLs from their ADR-041 homes into
# urn:gathering:icd/current through the governed DAL seed verb (#3692 +
# #3392 realm policy) — slot-validated, IRI-preserving, provenance-
# stamped, idempotent. Replaces gathering's FusekiSyncService load path
# for this graph (the paired gathering-side removal is the cross-repo op).
#
# The --base matters: the instance TTLs use relative IRIs that the live
# store resolved against https://jeffbridwell.com/ — seeding without it
# would mint different IRIs and break every consumer (zero-regression AC;
# the 2026-03-20 prefix-rewrite incident is the ancestor of this line).
#
# Usage: chorus-icd-load.sh [--verify-only]
#   --verify-only  print the triple count + the domain list without loading
#                  (the AC evidence surface)

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
FUSEKI="${FUSEKI_URL:-http://localhost:3030/pods}"
GRAPH="urn:gathering:icd/current"
BASE="https://jeffbridwell.com/"
ONTOLOGY="$CHORUS_ROOT/designing/products/athena/domains/domains/icd-ontology.ttl"
INSTANCES_DIR="$CHORUS_ROOT/building/products/convergence/domains/integrations/icd-instances"
# CHORUS_MODEL override lets a werk run its own freshly-built binary before
# the deployed one has the seed/--base surface (deploy lags the land).
CHORUS_MODEL="${CHORUS_MODEL:-$(command -v chorus-model || echo "$CHORUS_ROOT/platform/services/chorus-model/target/release/chorus-model")}"

count_triples() {
  curl -sf --max-time 15 -H "Accept: application/sparql-results+json" \
    --data-urlencode "query=SELECT (COUNT(*) as ?n) WHERE { GRAPH <$GRAPH> { ?s ?p ?o } }" \
    "$FUSEKI/query" | python3 -c "import json,sys; print(json.load(sys.stdin)['results']['bindings'][0]['n']['value'])"
}

domain_list() {
  curl -sf --max-time 15 -H "Accept: application/sparql-results+json" \
    --data-urlencode "query=SELECT ?s WHERE { GRAPH <$GRAPH> { ?s a <urn:gathering:icd#Domain> } } ORDER BY ?s" \
    "$FUSEKI/query" | python3 -c "import json,sys; [print(b['s']['value']) for b in json.load(sys.stdin)['results']['bindings']]"
}

echo "== before: $(count_triples) triples in <$GRAPH>"

if [ "${1:-}" = "--verify-only" ]; then
  domain_list
  exit 0
fi

[ -f "$ONTOLOGY" ] || { echo "FATAL: ontology TTL missing at $ONTOLOGY" >&2; exit 1; }
[ -d "$INSTANCES_DIR" ] || { echo "FATAL: instances dir missing at $INSTANCES_DIR" >&2; exit 1; }

# ONE batch across all 10 files. Subjects span files (icd/domain/notes and
# its provider live in both notes and notes-v2) — per-file seeding would let
# the later file's replace-subject wipe the earlier file's edges (12 triples
# lost in the first live run, caught by the zero-regression diff). Concatenate
# to canonical N-Triples first so seed groups each subject across ALL files
# and the union loads in one all-or-nothing transaction.
COMBINED="$(mktemp -t icd-combined).nt"
trap 'rm -f "$COMBINED"' EXIT
{
  riot --base="$BASE" --output=ntriples "$ONTOLOGY"
  for f in "$INSTANCES_DIR"/icd-instance-*.ttl; do
    riot --base="$BASE" --output=ntriples "$f"
  done
} | sort -u > "$COMBINED"
echo "== combined: $(wc -l < "$COMBINED" | tr -d ' ') distinct triples from 10 files"
"$CHORUS_MODEL" seed --kind domain --ttl "$COMBINED" --graph "$GRAPH" --provenance migrated

echo "== after: $(count_triples) triples in <$GRAPH>"
echo "== domains:"
domain_list
