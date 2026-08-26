#!/usr/bin/env python3
"""#4010 — the identity block of a design doc, PROJECTED from the model.

Jeff, 2026-08-25: "writing/updating product and service docs from our owl api."

A design doc's identity — what this thing is called, what it is for, who owns
it, where its code lives — is already in the graph. Retyping it into prose is
how a doc drifts from the system it describes: `service-design-pulse.html` was
15,682 bytes with ZERO references to the model, and its own owner had not read
it in four months.

This emits that block as HTML from athena-make, so the doc cannot claim an
identity the model does not hold. Everything BELOW the block stays hand-written
— judgement, history, open questions. Only identity is generated.

    design-doc-identity.py --class Product --name pulse
    design-doc-identity.py --class Domain  --name memory --check FILE

--check reads an existing doc and exits non-zero when its generated block has
drifted from the model. That is the gate; without it this is a convenience.
"""
import argparse, json, sys, urllib.request

OWL = "http://localhost:3360"
COLLECTION = {"Product": "products", "Domain": "domains", "Service": "services"}
BEGIN = "<!-- BEGIN generated-identity (#4010) — athena-make projection, do not hand-edit -->"
END = "<!-- END generated-identity -->"
FIELDS = ["label", "comment", "purpose", "ownedBy", "audience", "repoTarget", "partOf"]


def fetch(cls, name):
    coll = COLLECTION.get(cls)
    if not coll:
        sys.exit(f"design-doc-identity: unknown class {cls!r} (expected {'/'.join(COLLECTION)})")
    url = f"{OWL}/{coll}/{name}"
    try:
        with urllib.request.urlopen(url, timeout=10) as r:
            body = json.load(r)
    except Exception as e:
        # Fail loud. A doc built from a silent fallback would carry an identity
        # nobody can trace, which is the failure this script exists to end.
        sys.exit(f"design-doc-identity: {url} did not answer ({e})")
    data = body.get("data", body)
    if not data or not data.get("label"):
        sys.exit(f"design-doc-identity: {url} returned no identity — is {name} in the model?")
    return data


def esc(t):
    return (str(t).replace("&", "&amp;").replace("<", "&lt;")
            .replace(">", "&gt;").replace('"', "&quot;"))


def render(cls, name, data):
    rows = []
    for f in FIELDS:
        v = data.get(f)
        if v:
            rows.append(f"    <tr><th>{esc(f)}</th><td>{esc(v)}</td></tr>")
    if not rows:
        sys.exit(f"design-doc-identity: {name} has no renderable identity fields")
    return "\n".join([
        BEGIN,
        f'  <p class="muted">Identity below is generated from the model — '
        f'<code>GET {OWL}/{COLLECTION[cls]}/{esc(name)}</code>. '
        f'Edit the model, not this block.</p>',
        '  <table class="identity">',
        *rows,
        "  </table>",
        END,
    ])


def extract(doc):
    if BEGIN not in doc or END not in doc:
        return None
    return doc[doc.index(BEGIN):doc.index(END) + len(END)]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--class", dest="cls", required=True)
    ap.add_argument("--name", required=True)
    ap.add_argument("--check", metavar="FILE",
                    help="compare FILE's generated block to the model; exit 1 on drift")
    a = ap.parse_args()
    block = render(a.cls, a.name, fetch(a.cls, a.name))

    if not a.check:
        print(block)
        return 0

    with open(a.check, encoding="utf-8") as fh:
        doc = fh.read()
    found = extract(doc)
    if found is None:
        print(f"DRIFT: {a.check} carries no generated-identity block", file=sys.stderr)
        return 1
    if found.strip() != block.strip():
        print(f"DRIFT: {a.check} identity block differs from the model "
              f"({a.cls}/{a.name}). Regenerate it.", file=sys.stderr)
        return 1
    print(f"ok: {a.check} identity matches the model ({a.cls}/{a.name})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
