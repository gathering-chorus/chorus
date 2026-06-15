#!/usr/bin/env python3
"""
#3426 — file→domain herald unit tests. Run: python3 3426-file-domain-herald.py
The critical test is file_iri MATCHING crawler-hydrate-graph.sh's scheme —
a mismatch means edges point at phantom nodes and #3420 stays dark.
"""
import importlib.util, os, sys

ENG = os.path.join(os.path.dirname(__file__), "..", "scripts", "crawler-facet-file-domain.py")
spec = importlib.util.spec_from_file_location("facet", ENG)
m = importlib.util.module_from_spec(spec)
# pin CHORUS_ROOT so file_iri is deterministic for the known-hash assertion
os.environ["CHORUS_ROOT"] = "/Users/jeffbridwell/CascadeProjects/chorus"
m.CHORUS_ROOT = "/Users/jeffbridwell/CascadeProjects/chorus"
spec.loader.exec_module(m)
m.CHORUS_ROOT = "/Users/jeffbridwell/CascadeProjects/chorus"

fails = []
def check(name, cond):
    print(("PASS" if cond else "FAIL"), name)
    if not cond: fails.append(name)

# 1. file IRI matches crawler-hydrate-graph.sh: sha1(absolute path), urn:chorus:file: prefix.
#    Known-good from the grounding sweep: platform/api/src/server.ts -> 28880f65...
check("file_iri matches hydrator scheme (server.ts → known sha1)",
      m.file_iri("platform/api/src/server.ts")
      == "urn:chorus:file:28880f65b252a046d805deeb7676b467d7f1872e")

# 2. domain IRI: tree 'chorus:cards' -> full https#cards
check("domain_iri expands chorus: prefix",
      m.domain_iri("chorus:cards") == "https://jeffbridwell.com/chorus#cards")

# 3. longest-prefix-wins = primary; shorter match = non-primary edge.
rules = [
    ("platform/", m.domain_iri("chorus:code"), "code"),
    ("platform/api/src/handlers/cards.ts", m.domain_iri("chorus:cards"), "cards"),
]
# engine normalizes prefixes with trailing slash internally; emulate load_rules norm:
rules = [(p if p.endswith("/") else p + "/", d, l) for (p, d, l) in rules]
res = m.attribute(["platform/api/src/handlers/cards.ts", "platform/foo.ts"], rules)
by_file = {f: (prim, non) for f, prim, non in res}
prim, non = by_file["platform/api/src/handlers/cards.ts"]
check("longest-prefix-wins: cards.ts primary = cards",
      prim == m.domain_iri("chorus:cards"))
check("shorter match kept as non-primary edge: cards.ts also part-of code",
      m.domain_iri("chorus:code") in non)
prim2, _ = by_file["platform/foo.ts"]
check("only-match wins: platform/foo.ts primary = code",
      prim2 == m.domain_iri("chorus:code"))

# 4. no match -> uncovered (honest gauge), not a forced domain.
res2 = m.attribute(["random/unmapped/thing.txt"], rules)
check("unmatched file is uncovered (primary None)", res2[0][1] is None)

print(f"\n{'ALL GREEN' if not fails else 'FAILURES: ' + ', '.join(fails)}")
sys.exit(1 if fails else 0)
