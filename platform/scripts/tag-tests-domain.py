#!/usr/bin/env python3
"""tests-domain ingestion (#2818) — REPEATABLE, not a one-off mint.

Crawls the repo's test corpus and writes one chorus:Test per test CASE into the
tests domain's owned graph (urn:chorus:domain:tests — a domain owns its instances
in urn:chorus:domain:<name>). Re-running regenerates the corpus by construction;
owl-api projects it under /domains/tests (read side, separate concern).

Each Test:
  - identity  : (chorus:filePath, chorus:testName)         [per-CASE grain]
  - home      : chorus:inDomain -> chorus:tests             [the V2 Domain, all]
  - subject   : chorus:covers   -> a GENERATED V2 domain    [/domains, validated]
  - class     : chorus:pyramidLayer + chorus:hermeticity (+ chorus:testConcern)
  - chorus:inFile -> chorus:SourceFile (per-FILE node; run-decision aggregates up:
    a file is hermetic-runnable iff ALL its Tests are hermetic)

Two CONFIG layers, validated against Silas's #3528 hand-set (12/12 anchor, clean
at scale — the gate caught security/senses/alert mis-tags before silent wrong-data):
  1. CLASSIFIER  = EXECUTE-vs-INSPECT: hermetic if the test only INSPECTS a
     representation (static-grep / build-string / pure fn) regardless of vocabulary;
     needs-stack only if it EXECUTES a live dep.
  2. COVERS-INFERENCE = path-prefix + keyword + handler + card rules -> the generated
     V2 domains (:3360/domains). ZERO invented domains (asserted against /domains).

The domain MODEL stays generated (owl-api projects it); this is the INGESTION
(tooling config), deliberately separate from the model.
"""
import re, os, urllib.request, urllib.parse, json

NS  = "https://jeffbridwell.com/chorus#"
DG  = "urn:chorus:domain:tests"
UPD = os.environ.get("ATHENA_UPDATE", "http://localhost:3030/pods/update")
OWLAPI = os.environ.get("OWLAPI", "http://localhost:3360")
HOME = f"{NS}tests"

def esc(s):
    s = re.sub(r'[\x00-\x1f]', ' ', s)
    return s.replace('\\', '\\\\').replace('"', '\\"')
def slug(s): return re.sub(r'[^a-z0-9]+', '-', s.lower()).strip('-')[:90]

# the generated V2 domains: the ONLY legal covers targets (no invented domains)
GEN = {x['name'] for x in json.load(urllib.request.urlopen(f"{OWLAPI}/domains", timeout=6))['data']}

HANDMAP = [("failure_class","builds"),("ac-autocheck","cicd"),("api-fragile-endpoints","senses"),
 ("chorus-inject-signed-stable","messages"),("chorus-ops-triage","alerts-monitors"),("close-out","roles"),
 ("daily-signal-scan","alerts-monitors"),("domain-detail-retired","domains"),("execsync-audit","security-trust"),
 ("ownership-partof-chain","domains"),("regression-locks","cicd"),("write-story","cards")]
PREFIX = sorted([("platform/services/chorus-hooks","cicd"),("platform/services/owl-api","domains"),
 ("platform/services/chorus-model","domains"),("platform/services/athena-deploy","deploys"),
 ("platform/services/chorus-inject","messages"),("platform/services/pulse-gather","messages"),
 ("platform/services/properties-resolver","properties"),("platform/services/loom-gemba","senses"),
 ("platform/services/pair-heartbeat","roles"),("platform/services/werk-","builds"),
 ("platform/mcp-server","services"),("platform/chorus-sdk","services"),("platform/scripts","toolchain"),
 ("platform/workflow-engine","pipelines"),("platform/pulse","messages"),("platform/api","senses")],
 key=lambda x: -len(x[0]))
KW = [(r'secret|gitleaks|scrubber|sensitive|credential|leak','security-trust'),(r'alert','alerts-monitors'),
 (r'health|probe|heartbeat|monitor|andon|watchdog','alerts-monitors'),(r'doc|catalog','knowledge'),(r'knowledge','knowledge'),
 (r'principle','principles'),(r'skill|standards','skills'),(r'clippy|lint','code'),(r'decision','decisions'),
 (r'perf|baseline','metrics'),(r'infrastructure','infrastructure'),(r'nudge|bridge|message|clearing','messages'),
 (r'pulse','messages'),(r'role-state|alias','roles'),(r'context-inject|inject-lock|shim|spine','spine'),
 (r'ci-|nightly','cicd'),(r'hook|gate|guard|bouncer','cicd'),(r'demo|werk|run-tests|manifest|jest-randomize','builds'),
 (r'env-setup|building|pipeline|act-','builds'),(r'deploy|launch','deploys'),(r'promtail','logs'),(r'search|fts','search'),
 (r'force-push','version-control'),(r'filedependson|fileindomain','senses'),(r'crawl|index|convergence','senses'),
 (r'session|correlation|frustration','senses'),(r'operating-model|reference-model','domains'),
 (r'git|commit|merge|branch','version-control')]

def cardlookup(n):
    try:
        d = json.load(urllib.request.urlopen(f"http://localhost:3340/api/chorus/card-story/{n}", timeout=4))
        dom = str(d.get('domain') or d.get('subproduct') or '').lower()
        return dom if dom in GEN else None
    except Exception:
        return None

def covers_for(path):
    b = os.path.basename(path).lower()
    for sub, dom in HANDMAP:
        if sub in b: return dom
    if path.startswith("platform/api/tests/handlers/"): return "domains"
    m = re.match(r'platform/tests/(\d{3,4})-', path)
    if m: return cardlookup(m.group(1)) or "senses"
    if path.startswith("platform/tests/"):
        for pat, dom in KW:
            if re.search(pat, b): return dom
        return "senses"
    for pre, dom in PREFIX:
        if path.startswith(pre): return dom
    for pat, dom in KW:
        if re.search(pat, b): return dom
    return "senses"

def classify(path, c):
    pc = path + "\n" + c
    concern = None
    if re.search(r'gitleaks|write_scrubber|sensitive-path', pc, re.I): concern = 'security'
    elif re.search(r'#\[bench\]|criterion|latency.?budget|throughput.?budget', pc, re.I): concern = 'perf'
    in_crate = path.endswith('.rs') and '/src/' in path
    EXEC = re.search(
        r'''curl\s+(-\w+\s+|--\S+\s+)*["']?https?://(localhost|127\.)|curl[^\n]{0,40}:3[0-9]{3}'''
        r'''|Command::new\(\s*["'](launchctl|osascript|curl|kickstart|fuseki|gitleaks|git)'''
        r'''|^\s*launchctl\s+(kickstart|bootstrap|bootout|list|print|kill)|sparqlClient|\.query\('''
        r'''|\bfetch\(|await\s+[\w.]*(get|post|request|query)\(|POST[^\n]{0,40}(fuseki|3030)'''
        r'''|http://localhost:3[0-9]{3}|run\s+gitleaks|gitleaks\s+(detect|protect|--)|pre-commit\s+run|\bgit\s+commit''',
        c, re.I | re.M)
    if re.search(r'\.feature|cucumber|flow.{0,12}validator|scenario.?runner', pc, re.I): return 'bdd', 'hermetic', concern
    if re.search(r'env.?up[^\n]{0,40}teardown|launchd[^\n]{0,20}lifecycle|full.?pipeline|both_slots', c, re.I): return 'e2e', 'needs-stack', concern
    if EXEC and not in_crate: return 'integration', 'needs-stack', concern
    return 'unit', 'hermetic', concern

def case_names(path):
    try: c = open(path, errors='ignore').read()
    except Exception: return [os.path.basename(path)], ''
    if path.endswith('.rs'): r = re.findall(r'#\[(?:tokio::)?test\][^\n]*\n\s*(?:async\s+)?fn\s+(\w+)', c)
    elif path.endswith('.bats'): r = re.findall(r'@test\s+"([^"]+)"', c)
    elif re.search(r'\.(test|spec)\.[tj]s$', path): r = re.findall(r'\b(?:it|test)\s*\(\s*[\'"`]([^\'"`]+)', c)
    else: r = []
    return (r or [os.path.basename(path)]), c

def discover(root="platform"):
    excl = re.compile(r'node_modules|/dist/|/spikes/|/target/|/\.git/')
    out = []
    for d, _, fs in os.walk(root):
        if excl.search(d + '/'): continue
        for f in fs:
            p = os.path.join(d, f)
            if excl.search(p): continue
            if re.search(r'\.bats$|\.(test|spec)\.[tj]s$|\.test\.sh$|(_test|test_).*\.py$', f): out.append(p)
            elif f.endswith('.rs') and re.search(r'#\[(?:tokio::)?test\]', open(p, errors='ignore').read()): out.append(p)
    return out

def post(q):
    r = urllib.request.Request(UPD, data=urllib.parse.urlencode({'update': q}).encode(),
                               headers={'Content-Type': 'application/x-www-form-urlencoded'})
    return urllib.request.urlopen(r, timeout=40).status

def main():
    files = discover()
    post(f"PREFIX chorus: <{NS}> DELETE WHERE {{ GRAPH <{DG}> {{ ?t ?p ?o }} }}")  # idempotent
    batch, seen, ntests = [], set(), 0
    def flush():
        nonlocal batch
        if batch:
            post(f"PREFIX chorus: <{NS}> INSERT DATA {{ GRAPH <{DG}> {{\n" + "\n".join(batch) + "\n} }")
            batch = []
    for p in files:
        cs, c = case_names(p)
        layer, herm, concern = classify(p, c)
        cov = "security-trust" if concern == 'security' else covers_for(p)
        assert cov in GEN, f"covers target {cov!r} is not a generated V2 domain"   # no invented domains
        sf = f"{NS}sf-{slug(p)}"
        batch.append(f'<{sf}> a chorus:SourceFile ; chorus:filePath "{esc(p)}" .')
        for nm in cs:
            ti = f"{NS}test-{slug(p)}-{slug(nm)}"
            if ti in seen: continue
            seen.add(ti)
            t = (f'<{ti}> a chorus:Test ; chorus:filePath "{esc(p)}" ; chorus:testName "{esc(nm[:160])}" ; '
                 f'chorus:pyramidLayer "{layer}" ; chorus:hermeticity "{herm}" ; '
                 f'chorus:inFile <{sf}> ; chorus:inDomain <{HOME}> ; chorus:covers <{NS}{cov}>')
            if concern: t += f' ; chorus:testConcern "{concern}"'
            batch.append(t + " .")
            ntests += 1
        if len(batch) >= 300: flush()
    flush()
    print(f"tests-domain ingested: {len(files)} files -> {ntests} Tests in {DG}")

if __name__ == "__main__":
    main()
