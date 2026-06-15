#!/usr/bin/env python3
"""
crawler-facet-file-domain.py — #3426: the file→domain herald.

The FIRST model-driven crawler facet. Reads the file→domain attribution rules
from data/athena/tree.json (Domain.hasMapsTo = path-prefixes per domain),
crawls git-tracked files, computes each file's PRIMARY domain
(longest-prefix-wins) plus NON-PRIMARY edges, and posts
`<domain> chorus:contains <file>` into urn:chorus:instances so owl-api
/domains/:name/contains (+ /partof inverse) renders it — lighting #3420.

CONTRACTS (must match the live graph — #3426 grounding, verified):
  file IRI   : urn:chorus:file:<sha1(ABSOLUTE path)>   (crawler-hydrate-graph.sh:134)
  domain IRI : https://jeffbridwell.com/chorus#<name>   (tree.json iri "chorus:<name>")
  graph      : urn:chorus:instances                     (owl-api INSTANCES_GRAPH, lib.rs:24)
  predicate  : chorus:contains (domain→file)            (#3420 /contains reads this)

COHERENCE DECISION (#3426, DBA): canonical file→domain predicate is
chorus:contains — what the page reads + the uniform decomposition edge.
Deprecates fileInDomain (enrichment-write-fileInDomain.sh) and inDomain
(the 06-09 readout): three names for one fact, collapsed to one.

IDEMPOTENT: per file, DELETE any existing `?d contains <fileIRI>` then INSERT
the computed edges. The DELETE is scoped to object = this file's IRI, so the
product→domain→subdomain contains edges (object = chorus:<domain>, never a
urn:chorus:file:*) are never touched. Low-blast: runs alongside the monolith.

Default DRY-RUN (prints honest coverage). Pass --post to write to Fuseki.
"""
import json, os, subprocess, hashlib, sys, urllib.request, urllib.parse, urllib.error
from collections import Counter

CHORUS_ROOT = os.environ.get("CHORUS_ROOT", "/Users/jeffbridwell/CascadeProjects/chorus")
TREE = os.environ.get("TREE", os.path.join(CHORUS_ROOT, "data/athena/tree.json"))
NS = "https://jeffbridwell.com/chorus#"
GRAPH = "urn:chorus:instances"
FUSEKI_UPDATE = os.environ.get("FUSEKI_UPDATE", "http://localhost:3030/pods/update")
EXCLUDE_PREFIXES = ("node_modules/", "target/", "dist/", "dist.prev/", ".git/", ".venv/", "__pycache__/")


def file_iri(rel_path):
    """urn:chorus:file:<sha1(absolute path)> — identical to crawler-hydrate-graph.sh."""
    abspath = os.path.join(CHORUS_ROOT, rel_path)
    sha = hashlib.sha1(abspath.encode("utf-8")).hexdigest()
    return f"urn:chorus:file:{sha}"


def domain_iri(tree_iri):
    """tree.json iri 'chorus:cards' -> https://jeffbridwell.com/chorus#cards."""
    local = tree_iri.split(":", 1)[1] if ":" in tree_iri else tree_iri
    return f"{NS}{local}"


def load_rules(tree_path):
    """[(normalized_prefix, domain_iri, domain_label), ...] from hasMapsTo."""
    with open(tree_path) as fh:
        d = json.load(fh)
    rules = []
    for dom in d.get("domains", []):
        diri = domain_iri(dom.get("iri", ""))
        for pref in (dom.get("hasMapsTo") or []):
            norm = pref if pref.endswith("/") else pref + "/"
            rules.append((norm, diri, dom.get("label")))
    return rules


def git_files():
    try:
        out = subprocess.check_output(["git", "-C", CHORUS_ROOT, "ls-files"],
                                      text=True, timeout=60, stderr=subprocess.PIPE)
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired, OSError) as e:
        raise RuntimeError(f"git ls-files failed in {CHORUS_ROOT}: {e}") from e
    files = []
    for line in out.splitlines():
        f = line.strip()
        if not f or any(f.startswith(p) for p in EXCLUDE_PREFIXES):
            continue
        files.append(f)
    return files


def attribute(files, rules):
    """(file, primary_domain_iri|None, [nonprimary_iris]) per file. Longest-prefix-wins."""
    results = []
    for f in files:
        probe = f + "/"
        matches = [(pref, diri) for (pref, diri, _) in rules if probe.startswith(pref)]
        if not matches:
            results.append((f, None, []))
            continue
        matches.sort(key=lambda m: len(m[0]), reverse=True)
        primary = matches[0][1]
        nonprimary, seen = [], {primary}
        for _, diri in matches[1:]:
            if diri not in seen:
                nonprimary.append(diri); seen.add(diri)
        results.append((f, primary, nonprimary))
    return results


def sparql_update(update):
    data = urllib.parse.urlencode({"update": update}).encode()
    req = urllib.request.Request(FUSEKI_UPDATE, data=data,
                                 headers={"Content-Type": "application/x-www-form-urlencoded"})
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return r.status
    except (urllib.error.HTTPError, urllib.error.URLError, OSError) as e:
        raise RuntimeError(f"Fuseki update failed ({FUSEKI_UPDATE}): {e}") from e


def edges_from(results):
    """Flatten attribution results into (domain_iri, file_iri) edges — primary + non-primary."""
    edges = []
    for f, primary, nonprimary in results:
        if primary is None:
            continue
        firi = file_iri(f)
        for diri in [primary] + nonprimary:
            edges.append((diri, firi))
    return edges


def build_update(chunk):
    """SPARQL UPDATE for one batch. Idempotent + scoped: a per-file-IRI DELETE
    (object-scoped, so product->domain `contains` edges — object = chorus:<domain>,
    never urn:chorus:file:* — are NEVER touched), then INSERT the computed edges."""
    file_iris = sorted({fi for _, fi in chunk})
    dels = " ".join(
        f"DELETE WHERE {{ GRAPH <{GRAPH}> {{ ?d <{NS}contains> <{fi}> }} }};"
        for fi in file_iris)
    triples = " ".join(f"<{d}> <{NS}contains> <{f}> ." for d, f in chunk)
    return f"{dels} INSERT DATA {{ GRAPH <{GRAPH}> {{ {triples} }} }}"


def post_edges(results, batch=150):
    edges = edges_from(results)
    posted, failed = 0, 0
    for i in range(0, len(edges), batch):
        chunk = edges[i:i + batch]
        try:
            sparql_update(build_update(chunk))
            posted += len(chunk)
        except RuntimeError as e:
            failed += len(chunk)
            print(f"  batch {i // batch} FAILED: {e}")
    if failed:
        print(f"WARN: {failed} edges failed to post (continued past the failures)")
    return posted


def main():
    do_post = "--post" in sys.argv
    rules = load_rules(TREE)
    files = git_files()
    results = attribute(files, rules)
    covered = [r for r in results if r[1] is not None]
    uncovered = [r for r in results if r[1] is None]
    pc = Counter(r[1].split("#")[-1] for r in covered)
    pct = 100 * len(covered) // max(len(files), 1)
    print(f"facet=file->domain  rules={len(rules)}  files={len(files)}")
    print(f"covered(primary)={len(covered)}  uncovered={len(uncovered)}  coverage={pct}%  (honest: code coverage, not roles-blanket)")
    print("per-domain primary (top 20):")
    for dom, n in pc.most_common(20):
        print(f"  {dom}: {n}")
    if uncovered:
        print(f"uncovered sample (first 15 of {len(uncovered)}):")
        for f, _, _ in uncovered[:15]:
            print(f"  {f}")
    if do_post:
        n = post_edges(results)
        print(f"POSTED {n} <domain> contains <file> edges -> <{GRAPH}>")
    else:
        print("(dry-run; pass --post to write to Fuseki)")


if __name__ == "__main__":
    main()
