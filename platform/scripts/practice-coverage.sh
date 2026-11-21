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
#
# #5021: the principles come from their door (principles-api, through chorus-api's /owl
# proxy), never from the principles store directly: the principles domain has one door.
set -uo pipefail

FUSEKI_QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"
PRACTICES_GRAPH="${PRACTICES_GRAPH:-urn:chorus:domains:practices}"
PRINCIPLES_URL="${PRINCIPLES_URL:-http://localhost:3340/owl/v1/principles/principles}"
JSON=0
[ "${1:-}" = "--json" ] && JSON=1

ask() {
  curl -s -G "$FUSEKI_QUERY" --data-urlencode "query=$1" -H 'Accept: text/csv' 2>/dev/null
}

P='PREFIX chorus: <https://jeffbridwell.com/chorus#>'

# the principles, from the door: "chorus:<name> chorus:<name> ..." for a VALUES block. Every row,
# one page (limit=1000); a door that holds more than it gave, or a name that cannot be a chorus
# local name, is refused, never dropped (a dropped principle would read as dangling). A door that
# answers with no principles is an empty list, not a silent door.
BODY=$(curl -sf --max-time 15 -G "$PRINCIPLES_URL" --data-urlencode limit=1000 2>/dev/null) || {
  echo "practice-coverage: the principles door did not answer ($PRINCIPLES_URL) — refusing to report a blind zero" >&2
  exit 2
}
PRINCIPLES=$(printf '%s' "$BODY" | python3 -c '
import json, re, sys
d = json.load(sys.stdin)
rows = d.get("data")
if not isinstance(rows, list):
    sys.exit("the door answered without a data list")
if isinstance(d.get("count"), int) and d["count"] > len(rows):
    sys.exit("the door holds %d principles and gave %d" % (d["count"], len(rows)))
names = [r.get("name", "") for r in rows]
bad = [n for n in names if not re.fullmatch(r"[a-z0-9]+(-[a-z0-9]+)*", n)]
if bad:
    sys.exit("names that cannot be read: " + ", ".join(map(repr, bad)))
print("OK " + " ".join("chorus:" + n for n in names))') || {
  echo "practice-coverage: the principles door's answer could not be used — refusing to report a blind zero" >&2
  exit 2
}
PRINCIPLES=${PRINCIPLES#OK }

# 1 — principles with no practice expressing them
UNEXPRESSED=$(ask "$P SELECT ?p WHERE {
  VALUES ?p { $PRINCIPLES }
  FILTER NOT EXISTS { GRAPH <$PRACTICES_GRAPH> { ?x chorus:expresses ?p } } } ORDER BY ?p")

# 2 — practices nothing enacts
UNENFORCED=$(ask "$P SELECT ?x WHERE {
  GRAPH <$PRACTICES_GRAPH> { ?x a chorus:Practice }
  FILTER NOT EXISTS { GRAPH <$PRACTICES_GRAPH> { ?x chorus:enactedBy ?v } } } ORDER BY ?x")

# 3 — dangling expresses (a FAILURE, not a gap)
DANGLING=$(ask "$P SELECT ?x ?t WHERE {
  GRAPH <$PRACTICES_GRAPH> { ?x chorus:expresses ?t }
  FILTER NOT EXISTS { VALUES ?t { $PRINCIPLES } } } ORDER BY ?x")

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
