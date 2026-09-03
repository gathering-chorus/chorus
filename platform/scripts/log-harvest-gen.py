#!/usr/bin/env python3
"""log-harvest-gen.py — #4084. Every launchd unit's log, as chorus:LogSource rows.

Reads the LaunchAgents dir (StandardOutPath / StandardErrorPath), stats each file,
asks Loki which jobs promtail ships, joins the AUTHORED unit -> domain rows
(chorus:UnitDomainMapping, ADR-027) and writes Turtle to stdout. Pure transform:
the same inputs give the same rows (harvest, not append). Bedroom units come
from the service-harvest snapshot when present; their files are not stat'd from
Library, so they carry no lastWrittenAt (a gap the logs row names, not a guess).

  --plists DIR      LaunchAgents dir (default ~/Library/LaunchAgents)
  --mapping FILE    TSV label<TAB>domain (default: query the store)
  --loki-jobs FILE  one job per line (default: query Loki; absent = no lokiJob)
  --machine NAME    library | bedroom (default library)
  --check           print unmapped units, exit 1 if any (the negative proof)
"""
import argparse, glob, json, os, plistlib, re, subprocess, sys, time, urllib.request
from datetime import datetime, timezone

P = "https://jeffbridwell.com/chorus#"
FUSEKI_QUERY = os.environ.get("FUSEKI_QUERY", "http://localhost:3030/pods/query")
INFRA_GRAPH = os.environ.get("CHORUS_INFRA_GRAPH", "urn:chorus:domains:infrastructure")
LOKI = os.environ.get("LOKI_URL", "http://localhost:3102")

def esc(s): return str(s).replace("\\", "\\\\").replace('"', '\\"')

def mapping_from_store():
    q = ("PREFIX c: <%s> SELECT ?l ?d WHERE { GRAPH <%s> { ?m a c:UnitDomainMapping ; c:launchdLabel ?l ; c:hasDomain ?d } }" % (P, INFRA_GRAPH))
    data = urllib.parse.urlencode({"query": q}).encode()
    req = urllib.request.Request(FUSEKI_QUERY, data=data, headers={"Accept": "application/sparql-results+json"})
    with urllib.request.urlopen(req, timeout=20) as r:
        rows = json.load(r)["results"]["bindings"]
    return {b["l"]["value"]: b["d"]["value"].split("#")[-1] for b in rows}

def mapping_from_tsv(path):
    out = {}
    for line in open(path):
        line = line.rstrip("\n")
        if not line or line.startswith("#"): continue
        l, d = line.split("\t")[:2]; out[l] = d
    return out

def loki_jobs():
    try:
        with urllib.request.urlopen(LOKI + "/loki/api/v1/label/job/values", timeout=5) as r:
            return set(json.load(r).get("data", []))
    except Exception:
        return None  # Loki did not answer: absent lokiJob everywhere, and the run says so

# a unit's Loki job is the launchd label's last segment (com.chorus.alert-runner -> alert-runner),
# with the two renames promtail actually uses today
JOB_ALIAS = {"com.chorus.api": "chorus-api", "com.chorus.clearing": "chorus-bridge", "com.chorus.hooks": "chorus-hooks",
             "com.chorus.ops": "chorus-ops", "com.gathering.app": "gathering-app"}

def job_for(label, jobs):
    if jobs is None: return None
    cand = JOB_ALIAS.get(label, label.split(".")[-1])
    return cand if cand in jobs else None

def status_for(paths, now):
    if not paths: return "unobservable", None, None
    best = None
    for p in paths:
        try:
            st = os.stat(p); best = st if best is None or st.st_mtime > best.st_mtime else best
        except FileNotFoundError:
            continue
    if best is None: return "missing", None, None
    age = now - best.st_mtime
    return ("active" if age < 86400 else "silent"), best.st_mtime, best.st_size

def main():
    import urllib.parse
    ap = argparse.ArgumentParser()
    ap.add_argument("--plists", default=os.path.expanduser("~/Library/LaunchAgents"))
    ap.add_argument("--mapping"); ap.add_argument("--loki-jobs"); ap.add_argument("--machine", default="library")
    ap.add_argument("--check", action="store_true")
    a = ap.parse_args()
    mapping = mapping_from_tsv(a.mapping) if a.mapping else mapping_from_store()
    jobs = set(l.strip() for l in open(a.loki_jobs) if l.strip()) if a.loki_jobs else loki_jobs()
    now = time.time(); observed = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    units = []
    for f in sorted(glob.glob(os.path.join(a.plists, "com.chorus.*.plist")) + glob.glob(os.path.join(a.plists, "com.gathering.*.plist"))):
        try:
            with open(f, "rb") as fh: pl = plistlib.load(fh)
        except Exception as e:
            print(f"log-harvest-gen: cannot read {f}: {e}", file=sys.stderr); continue
        label = pl.get("Label") or os.path.basename(f)[:-6]
        paths = sorted({p for p in (pl.get("StandardOutPath"), pl.get("StandardErrorPath")) if p})
        units.append((label, paths))
    # A werk variant (com.chorus.api.werk.kade) is the base unit run for one role's card; it inherits the
    # base unit's domain. Rule, not a guess: env-up mints these per card and nobody authors rows for them.
    WERK = re.compile(r"^(.*)\.werk\.(silas|wren|kade)$")
    for l, _ in units:
        m = WERK.match(l)
        if m and l not in mapping and m.group(1) in mapping:
            mapping[l] = mapping[m.group(1)]
    unmapped = [l for l, _ in units if l not in mapping]
    if a.check:
        for l in unmapped: print(f"UNMAPPED {l}")
        print(f"log-harvest-gen --check: {len(units)} units, {len(unmapped)} unmapped", file=sys.stderr)
        sys.exit(1 if unmapped else 0)
    out = [f"@prefix chorus: <{P}> .", "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .",
           "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .", "",
           f"# log-harvest-gen #4084 — {len(units)} units on {a.machine}, {len(unmapped)} unmapped, observed {observed}", ""]
    for label, paths in units:
        st, mtime, size = status_for(paths, now)
        job = job_for(label, jobs)
        out.append(f"<urn:chorus:logsource-{a.machine}-{label}>")
        out.append(f"    a chorus:LogSource ;")
        out.append(f'    rdfs:label "{esc(label)} log ({a.machine})" ;')
        out.append(f'    chorus:launchdLabel "{esc(label)}" ;')
        out.append(f"    chorus:onMachine chorus:{a.machine} ;")
        for p in paths: out.append(f'    chorus:logPath "{esc(p)}" ;')
        if job: out.append(f'    chorus:lokiJob "{esc(job)}" ;')
        if label in mapping: out.append(f"    chorus:hasDomain chorus:{mapping[label]} ;")
        out.append(f'    chorus:logStatus "{st}" ;')
        if mtime is not None:
            out.append(f'    chorus:lastWrittenAt "{datetime.fromtimestamp(mtime, timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")}"^^xsd:dateTime ;')
            out.append(f"    chorus:sizeBytes {size} ;")
        out.append(f'    chorus:lastObserved "{observed}"^^xsd:dateTime .')
        out.append("")
    sys.stdout.write("\n".join(out))
    print(f"log-harvest-gen: {len(units)} units, {len(unmapped)} unmapped, loki {'answered' if jobs is not None else 'did not answer'}", file=sys.stderr)

if __name__ == "__main__":
    import urllib.parse  # noqa
    main()
