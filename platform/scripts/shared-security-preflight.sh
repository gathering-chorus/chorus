#!/usr/bin/env bash
# shared-security-preflight.sh (#3835) — capture the identity state as a BASELINE
# before any shared-security move, so "did we lose anything" is a DIFF, not a memory
# (Wren's ask on #3835). READ-ONLY. Snapshots: Principal count, holdsRole triple
# count, the Principal + role IRIs, holdsRole edges, and share-principals.txt
# contents + mtime + sha.
#
# FAIL-LOUD: a store that is down/empty must NOT write a false "zero" baseline — a
# post-move diff would then pass vacuously (the exact hollow-check class). If the
# Principal count is not a positive number, this refuses with exit 2.
set -uo pipefail
EP="${CHORUS_FUSEKI_QUERY:-http://localhost:3030/pods/query}"
SEC="urn:chorus:domains:security"
PFX='PREFIX chorus: <https://jeffbridwell.com/chorus#>'
OUT="${1:-$HOME/.chorus/shared-security-baseline-$(date +%Y%m%dT%H%M%S).txt}"
SPT="${CHORUS_HOME:-$HOME/CascadeProjects/chorus}/config/share-principals.txt"

q() { curl -s --max-time 10 "$EP" -H "Accept: application/sparql-results+json" --data-urlencode "query=$1"; }
scalar() { q "$1" | python3 -c "import sys,json;b=json.load(sys.stdin)['results']['bindings'];print(b[0][list(b[0])[0]]['value'] if b else '')" 2>/dev/null; }

PCOUNT="$(scalar "$PFX SELECT (COUNT(?p) AS ?n) WHERE { GRAPH <$SEC> { ?p a chorus:Principal } }")"
case "$PCOUNT" in
  ''|0|*[!0-9]*) echo "REFUSE: principal_count='$PCOUNT' — store unreachable or empty at $EP; not writing a false baseline (fail-closed)." >&2; exit 2 ;;
esac

{
  echo "# shared-security identity baseline — $(date '+%Y-%m-%d %H:%M:%S %Z')"
  echo "endpoint: $EP"
  echo "principal_count: $PCOUNT"
  echo "holdsRole_triples: $(scalar "$PFX SELECT (COUNT(*) AS ?n) WHERE { GRAPH ?g { ?p chorus:holdsRole ?r } }")"
  echo
  echo "## principal IRIs"
  q "$PFX SELECT ?p WHERE { GRAPH <$SEC> { ?p a chorus:Principal } } ORDER BY ?p" | python3 -c "import sys,json;[print(r['p']['value']) for r in json.load(sys.stdin)['results']['bindings']]" 2>/dev/null
  echo
  echo "## holdsRole edges (principal -> role)"
  q "$PFX SELECT ?p ?r WHERE { GRAPH ?g { ?p chorus:holdsRole ?r } } ORDER BY ?p" | python3 -c "import sys,json;[print(r['p']['value'],'->',r['r']['value']) for r in json.load(sys.stdin)['results']['bindings']]" 2>/dev/null
  echo
  echo "## share-principals.txt"
  if [ -f "$SPT" ]; then
    echo "path: $SPT"
    echo "mtime: $(stat -f '%Sm' "$SPT" 2>/dev/null)"
    echo "sha256: $(shasum -a 256 "$SPT" | awk '{print $1}')"
    echo "--- webids (comments/blanks stripped) ---"
    grep -vE '^\s*#|^\s*$' "$SPT"
  else
    echo "MISSING: $SPT"
  fi
} | tee "$OUT"
echo "baseline written: $OUT" >&2
