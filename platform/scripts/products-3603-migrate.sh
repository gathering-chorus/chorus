#!/usr/bin/env bash
# #3603 — live-graph migration for the V1 product-layer retirement. STAGED, not
# yet runnable end-to-end: the write path is the governed /batch door (#3573),
# which is built and review-passed but NOT merged until Jeff's /cw on that card.
# This script therefore has two modes:
#
#   generate  (default) — read the LIVE graph, emit the full typed-slot batch
#               body (DEL/INS tab-delimited lines) to stdout. Read-only. Lets a
#               human review exactly what the migration will do.
#   apply     — POST the generated body to the door (owl-api /batch) with a
#               scoped Bearer token. REFUSES unless CHORUS_WRITE_DOOR_TOKEN is
#               set (mint via chorus-mint-token.py, scope urn:chorus:ontology).
#               Never writes to Fuseki directly — the door is the only write path.
#
# What it migrates (source of truth: roles/silas/ontology/chorus.ttl +
# designing/data/product-instances.ttl + roles/*/ontology/domains-*.ttl in this
# branch — the live graph is post-wipe wreckage and is NOT the source):
#   1. DELETE every triple of the retired subjects, in and out:
#      loom, clearing-product, quality-product, werk-product, athena-product,
#      convergence-product (SubProduct instances), product-borg (dup of
#      borgProduct), and every chorusProduct hasSubProduct edge (falls out of
#      the inbound sweep on each retired subject).
#   2. DELETE the stale partOf of each re-parented V2 domain (functional prop).
#   3. INSERT the product layer as authored: type + floor + partOf + hasDomain
#      for chorusProduct, borgProduct, gathering, product-loom/athena/werk/
#      clearing/spine/pulse/convergence, and the new pulse domain.
#
# Known door constraint (flagged to Silas): is_literal_term refuses ';' inside
# literals. All #3603-authored literals are semicolon-free; three pre-existing
# #3545 literals (product-athena/werk/loom) contain ';' but are already live —
# they are not re-inserted here.

set -euo pipefail

NS="https://jeffbridwell.com/chorus#"
SPARQL="http://localhost:3030/pods/sparql"
DOOR="http://localhost:3360/batch"
GRAPH="urn:chorus:ontology"
REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

RETIRED=(loom clearing-product quality-product werk-product athena-product convergence-product product-borg)

sparql_select() { # $1 = query -> TSV rows without header
  curl -s --max-time 15 "$SPARQL" --data-urlencode "query=$1" \
    -H "Accept: text/tab-separated-values" | tail -n +2
}

generate() {
  # 1. Outbound triples of retired subjects: DEL <s> <p> ?o per distinct predicate.
  for r in "${RETIRED[@]}"; do
    sparql_select "SELECT DISTINCT ?p WHERE { GRAPH <$GRAPH> { <$NS$r> ?p ?o } }" \
      | while IFS=$'\t' read -r p; do
          [ -n "$p" ] && printf 'DEL\t<%s%s>\t%s\t?o\n' "$NS" "$r" "$p"
        done
    # Inbound edges: DEL <s> <p> <retired> (ground object).
    sparql_select "SELECT DISTINCT ?s ?p WHERE { GRAPH <$GRAPH> { ?s ?p <$NS$r> } }" \
      | while IFS=$'\t' read -r s p; do
          [ -n "$s" ] && printf 'DEL\t%s\t%s\t<%s%s>\n' "$s" "$p" "$NS" "$r"
        done
  done

  # 2. Stale partOf on re-parented V2 domains (functional — one parent).
  for d in roles principles policies practices skills decisions rcas domains services messages streams spine cards; do
    printf 'DEL\t<%s%s>\t<%spartOf>\t?o\n' "$NS" "$d" "$NS"
  done
  # borgProduct re-parents chorusStream -> chorusProduct
  printf 'DEL\t<%sborgProduct>\t<%spartOf>\t?o\n' "$NS" "$NS"

  # 3. INSERT the authored product layer from THIS branch's source files.
  #    riot flattens to N-Triples; filter to the product-layer subjects.
  riot --syntax=turtle --output=ntriples \
      "$REPO_ROOT/roles/silas/ontology/chorus.ttl" \
      "$REPO_ROOT/designing/data/product-instances.ttl" \
      "$REPO_ROOT/roles/wren/ontology/domains-wren-silas.ttl" \
    | grep -E "^<${NS}(product-[a-z]+|chorusProduct|borgProduct|gathering|pulse|roles|principles|policies|practices|skills|decisions|rcas|domains|services|messages|streams|spine|cards)> " \
    | while read -r s p o_rest; do
        o="${o_rest% .}"
        # keep only product-layer-relevant predicates (type/floor/edges)
        case "$p" in
          "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>"|"<http://www.w3.org/2000/01/rdf-schema#label>"|"<http://www.w3.org/2000/01/rdf-schema#comment>"|"<${NS}label>"|"<${NS}comment>"|"<${NS}ownedBy>"|"<${NS}vision>"|"<${NS}valueProposition>"|"<${NS}audience>"|"<${NS}status>"|"<${NS}gaps>"|"<${NS}atStep>"|"<${NS}partOf>"|"<${NS}hasDomain>"|"<${NS}consumes>"|"<${NS}runsOn>"|"<${NS}purpose>"|"<${NS}atStream>"|"<${NS}repoTarget>"|"<${NS}definesVocabulary>")
            # skip literals the door refuses (pre-existing #3545 semicolon strings — already live)
            case "$o" in \"*\;*\") continue ;; esac
            printf 'INS\t%s\t%s\t%s\n' "$s" "$p" "$o"
            ;;
        esac
      done
}

case "${1:-generate}" in
  generate) generate ;;
  apply)
    : "${CHORUS_WRITE_DOOR_TOKEN:?refusing: mint a scoped token first (chorus-mint-token.py --scope $GRAPH)}"
    body="$(generate)"
    [ -n "$body" ] || { echo "refusing: generated empty batch" >&2; exit 1; }
    printf '%s' "$body" | curl -sf -X POST "$DOOR" \
      -H "Authorization: Bearer $CHORUS_WRITE_DOOR_TOKEN" \
      -H "x-target-graph: $GRAPH" \
      --data-binary @- \
      && echo "migration applied via the door" \
      || { echo "door refused or failed — nothing partial: batch is atomic" >&2; exit 1; }
    ;;
  *) echo "usage: $0 [generate|apply]" >&2; exit 2 ;;
esac
