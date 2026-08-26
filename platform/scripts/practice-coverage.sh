#!/usr/bin/env bash
# practice-coverage.sh — #3754: the loom coverage query.
#
# Two questions the model can now answer, because PracticeShape forces every
# expresses edge to resolve to a live Principle:
#
#   1. Which principles have NO practice expressing them?   (abstract claims)
#   2. Which practices have NO enactedBy?                   (declared, not enacted)
#
# Neither is a failure. Both are gaps we choose to see rather than hide — a
# principle nobody enacts is aspiration, and a practice nothing enforces is
# voluntary. The point is that the numbers are queryable instead of asserted.
#
# A third check IS a failure and exits 1: a dangling expresses target. The
# shape refuses it at deploy; this catches anything that reached the store by
# another door.
#
# Usage: practice-coverage.sh [--json]
set -uo pipefail

FUSEKI_QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
PRACTICES_GRAPH="${PRACTICES_GRAPH:-urn:chorus:domains:practices}"
PRINCIPLES_GRAPH="${PRINCIPLES_GRAPH:-urn:chorus:domains:principles}"
JSON=0
[ "${1:-}" = "--json" ] && JSON=1

ask() {
  curl -s -G "$FUSEKI_QUERY" --data-urlencode "query=$1" -H 'Accept: text/csv' 2>/dev/null
}

P='PREFIX chorus: <https://jeffbridwell.com/chorus#>'

# 1 — principles with no practice expressing them
UNEXPRESSED=$(ask "$P SELECT ?p WHERE {
  GRAPH <$PRINCIPLES_GRAPH> { ?p a chorus:Principle }
  FILTER NOT EXISTS { GRAPH <$PRACTICES_GRAPH> { ?x chorus:expresses ?p } } } ORDER BY ?p")

# 2 — practices nothing enacts
UNENFORCED=$(ask "$P SELECT ?x WHERE {
  GRAPH <$PRACTICES_GRAPH> { ?x a chorus:Practice }
  FILTER NOT EXISTS { GRAPH <$PRACTICES_GRAPH> { ?x chorus:enactedBy ?v } } } ORDER BY ?x")

# 3 — dangling expresses (a FAILURE, not a gap)
DANGLING=$(ask "$P SELECT ?x ?t WHERE {
  GRAPH <$PRACTICES_GRAPH> { ?x chorus:expresses ?t }
  FILTER NOT EXISTS { GRAPH <$PRINCIPLES_GRAPH> { ?t a chorus:Principle } } } ORDER BY ?x")

rows() { printf '%s' "$1" | tail -n +2 | sed '/^$/d' | sed 's|https://jeffbridwell.com/chorus#||g'; }
count() { rows "$1" | grep -c . | tr -d ' '; }

# An unanswerable store must never read as clean (#3726 single-request-truth):
# a coverage tool that reports zero when nothing answered is the same defect
# class as a suite that reports pass when it never ran.
for r in "$UNEXPRESSED" "$UNENFORCED" "$DANGLING"; do
  if ! printf '%s' "$r" | head -1 | grep -q '^[?a-zA-Z]'; then
    echo "practice-coverage: store did not answer — refusing to report a blind zero" >&2
    exit 2
  fi
done

NU=$(count "$UNEXPRESSED"); NE=$(count "$UNENFORCED"); ND=$(count "$DANGLING")

if [ "$JSON" = "1" ]; then
  printf '{"unexpressed_principles":%s,"unenforced_practices":%s,"dangling_expresses":%s}\n' "$NU" "$NE" "$ND"
else
  echo "practice coverage — principles without a practice: $NU"
  rows "$UNEXPRESSED" | sed 's/^/  /'
  echo "practice coverage — practices nothing enacts (declared, not enacted): $NE"
  rows "$UNENFORCED" | sed 's/^/  /'
  echo "practice coverage — DANGLING expresses targets: $ND"
  rows "$DANGLING" | sed 's/^/  /'
fi

[ "$ND" -eq 0 ] || exit 1
