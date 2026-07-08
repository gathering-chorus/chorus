#!/usr/bin/env python3
"""#3558 — recover the product model to Jeff's canonical shape (2026-07-08).

Canonical (Jeff, pair scratch /tmp/pair-3558.md "JEFF'S DECISION"):
  parent product-chorus + SEVEN children: athena, loom, werk, borg, clearing,
  convergence, pulse. No chorus-service product (search/memory/knowledge fold
  into Pulse — Jeff 13:31). product-spine retires into clearing.

All mutations ride owl-api's write surface (card AC: no live SPARQL, no shape
bypass): generated per-entity routes for products (their resolved graph is
urn:chorus:instances) and for partOf edges (Domain's declared graph is
urn:chorus:ontology); the ontology-copy deletions go through POST /batch with
an explicit x-target-graph (the one cross-graph door, #3573-scoped).

Default is --dry-run: prints the full derived op plan, writes nothing.
--execute requires CHORUS_WRITE_TOKEN (mint per #3619:
chorus-mint-token.py --scope urn:chorus:instances --scope urn:chorus:ontology;
the secret sources from the realm env and is never echoed) and refuses to run
without today's snapshots (13:29 set: instances 44,036 / ontology 4,606
triples in ~/chorus-graph-snapshots/). Stops at the FIRST failed op — no
partial wreckage; snapshot-restore is the rollback.
"""
import glob
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request

NS = "https://jeffbridwell.com/chorus#"
FUSEKI = os.environ.get("CHORUS_FUSEKI", "http://localhost:3030/pods") + "/sparql"
OWL = os.environ.get("OWL_API", "http://localhost:3360")
ONT = "urn:chorus:ontology"
INS = "urn:chorus:instances"

SEVEN = ["athena", "loom", "werk", "borg", "clearing", "convergence", "pulse"]
PARENT = "product-chorus"
# Displaced instances squatting in the schema room. NOT listed: chorus#gathering
# (a different product entirely — out of this card's scope, flagged in scratch);
# chorusStream/security edge noise + `tests` hasDomain (silas coherence calls).
RETIRE_IN_ONT = [f"product-{n}" for n in
                 ["athena", "loom", "werk", "clearing", "convergence", "pulse", "spine"]] \
                + ["chorusProduct", "borgProduct"]
EDGE_REPOINT = {"chorusProduct": PARENT, "borgProduct": "product-borg",
                "product-spine": "product-clearing"}
# silas zero-orphan check 13:34 — the 3 edges OUTSIDE the 44 mapped, dispositioned:
#   gathering partOf chorusStream        -> DROP (out-of-scope product; not stream-contained)
#   chorusProduct partOf chorusStream    -> dies with the chorusProduct subject-delete (P3)
#   identity partOf security             -> re-point to product-borg (ops/observability, #3628)
EXPLICIT_EDGE_OPS = [
    ("DELETE", "/products/gathering/partof", {"target": "value-stream:chorusStream"}),
    ("DELETE", "/domains/identity/partof", {"target": "domain:security"}),
    ("POST", "/domains/identity/partof", {"target": "product:product-borg"}),
]
# Per-child overrides inside the chorusProduct re-point (one containment edge each,
# partOf everywhere — no parallel contains vocabulary):
#   memory/knowledge/search -> product-pulse (Jeff 13:31: search is part of pulse)
#   tests -> product-werk (silas 13:34: werk owns build->test->deploy)
CHILD_OVERRIDES = {"memory": "product-pulse", "knowledge": "product-pulse",
                   "search": "product-pulse", "tests": "product-werk"}
SCALARS = ["label", "comment", "status", "audience", "gaps",
           "valueProposition", "vision", "purpose"]


def sparql(q):
    data = ("query=" + urllib.parse.quote(q)).encode()
    req = urllib.request.Request(
        FUSEKI, data=data,
        headers={"Content-Type": "application/x-www-form-urlencoded",
                 "Accept": "application/sparql-results+json"})
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)["results"]["bindings"]


def fields_of(local, graph):
    rows = sparql(f"SELECT ?p ?o WHERE {{ GRAPH <{graph}> {{ <{NS}{local}> ?p ?o }} }}")
    out = {}
    for b in rows:
        p = b["p"]["value"].rsplit("#", 1)[-1].rsplit("/", 1)[-1]
        out.setdefault(p, []).append(b["o"]["value"])
    return out


def partof_children(target_local):
    rows = sparql(f"SELECT ?c WHERE {{ GRAPH <{ONT}> "
                  f"{{ ?c <{NS}partOf> <{NS}{target_local}> }} }}")
    return [b["c"]["value"].rsplit("#", 1)[-1] for b in rows]


def call(method, path, body=None, headers=None, token="", execute=False):
    desc = f"{method} {path}" + (f"  {json.dumps(body)[:120]}" if body else "")
    if not execute:
        print("  DRY:", desc)
        return 200, "dry-run"
    h = {"Authorization": f"Bearer {token}", "Content-Type": "application/json"}
    h.update(headers or {})
    req = urllib.request.Request(OWL + path, method=method,
                                 data=json.dumps(body).encode() if body else None,
                                 headers=h)
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return r.status, r.read().decode()[:200]
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode()[:200]


def main():
    execute = "--execute" in sys.argv
    token = os.environ.get("CHORUS_WRITE_TOKEN", "")
    if execute:
        if not glob.glob(os.path.expanduser("~/chorus-graph-snapshots/instances-2026*.nt")):
            sys.exit("REFUSED: no snapshots on disk — reversibility first")
        if not token:
            sys.exit("REFUSED: CHORUS_WRITE_TOKEN not set (mint per the #3619 lane)")

    ops = []  # (method, path, body, extra-headers)

    # P1 — canonical products into Product's resolved room (urn:chorus:instances).
    # Content source: current ontology copies (the model as authored); borg's only
    # V2 copy already lives in instances. DELETE-then-POST = idempotent upsert.
    for n in SEVEN:
        local = f"product-{n}"
        src = fields_of(local, ONT) or fields_of(local, INS)
        if not src:
            sys.exit(f"REFUSED: no source content anywhere for {local}")
        body = {"name": local}
        body.update({f: src[f][0] for f in SCALARS if f in src})
        ops.append(("DELETE", f"/products/{local}", None, None))
        ops.append(("POST", "/products", body, None))
    parent_src = fields_of("chorusProduct", ONT)
    if not parent_src:
        sys.exit("REFUSED: chorusProduct content missing — parent authoring source gone")
    parent = {f: parent_src[f][0] for f in SCALARS if f in parent_src}
    parent.update({"name": PARENT, "label": "Chorus"})  # override AFTER copy
    ops.append(("DELETE", f"/products/{PARENT}", None, None))
    ops.append(("POST", "/products", parent, None))
    for n in SEVEN:  # the parent contains the seven
        ops.append(("POST", f"/products/product-{n}/partof",
                    {"target": f"product:{PARENT}"}, None))

    # P2 — re-point every legacy containment edge (zero-orphan contract, silas count-check).
    edge_count = 0
    for old, new in EDGE_REPOINT.items():
        for c in partof_children(old):
            if c.startswith("product-") or c in RETIRE_IN_ONT:
                continue  # the seven ride P1 parent edges; retired subjects die whole in P3
            target = CHILD_OVERRIDES.get(c, new)
            ops.append(("DELETE", f"/domains/{c}/partof",
                        {"target": f"product:{old}"}, None))
            ops.append(("POST", f"/domains/{c}/partof",
                        {"target": f"product:{target}"}, None))
            edge_count += 1

    for method, path, body in EXPLICIT_EDGE_OPS:  # the 3-edge disposition + tests->werk
        ops.append((method, path, body, None))
        edge_count += 1

    # P3 — retire displaced Product instances from the schema room, via the one
    # cross-graph door: POST /batch, x-target-graph names the graph explicitly.
    # (Batch payload shape confirmed at silas green-check before --execute.)
    for local in RETIRE_IN_ONT:
        ops.append(("POST", "/batch",
                    {"op": "delete-subject", "subject": f"{NS}{local}"},
                    {"x-target-graph": ONT}))

    print(f"#3558 recovery plan — {len(ops)} ops, {edge_count} edge re-points, "
          f"execute={execute}")
    for method, path, body, headers in ops:
        code, msg = call(method, path, body, headers, token, execute)
        if execute:
            print(f"  {code} {method} {path} {msg[:80]}")
            if code >= 400:
                sys.exit(f"STOPPED at first failure (no partial wreckage): "
                         f"{method} {path} -> {code} {msg}")

    # P4 — verify (read-only; runs in both modes). Self-verifiable per Jeff's bar.
    for g in (INS, ONT):
        n = sparql(f"SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE "
                   f"{{ GRAPH <{g}> {{ ?s a <{NS}Product> }} }}")[0]["n"]["value"]
        print(f"verify: Product count in {g}: {n}")
    # silas step-4 contract: 47 accounted, 0 partOf targets outside the 8 canonical
    canon = ", ".join(f"<{NS}product-{n}>" for n in SEVEN) + f", <{NS}{PARENT}>"
    outside = sparql(
        f"SELECT (COUNT(?c) AS ?n) WHERE {{ GRAPH <{ONT}> {{ ?c <{NS}partOf> ?t . "
        f"?t a <{NS}Product> . FILTER(?t NOT IN ({canon})) }} }}"
    )[0]["n"]["value"]
    print(f"verify: partOf edges pointing at non-canonical Products: {outside} (target 0)")
    print("verify targets: instances=8 (seven+parent) · ontology=1 (gathering, out of scope) "
          "· orphans=0 · then curl :3360/products and open /athena/product.html")


if __name__ == "__main__":
    main()
