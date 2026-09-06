#!/usr/bin/env bash
# ownership-partof-chain.test.sh — #3450 (red-first, DEC-1674)
#
# The single-parent ownership edge: chorus:partOf, ONE edge used identically at
# every level (Jeff's dead-simple steer 2026-06-16). This is the model-half:
# define the edge + seed a coherent slice so a node's ownership chain resolves
# by walking partOf upward to a ValueStream — the totally-ordered chain #3437's
# pure core consumes. Hermetic: parses the TTL, no Fuseki.
#
# Asserts: (1) partOf is a defined ObjectProperty; (2) single-parent invariant —
# no node declares more than one partOf; (3) walking partOf from two real nodes
# reaches a chorus:ValueStream (no orphan, terminates at the top).
#
# Run: bash platform/tests/ownership-partof-chain.test.sh

set -uo pipefail

PASS=0; FAIL=0
test_pass() { echo "  PASS: $1"; ((PASS++)); }
test_fail() { echo "  FAIL: $1"; ((FAIL++)); }


echo "=== ownership partOf chain (#3450 model-half) ==="

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TTL="$REPO_ROOT/roles/silas/ontology/chorus.ttl"
[ -f "$TTL" ] || { test_fail "chorus.ttl missing at $TTL"; echo "=== Results: FAIL ==="; exit 1; }

# #4111 — walk the GRAPH, not a regex over one file.
#
# The old walk collected partOf edges with a regex that only matched the
# one-line form ending in a period. Almost every real edge is written mid-block
# with a semicolon:
#     chorus:ownedBy chorus:role-wren ; chorus:partOf chorus:product-loom ;
# so the parser saw almost none of them and both chains "stopped" one hop in.
# Measured against the live store today, the edges it reported as missing are
# there: werk -> chorus, borgProduct -> chorusProduct.
#
# It also read a single file while the model is authored across many, so a
# correct edge in a sibling file read as an orphan.
#
# The store is the truth about what is served. If it cannot be reached, say
# UNMEASURED — never pass, and never blame the model for a down service.
source "$REPO_ROOT/platform/scripts/fuseki-auth.sh" 2>/dev/null || true
FUSEKI_QUERY="${FUSEKI_QUERY:-http://localhost:3030/pods/query}"

sparql() {
  curl -sf -m 30 "${FUSEKI_AUTH[@]+"${FUSEKI_AUTH[@]}"}" -X POST "$FUSEKI_QUERY" \
    -H 'Content-Type: application/sparql-query' \
    -H 'Accept: application/sparql-results+json' --data "$1" 2>/dev/null
}

if ! sparql 'ASK { ?s ?p ?o }' >/dev/null; then
  echo "  UNMEASURED: the graph store at $FUSEKI_QUERY is unreachable; the"
  echo "              ownership chain was NOT checked."
  echo "=== Results: UNMEASURED ==="
  exit 0
fi

sparql 'PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?s ?o WHERE { GRAPH ?g { ?s chorus:partOf ?o } }' > /tmp/partof-edges.$$.json
sparql 'PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?s WHERE { GRAPH ?g { ?s a chorus:ValueStream } }' > /tmp/partof-vs.$$.json

python3 - "$TTL" "/tmp/partof-edges.$$.json" "/tmp/partof-vs.$$.json" <<'PY'
import sys, re, json
ttl = open(sys.argv[1], encoding="utf-8", errors="replace").read()

PASS = FAIL = 0
def p(m):
    global PASS; PASS += 1; print(f"  PASS: {m}")
def f(m):
    global FAIL; FAIL += 1; print(f"  FAIL: {m}")

def local(u):
    return u.rsplit("#", 1)[-1]

# (1) partOf defined as an ObjectProperty — a statement about the model TEXT,
# so it stays a text check.
if re.search(r"chorus:partOf\s+a\s+owl:ObjectProperty", ttl):
    p("chorus:partOf is a defined owl:ObjectProperty")
else:
    f("chorus:partOf is not defined as owl:ObjectProperty")

rows = json.load(open(sys.argv[2]))["results"]["bindings"]
vs = {local(b["s"]["value"]) for b in json.load(open(sys.argv[3]))["results"]["bindings"]}

parent, multi = {}, []
for b in rows:
    s, o = local(b["s"]["value"]), local(b["o"]["value"])
    if s in parent and parent[s] != o:
        multi.append(s)
    parent[s] = o

if not multi:
    p(f"single-parent invariant holds ({len(parent)} nodes carry exactly one partOf)")
else:
    f(f"nodes with >1 partOf (violates single-parent): {sorted(set(multi))}")

def walk(node):
    seen, cur = [], node
    while cur in parent:
        cur = parent[cur]
        if cur in seen:
            return seen, None
        seen.append(cur)
    return seen, cur

for node in ("gates-service", "observability-domain"):
    chain, top = walk(node)
    if chain and top in vs:
        p(f"{node} -> {' -> '.join(chain)} (terminates at ValueStream {top})")
    else:
        f(f"{node} ownership chain does not reach a ValueStream (got chain={chain}, top={top})")

print(f"::RESULT:: {PASS} passed, {FAIL} failed")
sys.exit(0 if FAIL == 0 else 1)
PY
rc=$?
rm -f /tmp/partof-edges.$$.json /tmp/partof-vs.$$.json

echo ""
if [ "$rc" -eq 0 ]; then echo "=== Results: PASS ==="; else echo "=== Results: FAIL ==="; fi
exit $rc
