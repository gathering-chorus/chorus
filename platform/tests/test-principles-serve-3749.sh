#!/usr/bin/env bash
# #3749 — the three-state proof that /principles serves the RIGHT graph, not
# just "serves" (pair design, Wren+Kade 2026-08-05). RUN_INTEGRATION=true to
# run against the live store + owl-api (the vocab-claim-authority.bats pattern:
# RED on live before the migration lands, GREEN after — the state transition IS
# the proof).
#
#   A (pre-land)  : Principle unserved / domain graph empty → this script FAILS
#   B (post-land) : /principles serves exactly the 14 PC principles
#   C (post-land) : ZERO dangling skos:broader references — the 12 parentage
#                   edges belonged to the 13 RETIRED principle-* subjects
#                   (measured 2026-08-05: hemenway-subject broader edges = 0),
#                   so after retire+migration nothing may point at or from a
#                   retired subject. "Served" and "served correctly" separated.
set -euo pipefail

[ "${RUN_INTEGRATION:-}" = "true" ] || { echo "skip: RUN_INTEGRATION not set (integration test — live store + owl-api)"; exit 0; }

FUSEKI_QUERY="${CHORUS_FUSEKI_QUERY:-http://localhost:3030/pods/query}"
OWL_API="${OWL_API_URL:-http://localhost:3360}"
NS="https://jeffbridwell.com/chorus#"
fail() { echo "FAIL: $*" >&2; exit 1; }

count_q() { # SPARQL count query -> bare integer, fail-closed on no answer
  local q="$1" resp
  resp=$(curl -s "$FUSEKI_QUERY" --data-urlencode "query=$q" -H 'Accept: text/csv' 2>/dev/null)
  printf '%s' "$resp" | head -1 | grep -q '^n' || fail "store did not answer (single-request-truth #3726): $q"
  printf '%s\n' "$resp" | tail -1 | tr -dc '0-9'
}

# B1 — the domain graph holds exactly 14 Principles (the ADR-051 home).
home=$(count_q "PREFIX c: <$NS> SELECT (COUNT(DISTINCT ?p) AS ?n) WHERE { GRAPH <urn:chorus:domains:principles> { ?p a c:Principle } }")
[ "$home" = "14" ] || fail "domain graph urn:chorus:domains:principles holds $home Principles, want 14 (state A = 0 means the migration has not landed)"

# B2 — owl-api SERVES 14 from that graph (reader = writer, one canonical graph).
served=$(curl -s -m 10 "$OWL_API/principles" | python3 -c "import json,sys
d=json.load(sys.stdin)
items=d.get('data', d.get('items', d.get('principles', d if isinstance(d, list) else [])))
print(len(items))" 2>/dev/null) || fail "owl-api /principles did not answer parseable JSON"
[ "$served" = "14" ] || fail "/principles serves $served, want 14 — surface exists but reads the wrong graph (the 2-of-29 class)"

# B3 — SOURCE DISAMBIGUATION (Silas catch): the 14 must exist ONLY in the
# domain graph. With copies in both graphs, served=14 proves nothing about
# which graph the reader uses — dual-tenancy makes the source undecidable.
stale=$(count_q "PREFIX c: <$NS> SELECT (COUNT(DISTINCT ?p) AS ?n) WHERE { GRAPH <urn:chorus:ontology> { ?p a c:Principle . FILTER(STRSTARTS(STR(?p), \"${NS}hemenway-\")) } }")
[ "$stale" = "0" ] || fail "$stale stale hemenway copies remain in urn:chorus:ontology — served-source undecidable while dual-tenant"

# C — zero dangling broader references: nothing points at or from a retired
# principle-* subject, in ANY graph. An edge surviving its subject's retirement
# is invisible rot; this is the check that makes the retire complete.
dangling=$(count_q "PREFIX c: <$NS> PREFIX skos: <http://www.w3.org/2004/02/skos/core#> SELECT (COUNT(*) AS ?n) WHERE { GRAPH ?g { { ?s skos:broader ?o . FILTER(STRSTARTS(STR(?s), \"${NS}principle-\")) } UNION { ?s skos:broader ?o . FILTER(STRSTARTS(STR(?o), \"${NS}principle-\")) } } }")
[ "$dangling" = "0" ] || fail "$dangling skos:broader edge(s) still reference retired principle-* subjects — retire incomplete"

# NEGATIVE PROOF of this script itself (#3734): a bogus expected count must fail.
bogus=$(count_q "PREFIX c: <$NS> SELECT (COUNT(DISTINCT ?p) AS ?n) WHERE { GRAPH <urn:chorus:domains:principles> { ?p a c:NoSuchClass } }")
[ "$bogus" = "0" ] || fail "self-check: NoSuchClass count should be 0, got $bogus"

echo "PASS: /principles serves 14 from urn:chorus:domains:principles, zero dangling broader references"
