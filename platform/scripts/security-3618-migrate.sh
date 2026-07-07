#!/usr/bin/env bash
# #3618 — live-graph migration: chorus:security-trust → chorus:security + the
# security-family model (identity child, Principal, KeyRegistryEntry, APISurface,
# first-class instances, shapes). Same two-mode pattern as products-3603-migrate.sh:
#
#   generate  (default) — read the LIVE graph, emit the typed-slot DEL/INS batch
#               to stdout. Read-only; lets a human eyeball exactly what will land.
#   apply     — POST the generated body to the governed door (owl-api /batch).
#               REFUSES unless CHORUS_WRITE_DOOR_TOKEN is set (mint via
#               chorus-mint-token.py, scope urn:chorus:ontology). Never writes
#               to Fuseki directly. Requires the #3609 body fix (batch > 4KB).
#
# What it migrates (source of truth: THIS branch's TTLs, post-rename):
#   1. DELETE every triple of chorus:security-trust, outbound and inbound
#      (44 counted live 2026-07-04: node statements + borgProduct hasDomain +
#      8 domains' consumes edges).
#   2. INSERT the renamed chorus:security node + repointed edges + the new
#      identity/Principal/KeyRegistryEntry/APISurface vocabulary, first-class
#      surface instances, and SHACL shapes from security-model-3618.ttl.
#
# Tests: platform/tests/security-3618-migration.bats — shape tests run pre-apply;
# done-state tests are RED until apply lands (definition of done).

set -euo pipefail

NS="https://jeffbridwell.com/chorus#"
SPARQL="http://localhost:3030/pods/sparql"
DOOR="http://localhost:3360/batch"
GRAPH="urn:chorus:ontology"
REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
RETIRED="security-trust"

sparql_select() { # $1 = query -> TSV rows without header
  curl -s --max-time 15 "$SPARQL" --data-urlencode "query=$1" \
    -H "Accept: text/tab-separated-values" | tail -n +2
}

generate() {
  # 1. Outbound triples of the retired subject: DEL <s> <p> ?o per predicate.
  sparql_select "SELECT DISTINCT ?p WHERE { GRAPH <$GRAPH> { <$NS$RETIRED> ?p ?o } }" \
    | while IFS=$'\t' read -r p; do
        [ -n "$p" ] && printf 'DEL\t<%s%s>\t%s\t?o\n' "$NS" "$RETIRED" "$p"
      done
  # Inbound edges: DEL <s> <p> <retired> (ground object).
  sparql_select "SELECT DISTINCT ?s ?p WHERE { GRAPH <$GRAPH> { ?s ?p <$NS$RETIRED> } }" \
    | while IFS=$'\t' read -r s p; do
        [ -n "$s" ] && printf 'DEL\t%s\t%s\t<%s%s>\n' "$s" "$p" "$NS" "$RETIRED"
      done

  # 2a. INSERT the new security-family vocabulary — EVERY triple authored in
  #     security-model-3618.ttl. This file contains ONLY this card's subjects,
  #     so no broad prefix-matching against chorus.ttl (which would drag in
  #     unrelated live subjects like gate-product / gate-tdd — found + fixed
  #     2026-07-04). One exception: the two pre-existing base shapes are NOT in
  #     this file, so nothing to exclude here.
  riot --syntax=turtle --output=ntriples \
      "$REPO_ROOT/roles/silas/ontology/security-model-3618.ttl" 2>/dev/null \
    | while read -r s p o_rest; do
        o="${o_rest% .}"
        case "$o" in \"*\;*\") continue ;; esac   # door refuses ';' in literals
        printf 'INS\t%s\t%s\t%s\n' "$s" "$p" "$o"
      done

  # 2b. Repointed INBOUND edges only: every edge whose OBJECT was security-trust,
  #     now pointing at <security> (the 8 domains' consumes + borgProduct
  #     hasDomain). OBJECT-position match ONLY — the node's own body comes from
  #     2a (the model file), so matching subject-position here would double-
  #     define it (found + fixed 2026-07-04: 9 dup lines).
  riot --syntax=turtle --output=ntriples \
      "$REPO_ROOT/roles/silas/ontology/chorus.ttl" \
      "$REPO_ROOT/roles/wren/ontology/domains-wren-silas.ttl" 2>/dev/null \
    | grep -E " <${NS}security> \.\$" \
    | grep -vE "^<${NS}security> " \
    | while read -r s p o_rest; do
        o="${o_rest% .}"
        case "$o" in \"*\;*\") continue ;; esac
        printf 'INS\t%s\t%s\t%s\n' "$s" "$p" "$o"
      done

  # 3. FK note for the reviewer: the DEL sweep is live-derived, so every inbound
  #    ref that exists at generate time is in the batch by construction.
  local leftover
  leftover=$(sparql_select "SELECT (COUNT(*) as ?c) WHERE { GRAPH <$GRAPH> { ?s ?p <$NS$RETIRED> . FILTER(?s != <$NS$RETIRED>) } }" | tr -d '\r')
  echo "# fk-check: $leftover inbound refs to $RETIRED live now — DEL sweep is live-derived, all covered" >&2
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
      && echo "security-3618 migration applied via the door" \
      || { echo "door refused or failed — nothing partial: batch is atomic" >&2; exit 1; }
    ;;
  *) echo "usage: $0 [generate|apply]" >&2; exit 2 ;;
esac
