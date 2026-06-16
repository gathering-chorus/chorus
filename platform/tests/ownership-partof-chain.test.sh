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
[ -f "$TTL" ] || { test_fail "chorus.ttl missing at $TTL"; echo "=== Results: $PASS passed, $FAIL failed ==="; exit 1; }

python3 - "$TTL" <<'PY'
import sys, re
ttl = open(sys.argv[1], encoding="utf-8", errors="replace").read()

PASS = FAIL = 0
def p(m):
    global PASS; PASS += 1; print(f"  PASS: {m}")
def f(m):
    global FAIL; FAIL += 1; print(f"  FAIL: {m}")

# (1) partOf defined as an ObjectProperty
if re.search(r"chorus:partOf\s+a\s+owl:ObjectProperty", ttl):
    p("chorus:partOf is a defined owl:ObjectProperty")
else:
    f("chorus:partOf is not defined as owl:ObjectProperty")

# Collect partOf triples in the simple one-line form: chorus:S chorus:partOf chorus:O .
edges = re.findall(r"chorus:([\w-]+)\s+chorus:partOf\s+chorus:([\w-]+)\s*\.", ttl)
parent = {}
multi = []
for s, o in edges:
    if s in parent:
        multi.append(s)
    parent[s] = o  # last wins; multi tracked separately

# (2) single-parent invariant
if not multi:
    p(f"single-parent invariant holds ({len(parent)} nodes carry exactly one partOf)")
else:
    f(f"nodes with >1 partOf (violates single-parent): {sorted(set(multi))}")

# ValueStream instances (chain terminals)
vs = set(re.findall(r"chorus:([\w-]+)\s+a\s+chorus:ValueStream", ttl))

def walk(node):
    seen = []
    cur = node
    while cur in parent:
        cur = parent[cur]
        if cur in seen:  # cycle guard
            return seen, None
        seen.append(cur)
    return seen, cur

# (3) two real nodes resolve up to a ValueStream
for node in ("gates-service", "observability-domain"):
    chain, top = walk(node)
    if chain and top in vs:
        p(f"{node} → {' → '.join(chain)} (terminates at ValueStream {top})")
    else:
        f(f"{node} ownership chain does not reach a ValueStream (got chain={chain}, top={top})")

print(f"::RESULT:: {PASS} passed, {FAIL} failed")
sys.exit(0 if FAIL == 0 else 1)
PY
rc=$?

echo ""
if [ "$rc" -eq 0 ]; then echo "=== Results: PASS ==="; else echo "=== Results: FAIL ==="; fi
exit $rc
