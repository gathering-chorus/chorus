#!/usr/bin/env bash
# @test-type: integration — reads the live store to compare each shape's declared
# chorus:instancesGraph against where its rows actually are. Reports UNMEASURED
# (exit 2) when the store is unreachable; a box-dependent check that goes green on
# an unreachable store says clean when it looked at nothing.
# #4237 — a shape's declared instance home must be where its rows actually live.
#
# chorus:instancesGraph tells the generated route which graph to read. When it
# names a graph the rows are not in, the route answers [] — a normal-looking empty
# answer, not an error. That is what emptied /api/athena/subdomains, /products and
# (for twenty minutes today, by my own hand) /domains/domains.
#
# Silas measured eight classes in this state on 2026-09-21: Domain (mine, fixed),
# APISurface, Gate, Property, PropertyKey, GovernanceCheck, Metric, EmitContract —
# 87 rows declaring a home none of them occupy.
#
# This is a STORE check, so it reports UNMEASURED (exit 2) when it cannot reach
# Fuseki. A box-dependent check that reports green on an unreachable store is
# worse than no check: it says "clean" when it looked at nothing.

set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FUSEKI="${CHORUS_FUSEKI:-http://localhost:3030/pods/query}"

if ! curl -sf --max-time 5 -H 'Accept: text/csv' --data-urlencode 'query=ASK {}' "$FUSEKI" >/dev/null 2>&1; then
  echo "UNMEASURED: cannot reach the store at $FUSEKI — nothing was checked." >&2
  exit 2
fi

q() { curl -sf --max-time 20 -H 'Accept: text/csv' --data-urlencode "query=$1" "$FUSEKI" 2>/dev/null | tail -n +2 | tr -d '\r'; }

# Every shape that declares a home, with the class it targets.
DECLS=$(q 'PREFIX c: <https://jeffbridwell.com/chorus#>
PREFIX sh: <http://www.w3.org/ns/shacl#>
SELECT ?cls ?home WHERE { GRAPH ?g { ?shape c:instancesGraph ?home ; sh:targetClass ?cls } }')

if [ -z "$DECLS" ]; then
  echo "UNMEASURED: no shape declares chorus:instancesGraph — the query found nothing to check." >&2
  exit 2
fi

pass=0; fail=0; empty=0
while IFS=, read -r CLS HOME; do
  [ -n "$CLS" ] || continue
  SHORT="${CLS##*#}"
  # where do this class's rows actually live?
  ACTUAL=$(q "PREFIX c: <https://jeffbridwell.com/chorus#>
    SELECT ?g WHERE { GRAPH ?g { ?s a <$CLS> } } GROUP BY ?g")
  if [ -z "$ACTUAL" ]; then
    # No rows anywhere: nothing to contradict. Counted and named, not passed
    # silently — a class with no rows is its own thing to look at.
    empty=$((empty+1)); echo "  ---- $SHORT: no rows in any graph (declared $HOME)"
    continue
  fi
  if echo "$ACTUAL" | grep -qxF "$HOME"; then
    pass=$((pass+1)); echo "  PASS: $SHORT rows are in its declared home $HOME"
  else
    fail=$((fail+1))
    echo "  FAIL: $SHORT declares $HOME but its rows live in: $(echo "$ACTUAL" | paste -sd' ' -)"
  fi
done <<< "$DECLS"

echo
echo "Result: $pass passed, $fail failed, $empty class(es) with no rows"
[ "$fail" -eq 0 ]
