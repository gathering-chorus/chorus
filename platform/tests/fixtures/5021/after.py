#!/usr/bin/env python3
"""#5021 — today's principles snapshot with this card's changes applied, the way the land
applies them: (1) model-retirements.jsonl's staged retire_subject lines for this card drop
those subjects from their graph; (2) the Domain row is redeployed whole from every model file
that names it (athena-deploy stages a changed subject with its lines from every file, #4590).
The shapes ride along as <urn:chorus:ontology>, so a SPARQL check sees them as in the store.

  after.py <repo> <snapshot.nq> <out.nq>      (before = the snapshot itself + the same shapes)
  after.py --before <repo> <snapshot.nq> <out.nq>
"""
# @domain: principles
import json, re, subprocess, sys

C = "https://jeffbridwell.com/chorus#"
SUBJECT = f"<{C}principles>"
MODEL = ["roles/silas/ontology/chorus.ttl", "roles/wren/ontology/domains-wren-silas.ttl"]
SHAPES = ["roles/silas/ontology/chorus.ttl", "roles/silas/ontology/core-shape-4576.ttl",
          "roles/silas/ontology/session-model-4302.ttl", "roles/wren/ontology/principles-3749.ttl"]
NQ = re.compile(r"^(\S+) (\S+) (.*) <([^>]+)> \.$")


def nt(repo, path):
    p = subprocess.run(["riot", "--output=nt", f"{repo}/{path}"], capture_output=True, text=True, check=True)
    return [l for l in p.stdout.splitlines() if l.strip()]


def main(argv):
    before = argv[:1] == ["--before"]
    repo, snap, out = argv[1:] if before else argv
    quads = []
    for line in open(snap):
        m = NQ.match(line.strip())
        if m:
            quads.append(m.groups())
    if not before:
        gone = set()
        for l in open(f"{repo}/designing/schemas/model-retirements.jsonl"):
            r = json.loads(l)
            if r.get("card") == "5021" and r.get("retire_subject"):
                gone.add((f"<{r['retire_subject']}>", r["graph"]))
        quads = [q for q in quads if (q[0], q[3]) not in gone]
        quads = [q for q in quads if not (q[0] == SUBJECT and q[3] == "urn:chorus:domains:domains")]
        for f in MODEL:
            for l in nt(repo, f):
                if l.startswith(SUBJECT + " "):
                    s, p, o = l[:-2].split(" ", 2)
                    quads.append((s, p, o, "urn:chorus:domains:domains"))
    with open(out, "w") as f:
        for s, p, o, g in quads:
            f.write(f"{s} {p} {o} <{g}> .\n")
        for path in SHAPES:
            for l in nt(repo, path):
                f.write(l[:-2] + " <urn:chorus:ontology> .\n")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
