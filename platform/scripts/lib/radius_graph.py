# radius_graph — the store edges product-radius and domain-radius read (#4700)
# @domain: ontology
"""One raw graph, two views.

fetch_graph() reads the store once into a raw graph: the rows of each class and
the edges between them. A fixture is the same raw graph as JSON, so every flag
below is computed by the same code on a fixture as on prod, never planted.

Edges read (Silas + Wren, #4700 pair, measured 10-22):
  Product hasDomain Domain          product owns domain
  Product consumes Domain           product reads domain
  Domain  hosts Service             domain runs service
  Domain  atStep ValueStreamStep    domain's step; step inStream ValueStream
  Skill   hasDomain Domain          skill's domain
  Pipeline hasStep PipelineStep; StepSkill forStep PipelineStep, callsSkill Skill
  Pipeline automates ValueStream
  Domain  inLayer Layer             the domain's layer (16 of 64 on 10-22)
  Skill   executor "pipeline" + implementedBy <name>   the pipeline the skill runs
Product → value stream is derived through its domains: Product hasValueStream
has 0 rows.
"""
import json
import urllib.parse
import urllib.request

C = "https://jeffbridwell.com/chorus#"
CLASSES = ["Product", "Domain", "Service", "Pipeline", "ValueStream",
           "ValueStreamStep", "Skill", "PipelineStep", "StepSkill"]
PREDS = ["hasDomain", "consumes", "partOf", "hosts", "atStep", "inStream",
         "automates", "hasStep", "forStep", "callsSkill", "ownedBy", "executor",
         "stageOrder", "purpose", "status", "pipelineStatus", "inLayer", "implementedBy"]

KIND = {
    "domain-no-purpose": "no purpose",
    "domain-no-product": "no product lists it",
    "domain-two-products": "listed by two products",
    "domain-under-non-product": "owned through a link from a non-Product",
    "domain-no-step": "no value-stream step",
    "domain-no-layer": "no layer",
    "valuestream-no-product": "no product's domain sits at a step in it",
    "skill-no-pipeline": "runs a pipeline that has no Pipeline row",
    "consumes-own-domain": "consumes a domain it owns",
    "link-to-missing-domain": "links to a name that is not a Domain",
    "skill-missing-domain": "skill's domain is not a Domain",
    "service-unhosted": "no domain hosts it",
    "service-hosted-twice": "hosted by two domains",
    "hosts-non-service": "hosts a machine, not a service",
    "pipeline-no-stream": "automates no value stream",
    "product-no-domains": "owns no domains and has no parts",
    "probe-row": "test probe left in prod",
    "yaml-no-rows": "pipeline file with no Pipeline row",
}


def short(v):
    return v.replace(C, "")


def _sparql(endpoint, q):
    data = urllib.parse.urlencode({"query": f"PREFIX c: <{C}> " + q}).encode()
    req = urllib.request.Request(endpoint, data=data,
                                 headers={"Accept": "application/sparql-results+json"})
    rows = json.load(urllib.request.urlopen(req, timeout=30))["results"]["bindings"]
    return [{k: short(v["value"]) for k, v in r.items()} for r in rows]


def fetch_graph(endpoint):
    """Two bounded queries: rows by class, then the named edges of those rows."""
    vals_t = " ".join(f"c:{t}" for t in CLASSES)
    vals_p = " ".join(f"c:{p}" for p in PREDS)
    types = {t: set() for t in CLASSES}
    for r in _sparql(endpoint, f"SELECT DISTINCT ?s ?t WHERE {{ VALUES ?t {{ {vals_t} }} "
                               f"GRAPH ?h {{ ?s a ?t }} FILTER(STRSTARTS(STR(?s), STR(c:))) }}"):
        types[r["t"]].add(r["s"])
    edges = {p: set() for p in PREDS}
    for r in _sparql(endpoint, f"SELECT DISTINCT ?p ?s ?o WHERE {{ VALUES ?t {{ {vals_t} }} "
                               f"VALUES ?p {{ {vals_p} }} GRAPH ?h {{ ?s a ?t }} GRAPH ?g {{ ?s ?p ?o }} }}"):
        edges[r["p"]].add((r["s"], r["o"]))
    return {"types": {t: sorted(v) for t, v in types.items()},
            "edges": {p: sorted([s, o] for s, o in v) for p, v in edges.items()}}


def is_raw(data):
    return "types" in data and "edges" in data


class Graph:
    def __init__(self, raw):
        self.types = {t: set(raw["types"].get(t, [])) for t in CLASSES}
        self.edges = {p: [tuple(x) for x in raw["edges"].get(p, [])] for p in PREDS}

    def of(self, t):
        return self.types[t]

    def pairs(self, pred, subj=None):
        rows = self.edges[pred]
        return [(s, o) for s, o in rows if subj is None or s in self.types[subj]]

    def first(self, pred, subj=None):
        out = {}
        for s, o in self.pairs(pred, subj):
            out.setdefault(s, o)
        return out

    def owner(self, s):
        return self.first("ownedBy").get(s, "").replace("principal-", "")


def derive(g):
    """Every per-domain fact both views read, plus the issues. Pure: graph in, dict out."""
    domains = g.of("Domain")
    products = g.of("Product")
    services = g.of("Service")
    skills = g.of("Skill")
    issues = []

    def issue(kind, subject, text):
        issues.append({"kind": kind, "subject": subject, "text": text})

    owns = g.pairs("hasDomain", "Product")
    products_of = {}
    for p, d in owns:
        products_of.setdefault(d, set()).add(p)

    # A Domain owned through a hasDomain from something that is not a Product.
    # Skills, docs and code files name a domain as members, not owners, so only
    # a Domain-typed subject claiming another domain counts here.
    for s, d in g.pairs("hasDomain", "Domain"):
        issue("domain-under-non-product", d, f"{s} (a Domain, not a Product) has it as hasDomain")

    step_of = g.first("atStep", "Domain")
    stream_of_step = g.first("inStream", "ValueStreamStep")
    purpose = g.first("purpose", "Domain")
    for d in sorted(domains):
        n = len(products_of.get(d, ()))
        if n == 0:
            issue("domain-no-product", d, "no product lists it (hasDomain)")
        elif n > 1:
            issue("domain-two-products", d, "listed by " + ", ".join(sorted(products_of[d])))
        if d not in purpose:
            issue("domain-no-purpose", d, "no purpose")
        if d not in step_of:
            issue("domain-no-step", d, "no atStep to a value-stream step")

    for p, d in owns:
        if d not in domains:
            issue("link-to-missing-domain", p, f"hasDomain {d}, which is not a Domain row")
    consumes = g.pairs("consumes", "Product")
    for p, d in consumes:
        if d not in domains:
            issue("link-to-missing-domain", p, f"consumes {d}, which is not a Domain row")
        elif (p, d) in set(owns):
            issue("consumes-own-domain", p, f"consumes {d}, which it already owns")

    skill_domains = g.pairs("hasDomain", "Skill")
    for sk, d in skill_domains:
        if d not in domains:
            issue("skill-missing-domain", sk, f"hasDomain {d}, which is not a Domain row")

    layer_of = g.first("inLayer", "Domain")
    for d in sorted(domains):
        if d not in layer_of:
            issue("domain-no-layer", d, "no inLayer")

    pipeline_names = {pl for pl in g.of("Pipeline")} | {pl.replace("pipeline-", "") for pl in g.of("Pipeline")}
    impl = g.first("implementedBy", "Skill")
    for sk, ex in g.pairs("executor", "Skill"):
        if ex == "pipeline" and impl.get(sk, "") not in pipeline_names:
            issue("skill-no-pipeline", sk,
                  f"executor pipeline, implementedBy {impl.get(sk, '') or '(none)'}, which is not a Pipeline row")

    hosts = g.pairs("hosts", "Domain")
    hosted_by = {}
    for d, s in hosts:
        if s not in services:
            issue("hosts-non-service", d, f"hosts {s}, which is not a Service row")
        else:
            hosted_by.setdefault(s, set()).add(d)
    for s in sorted(services):
        n = len(hosted_by.get(s, ()))
        if n == 0:
            issue("service-unhosted", s, "no domain hosts it")
        elif n > 1:
            issue("service-hosted-twice", s, "hosted by " + ", ".join(sorted(hosted_by[s])))

    # pipeline → domain: Pipeline hasStep PipelineStep ← StepSkill forStep; StepSkill callsSkill Skill hasDomain
    step_pipeline = {}
    for pl, ps in g.pairs("hasStep", "Pipeline"):
        step_pipeline.setdefault(ps, set()).add(pl)
    calls = dict(g.pairs("callsSkill", "StepSkill"))
    pipes_of_skill = {}
    for ss, ps in g.pairs("forStep", "StepSkill"):
        if ss in calls:
            pipes_of_skill.setdefault(calls[ss], set()).update(step_pipeline.get(ps, ()))

    automates = dict(g.pairs("automates", "Pipeline"))
    pipe_status = g.first("pipelineStatus", "Pipeline")
    for pl in sorted(g.of("Pipeline")):
        if pl.startswith("pipeline-zz") or "probe" in pl:
            issue("probe-row", pl, f"test probe row left in prod (status {pipe_status.get(pl, '')!r})")
        elif pl not in automates:
            issue("pipeline-no-stream", pl, "automates no value stream")

    by_domain = {}
    # Products reach a value stream only through their domains' steps
    # (Product hasValueStream has 0 rows, 10-22), so a stream no owned domain
    # sits in is unreachable from every product.
    reached = {stream_of_step.get(step_of.get(d, ""), "") for _, d in owns}
    for vs in sorted(g.of("ValueStream")):
        if vs not in reached:
            issue("valuestream-no-product", vs, "no product's domain has atStep to a step in this stream")

    # Only typed Domain rows get a radius; a link to a non-Domain is flagged
    # above (link-to-missing-domain, skill-missing-domain), never rendered.
    for d in sorted(domains):
        sks = sorted(sk for sk, x in skill_domains if x == d)
        step = step_of.get(d, "")
        by_domain[d] = {
            "domain": d,
            "owner": g.owner(d),
            "status": g.first("status", "Domain").get(d, ""),
            "products": sorted(products_of.get(d, ())),
            "step": step,
            "valueStream": stream_of_step.get(step, ""),
            "services": sorted(s for x, s in hosts if x == d),
            "skills": sks,
            "pipelines": sorted({pl for sk in sks for pl in pipes_of_skill.get(sk, ())}),
            "readBy": sorted(p for p, x in consumes if x == d),
            "layer": layer_of.get(d, "").replace("layer-", ""),
        }
    for d, row in by_domain.items():
        flags = {i["kind"] for i in issues if i["subject"] == d}
        flags |= {i["kind"] for i in issues if i["subject"] in row["skills"]}
        row["flags"] = sorted(flags)
    return {"domains": by_domain, "issues": issues, "owns": owns, "consumes": consumes,
            "automates": automates, "pipeStatus": pipe_status}


def tree(lines):
    """[(depth, text), ...] → the ├─ │ └─ tree the card's output shape fixes."""
    out = []
    for i, (depth, text) in enumerate(lines):
        if depth == 0:
            out.append(text)
            continue
        last = True
        for dd, _ in lines[i + 1:]:
            if dd < depth:
                break
            if dd == depth:
                last = False
                break
        prefix = "│  " * (depth - 1)
        out.append(f"{prefix}{'└─' if last else '├─'} {text}")
    return "\n".join(out)


def join(xs):
    return ", ".join(xs) if xs else "—"
