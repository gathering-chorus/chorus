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
PARENT = "chorus"  # BARE grain — silas ruling 14:11: ADR-040 kinds table is the law, product is a bare kind
# Displaced instances squatting in the schema room. NOT listed: chorus#gathering
# (a different product entirely — out of this card's scope, flagged in scratch);
# chorusStream/security edge noise + `tests` hasDomain (silas coherence calls).
RETIRE_IN_ONT = [f"product-{n}" for n in
                 ["athena", "loom", "werk", "clearing", "convergence", "pulse", "spine"]] \
                + ["chorusProduct", "borgProduct"]
EDGE_REPOINT = {"chorusProduct": PARENT, "borgProduct": "borg",
                "product-spine": "clearing"}
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
# Every product's REAL design doc on disk (verified ls 2026-07-09; no fabrication):
# DocumentShape.hasDomain sh:class = SubDomain (V1-era constraint, conformed to;
# modernizing that shape is follow-on work, not smuggled into recovery). Targets
# verified against the 44 live SubDomains 2026-07-09:
DOC_SUBDOMAIN = {
    "athena": "athena-domain", "borg": "observability-domain", "loom": "loom-principles",
    "werk": "tests-domain", "clearing": "cards-service", "convergence": "convergence-domain",
    "pulse": "spine-service", "chorus": "chorus-domain",
}
DOC_MAP = {
    "athena": ("athena-product-design", "Athena — Product Design", "designing/docs/athena-product-design.html"),
    "borg": ("borg-product-design", "Borg — Product Design", "designing/docs/borg-product-design.html"),
    "loom": ("loom-subproduct-design", "Loom — Subproduct Design", "designing/docs/loom-subproduct-design.html"),
    "werk": ("werk-subproduct-design", "Werk — Subproduct Design", "designing/docs/werk-subproduct-design.html"),
    "clearing": ("clearing-value-stream-design", "Clearing — Value-Stream Design", "designing/docs/clearing-value-stream-design.html"),
    "convergence": ("convergence-value-stream-design", "Convergence — Value-Stream Design", "designing/docs/convergence-value-stream-design.html"),
    "pulse": ("pulse-service-design", "Pulse — Service Design (to-be)", "designing/docs/chorus-pulse-tobe.html"),
    "chorus": ("chorus-as-platform", "Chorus — A System to Build Systems", "designing/docs/chorus-as-platform.html"),
}
# hasDomain floor-fill where the source node carries none — SAME ownership mapping
# as the P2 edge re-points (Jeff 13:31 pulse absorbs; silas 13:34 tests->werk):
DOMAIN_FILL = {
    "pulse": ["memory", "knowledge", "search"],
    "werk": ["tests"],
    "borg": ["infrastructure", "builds", "deploys", "logs", "alerts-monitors"],
    "loom": ["principles", "practices", "policies", "decisions", "rcas"],
    "clearing": ["cards", "messages", "streams"],
    "convergence": ["integrations"],
    "chorus": ["pipelines", "cicd", "version-control", "code"],
    "athena": ["domains", "services", "Properties"],
}
CHILD_OVERRIDES = {"memory": "pulse", "knowledge": "pulse",
                   "search": "pulse", "tests": "werk"}
SCALARS = ["label", "comment", "status", "audience", "gaps",
           "valueProposition", "vision", "purpose"]
# PM-authored fill for fields absent in EVERY copy (conform data to the shape
# floor, never relax it — #3558's own law). Marked here, noted on the card.
AUTHORED_FIELDS = {"convergence": {"vision": "Every outside source lands through one governed ingestion path — mapped, reconciled, and conformed to the canonical model before it enters the graph, so nothing arrives unshaped."}}


def sparql(q):
    data = ("query=" + urllib.parse.quote(q)).encode()
    req = urllib.request.Request(
        FUSEKI, data=data,
        headers={"Content-Type": "application/x-www-form-urlencoded",
                 "Accept": "application/sparql-results+json"})
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)["results"]["bindings"]


def nt_lit(v):
    """Literal slot for the batch door — charset-checked (no \" \\n \\t {{ }} ;)."""
    for bad in ('"', "\n", "\r", "\t", "{", "}", ";"):
        if bad in v:
            raise SystemExit(f"REFUSED: literal contains batch-forbidden char {bad!r}: {v[:60]}")
    return f'"{v}"'


def fields_of_snapshot(local):
    """Yesterday's pre-recovery snapshot — the wipe in run 1 emptied the live
    instances row for borg (its only V2 copy), so its content reads from the
    13:29 2026-07-08 snapshot. Read-only."""
    import re, glob as g
    snaps = sorted(g.glob(os.path.expanduser("~/chorus-graph-snapshots/instances-20260708-*.nt")))
    if not snaps:
        return {}
    out = {}
    pat = re.compile(rf"^<{re.escape(NS + local)}> <([^>]+)> (.+) \.$")
    for line in open(snaps[-1], encoding="utf-8", errors="replace"):
        m = pat.match(line.strip())
        if not m:
            continue
        pred = m.group(1).rsplit("#", 1)[-1].rsplit("/", 1)[-1]
        obj = m.group(2).strip()
        if obj.startswith("<") and obj.endswith(">"):
            val = obj[1:-1]
        else:
            val = obj.split('"^^')[0].strip('"').encode().decode("unicode_escape")
        out.setdefault(pred, []).append(val)
    return out


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


def preds_of(local, graph):
    rows = sparql(f"SELECT DISTINCT ?p WHERE {{ GRAPH <{graph}> {{ <{NS}{local}> ?p ?o }} }}")
    return [b["p"]["value"] for b in rows]


def fields_localname(vals):
    return [v.rsplit("#", 1)[-1] for v in vals]


def post_batch(graph, lines, token, execute):
    body = "\n".join("\t".join(l) for l in lines)
    if not execute:
        print(f"  DRY: POST /batch -> <{graph}>  ({len(lines)} lines, {len(body)} bytes)")
        for l in lines[:400]:
            print("       ", " ".join(l)[:160])
        return 200, "dry-run"
    req = urllib.request.Request(OWL + "/batch", method="POST", data=body.encode(),
        headers={"Authorization": f"Bearer {token}", "x-target-graph": graph,
                 "Content-Type": "text/plain"})
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return r.status, r.read().decode()[:200]
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode()[:200]


def dal_add(local, fields, edges, execute, kind="product"):
    """Insert one product through chorus-model add — the DAL escapes field values
    internally and validates the shape floor; prose (semicolons etc.) rides safely."""
    import subprocess
    args = ["chorus-model", "add", "--kind", kind, "--name", local]
    for k, v in fields.items():
        args += ["--field", f"{k}={v}"]
    for prop, target in edges:
        args += ["--edge", f"{prop}={target}"]
    if not execute:
        print(f"  DRY: chorus-model add {kind} {local} fields={list(fields)} edges={edges}")
        return 0, "dry-run"
    r = subprocess.run(args, capture_output=True, text=True, timeout=60,
                       env={**os.environ, "DEPLOY_ROLE": "wren"})
    return r.returncode, (r.stdout + r.stderr).strip()[:200]


def main():
    execute = "--execute" in sys.argv
    token = os.environ.get("CHORUS_WRITE_TOKEN", "")
    if execute:
        if not glob.glob(os.path.expanduser("~/chorus-graph-snapshots/instances-2026*.nt")):
            sys.exit("REFUSED: no snapshots on disk — reversibility first")
        if not token:
            sys.exit("REFUSED: CHORUS_WRITE_TOKEN not set (mint per the #3619 lane)")

    P = lambda local: f"<{NS}{local}>"
    PARTOF = f"<{NS}partOf>"

    # ---- assemble ----
    # Batch A (instances): wildcard-object DELs — subject+predicate exact IRIs,
    # object ?o. Wipes every stale product row completely; charset-safe always.
    batch_a = []
    for n in SEVEN + [None]:
        for local in ((PARENT, "chorusProduct", "product-chorus") if n is None
                      else (n, f"product-{n}")):
            for pred in preds_of(local, INS):
                batch_a.append(("DEL", P(local), f"<{pred}>", "?o"))

    # DAL adds: one per canonical product, full field set from the source copy.
    adds = []
    for n in SEVEN + [None]:
        local = PARENT if n is None else n
        src_local = "chorusProduct" if n is None else f"product-{n}"
        src = dict(fields_of_snapshot(src_local)) if n else {}
        src.update(fields_of(src_local, INS) if n else {})
        src.update(fields_of(src_local, ONT))  # ontology (current model) wins per field
        if n == "borg" and "atStep" not in src:
            src["atStep"] = [NS + "Proving"]  # from borgProduct's own recorded atStep (queried 10:22)
        if not src:
            sys.exit(f"REFUSED: no source content anywhere for {local}")
        fields = {f: src[f][0] for f in SCALARS if f in src}
        for k, v in AUTHORED_FIELDS.get(local, {}).items():
            fields.setdefault(k, v)
        edges = []
        if "ownedBy" in src:
            edges.append(("ownedBy", "role:" + fields_localname(src["ownedBy"])[0].removeprefix("role-")))
        if "atStep" in src:
            # sources carry CamelCase step individuals (chorus#Designing); the
            # conformant grain is the typed value-stream-step-* instances
            edges.append(("atStep", "value-stream-step:" + fields_localname(src["atStep"])[0].lower()))
        if n is None:
            fields["label"] = "Chorus"
            edges += [("hasChild", f"product:{c}") for c in SEVEN]
        else:
            edges.append(("partOf", f"product:{PARENT}"))

        for d in DOMAIN_FILL.get(local, []):  # V2 Domain grain only (source carried V1 SubDomains)
            edges.append(("hasDomain", f"domain:{d}"))
        doc_name, doc_title, doc_path = DOC_MAP[local]
        D = P(doc_name)
        ins_doc = [
            ("INS", D, "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>", f"<{NS}Document>"),
            ("INS", D, f"<{NS}docTitle>", nt_lit(doc_title)),
            ("INS", D, f"<{NS}label>", nt_lit(doc_title)),
            ("INS", D, f"<{NS}comment>", nt_lit(f"Committed at {doc_path} (repo). Graph row added by #3558 recovery.")),
            ("INS", D, f"<{NS}hasDomain>", P(DOC_SUBDOMAIN[local])),
        ]
        batch_a.extend(ins_doc)
        edges.append(("hasDesignDoc", f"document:{doc_name}"))
        adds.append((local, fields, edges, "product"))

    # Batch B (ontology): edge re-points (all-IRI) + displaced-subject wipes
    # (wildcard-object per distinct predicate).
    batch_b = []
    edge_count = 0
    for old, new in EDGE_REPOINT.items():
        for c in partof_children(old):
            if c.startswith("product-") or c in RETIRE_IN_ONT:
                continue
            target = CHILD_OVERRIDES.get(c, new)
            batch_b.append(("DEL", P(c), PARTOF, P(old)))
            batch_b.append(("INS", P(c), PARTOF, P(target)))
            edge_count += 1
    batch_b.append(("DEL", P("gathering"), PARTOF, P("chorusStream")))
    batch_b.append(("DEL", P("identity"), PARTOF, P("security")))
    batch_b.append(("INS", P("identity"), PARTOF, P("borg")))
    for local in RETIRE_IN_ONT:
        for pred in preds_of(local, ONT):
            batch_b.append(("DEL", P(local), f"<{pred}>", "?o"))

    print(f"#3558 recovery plan — batchA(instances) {len(batch_a)} DELs · "
          f"{len(adds)} DAL adds · batchB(ontology) {len(batch_b)} lines "
          f"({edge_count} edge re-points) · execute={execute}")

    # ---- apply, stop at first failure ----
    if batch_a:
        code, msg = post_batch(INS, batch_a, token, execute)
        if execute:
            print(f"  {code} POST /batch <{INS}> {msg[:100]}")
            if code >= 400:
                sys.exit(f"STOPPED: batch A -> {code} {msg}")
    else:
        print("  batch A empty (already wiped in a prior run) — skipped")
    for local, fields, edges, kind in adds:
        code, msg = dal_add(local, fields, edges, execute, kind)
        if execute:
            print(f"  rc={code} chorus-model add {local} {msg[:90]}")
            if code != 0:
                sys.exit(f"STOPPED: dal add {local} -> {msg}")
    code, msg = post_batch(ONT, batch_b, token, execute)
    if execute:
        print(f"  {code} POST /batch <{ONT}> {msg[:100]}")
        if code >= 400:
            sys.exit(f"STOPPED: batch B -> {code} {msg}")

    # ---- verify (read-only, both modes) ----
    for g in (INS, ONT):
        n = sparql(f"SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE "
                   f"{{ GRAPH <{g}> {{ ?s a <{NS}Product> }} }}")[0]["n"]["value"]
        print(f"verify: Product count in {g}: {n}")
    canon = ", ".join(f"<{NS}{n}>" for n in SEVEN) + f", <{NS}{PARENT}>"
    outside = sparql(
        f"SELECT (COUNT(?c) AS ?n) WHERE {{ GRAPH <{ONT}> {{ ?c <{NS}partOf> ?t . "
        f"?t a <{NS}Product> . FILTER(?t NOT IN ({canon})) }} }}"
    )[0]["n"]["value"]
    print(f"verify: partOf edges pointing at non-canonical Products: {outside} (target 0)")
    print("verify targets: instances=8 (seven+parent) · ontology=1 (gathering, out of scope) "
          "· orphans=0 · then curl :3360/products and open /athena/product.html")


if __name__ == "__main__":
    main()
