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

# 5. build_update — the idempotent + SCOPED write (the runtime phantom-node / clobber risk).
chunk = [(m.domain_iri("chorus:cards"), "urn:chorus:file:abc123"),
         (m.domain_iri("chorus:code"),  "urn:chorus:file:abc123")]
upd = m.build_update(chunk)
check("build_update DELETE is scoped to the file IRI as OBJECT",
      "DELETE WHERE" in upd
      and "?d <https://jeffbridwell.com/chorus#contains> <urn:chorus:file:abc123>" in upd)
check("build_update never deletes by a domain object (won't clobber product->domain edges)",
      "#contains> <https://jeffbridwell.com/chorus#" not in upd.split("INSERT DATA")[0])
check("build_update INSERTs both domain->file edges",
      "<https://jeffbridwell.com/chorus#cards> <https://jeffbridwell.com/chorus#contains> <urn:chorus:file:abc123>" in upd
      and "<https://jeffbridwell.com/chorus#code> <https://jeffbridwell.com/chorus#contains> <urn:chorus:file:abc123>" in upd)
check("build_update DELETE precedes INSERT (re-run is idempotent)",
      upd.index("DELETE") < upd.index("INSERT DATA"))

# 6. edges_from — primary + non-primary both emitted; uncovered skipped.
e = m.edges_from([("platform/api/src/handlers/cards.ts",
                   m.domain_iri("chorus:cards"), [m.domain_iri("chorus:code")])])
diris = {d for d, _ in e}
check("edges_from emits primary + non-primary",
      m.domain_iri("chorus:cards") in diris and m.domain_iri("chorus:code") in diris and len(e) == 2)
check("edges_from skips uncovered (primary None)", m.edges_from([("x", None, [])]) == [])

# 7. load_rules normalizes prefixes to a trailing slash (from a temp tree).
import tempfile, json as _json
_tf = tempfile.NamedTemporaryFile("w", suffix=".json", delete=False)
_json.dump({"domains": [{"iri": "chorus:tests", "label": "tests",
                         "hasMapsTo": ["platform/tests", "x/y/"]}]}, _tf); _tf.close()
_pref = {p for p, _, _ in m.load_rules(_tf.name)}
check("load_rules normalizes prefixes to trailing-slash",
      "platform/tests/" in _pref and "x/y/" in _pref)

print(f"\n{'ALL GREEN' if not fails else 'FAILURES: ' + ', '.join(fails)}")
sys.exit(1 if fails else 0)
