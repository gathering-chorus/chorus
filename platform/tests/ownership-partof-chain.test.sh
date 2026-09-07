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
# #4113 — this read ONE file, roles/silas/ontology/chorus.ttl, with a regex that only
# matched a standalone one-line triple ending in a period. The model has been spread
# across per-role ontology files for months and most edges are authored inside a
# predicate list (`... ; chorus:partOf chorus:x ;`). So the walk stopped at `werk` and
# `borgProduct` and reported their chains as broken, while the live graph has
# werk partOf chorus and borgProduct partOf chorusProduct — measured 2026-09-07.
# It was reporting on the file it could see, not on the model.
# Products and value streams are authored under designing/data (the INSTANCE_SET), not
# under roles/. Reading only roles/ left chorus:werk invisible, so gates-service's chain
# appeared to dead-end at a Product with no parent — while product-instances.ttl:468
# defines it. The model is every authored .ttl, not one directory.
TTLS=$( { find "$REPO_ROOT/roles" -name "*.ttl" -not -path "*/node_modules/*";
          find "$REPO_ROOT/designing/data" -name "*.ttl" -not -path "*/node_modules/*" 2>/dev/null; } | sort)
[ -n "$TTLS" ] || { test_fail "no ontology .ttl found under $REPO_ROOT/roles"; echo "=== Results: $PASS passed, $FAIL failed ==="; exit 1; }

python3 - $TTLS <<'PY'
import sys, re
ttl = "\n".join(open(f, encoding="utf-8", errors="replace").read() for f in sys.argv[1:])

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
# #4113 — subject-scoped scan. A subject block runs from `chorus:name a ...` to the
# terminating period, and partOf may sit anywhere inside it, separated by `;`. The old
# pattern required the triple to be its own one-line statement, which is the least
# common way any of this is actually written.
edges = []
for m in re.finditer(r"^chorus:([\w-]+)\s+a\s+(.*?)(?<!\\)\s\.\s*$", ttl, re.S | re.M):
    subj, block = m.group(1), m.group(2)
    for po in re.findall(r"chorus:partOf\s+chorus:([\w-]+)", block):
        edges.append((subj, po))
# plus the standalone one-line form the original looked for
edges += re.findall(r"^chorus:([\w-]+)\s+chorus:partOf\s+chorus:([\w-]+)\s*\.", ttl, re.M)
edges = list(dict.fromkeys(edges))
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
