#!/usr/bin/env python3
"""populate-model-chain.py — land the v2 model chain through the DAL (#3350).

The durable, re-runnable form of the spike's population (gate-ops catch: the
77 instances must be reproducible, not session-only). Reads tree.json (the
authored source of record), writes EVERYTHING through chorus-model — never raw
SPARQL — in FK dependency order:

    value-stream-steps  →  roles  →  documents (stub designs)  →  domains

Idempotent by construction: the DAL's replace-subject write means re-running
produces the same triples (audit `created` survives, `modified` advances).

Usage:  python3 populate-model-chain.py [--tree PATH] [--bin PATH]
"""
import json
import os
import subprocess
import sys

ROOT = os.environ.get("CHORUS_ROOT", "/Users/jeffbridwell/CascadeProjects/chorus")
TREE = sys.argv[sys.argv.index("--tree") + 1] if "--tree" in sys.argv else f"{ROOT}/data/athena/tree.json"
BIN = (
    sys.argv[sys.argv.index("--bin") + 1]
    if "--bin" in sys.argv
    else os.path.expanduser("~/.chorus/bin/chorus-model")
)
ENV = {**os.environ, "DEPLOY_ROLE": os.environ.get("DEPLOY_ROLE", "silas")}


def add(kind, name, fields, edges=()):
    args = [BIN, "add", "--kind", kind, "--name", name]
    for k, v in fields.items():
        args += ["--field", f"{k}={v}"]
    for prop, target in edges:
        args += ["--edge", f"{prop}={target}"]
    r = subprocess.run(args, capture_output=True, text=True, env=ENV)
    return r.returncode == 0, r.stderr.strip()


def gaps_string(gaps):
    if isinstance(gaps, list) and gaps:
        return "; ".join(gaps)
    if isinstance(gaps, str) and gaps:
        return gaps
    return "none declared"  # invariant-rendering: absence is DECLARED, never blank


def main():
    d = json.load(open(TREE))
    ok = bad = 0
    errs = []

    # 1. value-stream steps (no dependencies)
    for st in d.get("valueStreamSteps", []):
        name = st["iri"].split(":")[1].replace("value-stream-step-", "")
        s, e = add("value-stream-step", name, {"label": st.get("label") or name})
        ok += s; bad += not s
        if not s: errs.append(f"step {name}: {e[:90]}")

    # 2. roles (no dependencies)
    for role in d.get("roles", []):
        name = role["iri"].split(":")[1].replace("role-", "")
        s, e = add("role", name, {"label": role.get("label") or name,
                                  "comment": f"kind={role.get('kind', 'agent')}"})
        ok += s; bad += not s
        if not s: errs.append(f"role {name}: {e[:90]}")

    # 3. stub design documents (required by DomainShape.hasDesignDoc)
    for dom in d.get("domains", []):
        name = dom["iri"].split(":")[1]
        s, e = add("document", f"{name}-domain-design",
                   {"label": f"{dom.get('label') or name} — Domain Design",
                    "comment": f"STUB design doc, consistent template (#3350): designing/domains/{name}.html"})
        ok += s; bad += not s
        if not s: errs.append(f"doc {name}: {e[:90]}")

    # 4. domains (depend on all of the above — FK order enforced by the DAL)
    for dom in d.get("domains", []):
        name = dom["iri"].split(":")[1]
        owner = (dom.get("ownedBy") or "").split(":")[-1].replace("role-", "")
        step = (dom.get("atStep") or "").split(":")[-1].replace("value-stream-step-", "")
        edges = [("hasDesignDoc", f"document:{name}-domain-design")]
        if owner:
            edges.append(("ownedBy", f"role:{owner}"))
        if step:
            edges.append(("atStep", f"value-stream-step:{step}"))
        s, e = add("domain", name,
                   {"label": dom.get("label") or name,
                    "comment": dom.get("comment") or "",
                    "status": dom.get("status") or "operating",
                    "gaps": gaps_string(dom.get("gaps"))},
                   edges)
        ok += s; bad += not s
        if not s: errs.append(f"dom {name}: {e[:90]}")

    print(f"populate-model-chain: {ok} written, {bad} refused")
    for e in errs[:10]:
        print("  ", e)
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
