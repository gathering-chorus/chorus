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
import re, os, sys, urllib.request, urllib.parse, json
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from fuseki_auth import write_auth_headers  # #3566 write-door credential (empty unless FUSEKI_ADMIN_PASSWORD set)

NS  = "https://jeffbridwell.com/chorus#"
DG  = "urn:chorus:domains:tests"   # 3825: the graph owl-api SERVES (plural). The singular was a dead graph nobody read - verify the store, never the file.
UPD = os.environ.get("ATHENA_UPDATE", "http://localhost:3030/pods/update")
OWLAPI = os.environ.get("OWLAPI", "http://localhost:3360")
HOME = f"{NS}tests"

def esc(s):
    s = re.sub(r'[\x00-\x1f]', ' ', s)
    return s.replace('\\', '\\\\').replace('"', '\\"')
def slug(s): return re.sub(r'[^a-z0-9]+', '-', s.lower()).strip('-')[:90]

# the generated V2 domains: the ONLY legal covers targets (no invented domains)
GEN = {x['name'] for x in json.load(urllib.request.urlopen(f"{OWLAPI}/domains", timeout=6))['data']}
# #3825 — one live domain is served capitalized ('Properties'); resolve config
# names case-insensitively but always EMIT the live name, never invent a casing.
GEN_CI = {n.lower(): n for n in GEN}

HANDMAP = [("failure_class","builds"),("ac-autocheck","cicd"),("api-fragile-endpoints","services"),
 ("chorus-inject-signed-stable","messages"),("chorus-ops-triage","alerts-monitors"),("close-out","roles"),
 ("daily-signal-scan","alerts-monitors"),("domain-detail-retired","domains"),("execsync-audit","security"),
 ("ownership-partof-chain","domains"),("regression-locks","cicd"),("write-story","cards")]
PREFIX = sorted([("platform/services/chorus-hooks","cicd"),("platform/services/owl-api","domains"),
 ("platform/services/chorus-model","domains"),("platform/services/athena-deploy","cicd"),
 ("platform/services/chorus-inject","messages"),("platform/services/pulse-gather","messages"),
 ("platform/services/properties-resolver","properties"),("platform/services/loom-gemba","alerts-monitors"),
 ("platform/services/pair-heartbeat","roles"),("platform/services/werk-","builds"),
 ("platform/mcp-server","services"),("platform/chorus-sdk","services"),("platform/scripts","toolchain"),
 ("platform/workflow-engine","pipelines"),("platform/pulse","messages"),("platform/api","services")],
 key=lambda x: -len(x[0]))
KW = [(r'secret|gitleaks|scrubber|sensitive|credential|leak','security'),(r'alert','alerts-monitors'),
 (r'health|probe|heartbeat|monitor|andon|watchdog','alerts-monitors'),(r'doc|catalog','knowledge'),(r'knowledge','knowledge'),
 (r'principle','principles'),(r'skill|standards','skills'),(r'clippy|lint','code'),(r'decision','decisions'),
 (r'perf|baseline','metrics'),(r'infrastructure','infrastructure'),(r'nudge|bridge|message|clearing','messages'),
 (r'pulse','messages'),(r'role-state|alias','roles'),(r'context-inject|inject-lock|shim|spine','spine'),
 (r'ci-|nightly','cicd'),(r'hook|gate|guard|bouncer','cicd'),(r'demo|werk|run-tests|manifest|jest-randomize','builds'),
 (r'env-setup|building|pipeline|act-','builds'),(r'deploy|launch','deploys'),(r'promtail','logs'),(r'search|fts','search'),
 (r'force-push','version-control'),(r'filedependson|fileindomain','search'),(r'crawl|index|convergence','search'),
 (r'session|correlation|frustration','pulse'),(r'operating-model|reference-model','domains'),
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
    if m: return cardlookup(m.group(1)) or "services"
    if path.startswith("platform/tests/"):
        for pat, dom in KW:
            if re.search(pat, b): return dom
        return "services"
    for pre, dom in PREFIX:
        if path.startswith(pre): return dom
    for pat, dom in KW:
        if re.search(pat, b): return dom
    return "services"

# #3924 — the AUTHORED declaration wins. The @test-type header (enforced at
# commit by gate-test-type.ts, #3442) was thrown away at ingest: classify()
# re-guessed every layer from path/content regexes, so the runner selected on
# a heuristic while the author's declaration sat unread at line 1. Grammar
# mirrors gate-test-type.ts exactly: layer[:concern] after "@test-type:",
# comment leader // or # or *. Returns (layer, concern) or None.
DECLARED_RE = re.compile(
    r"""^\s*(?:\/\/|#|\*)\s*@test-type:\s*([a-z0-9-]+)(?::([a-z0-9-]+))?""",
    re.I | re.M)
VALID_LAYERS = {'unit','integration','bdd','e2e','contract','fitness','smoke'}
VALID_CONCERNS = {'api','ui','perf','security'}

def declared(c):
    m = DECLARED_RE.search(c[:2000])
    if not m: return None
    layer = m.group(1).lower()
    concern = (m.group(2) or '').lower() or None
    if layer not in VALID_LAYERS: return None          # junk header -> heuristic, inferred
    if concern and concern not in VALID_CONCERNS: concern = None
    return layer, concern

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

# #3924 (with Wren) — discovery walks every test-bearing root, not just
# platform/. proving/ (browser flows) and directing/ (product tests) were
# invisible: SPARQL showed ZERO browser tests in the graph, which is how a
# green land could skip Jeff's phone entirely (#3872). Roots are explicit so
# a new test-bearing tree is a one-line, reviewed widening.
TEST_ROOTS = ("platform", "proving", "directing", "skills")

def discover(roots=TEST_ROOTS):
    excl = re.compile(r'node_modules|/dist/|/spikes/|/target/|/\.git/')
    out = []
    for root in roots:
      if not os.path.isdir(root): continue
      for d, _, fs in os.walk(root):
        if excl.search(d + '/'): continue
        for f in fs:
            p = os.path.join(d, f)
            if excl.search(p): continue
            # .spec.cjs/.mjs were missing — playwright flows are .spec.cjs (#3872)
            # #3974: platform/scripts/test-*.sh shell suites + .feature files
            # join the registry so the nightly's full selection covers them.
            if re.search(r'\.bats$|\.(test|spec)\.[cm]?[tj]s$|\.test\.sh$|(_test|test_).*\.py$|\.feature$', f): out.append(p)
            elif f.startswith('test-') and f.endswith('.sh') and d.rstrip('/').endswith('platform/scripts'): out.append(p)
            elif f.endswith('.rs') and re.search(r'#\[(?:tokio::)?test\]', open(p, errors='ignore').read()): out.append(p)
    return out

def post(q):
    r = urllib.request.Request(UPD, data=urllib.parse.urlencode({'update': q}).encode(),
                               headers={'Content-Type': 'application/x-www-form-urlencoded', **write_auth_headers()})
    return urllib.request.urlopen(r, timeout=40).status

# Bounded delete guard (#3560). The ONLY clear-target this script may touch is a
# domain-owned graph (urn:chorus:domain:<name>). Refuses empty/unscoped targets and
# every system/portfolio graph (urn:chorus:instances, urn:gathering:*, urn:jb:*) —
# the wrong-graph clobber class that made the 2026-06-22 incident recoverable only by
# luck. A raw `DELETE WHERE { GRAPH <X> { ?s ?p ?o } }` with an unvalidated X is gone.
_DOMAIN_GRAPH = re.compile(r'^urn:chorus:domains?:[a-z0-9-]+$')  # 3825: served graphs use the plural form

def clear_graph(dg):
    if not dg or not _DOMAIN_GRAPH.match(dg):
        raise SystemExit(f"#3560 guard: refusing to clear {dg!r} — only "
                         f"urn:chorus:domain:<name> may be cleared, never a system/portfolio graph")
    # #3825 - TYPED clear: this graph also serves TestResult + TestSuiteRun
    # (the wire-back corpus, 27k+ rows). Deleting ?t ?p ?o would wipe them - the
    # #3601 graph-wipe class. The tagger owns exactly Test, SourceFile, and its
    # HydrationStamp; only those are cleared.
    rc = 0
    for cls in ("Test", "SourceFile", "HydrationStamp"):
        rc = post(f"PREFIX chorus: <{NS}> DELETE WHERE {{ GRAPH <{dg}> {{ ?t a chorus:{cls} ; ?p ?o }} }}")
    return rc

def freshness_stamp(now_iso, commit):
    """#3811 AC5 — corpus-level provenance, written WITH the corpus so staleness
    is answerable from the graph itself (generated-at + source commit)."""
    return (f'<{NS}tests-corpus> a chorus:HydrationStamp ; '
            f'chorus:generatedAt "{esc(now_iso)}" ; chorus:fromCommit "{esc(commit)}" ; '
            f'chorus:inDomain <{HOME}> .')

def main():
    files = discover()
    # #3970 — VALIDATE BEFORE CLEAR. The old order (clear, then build+assert per
    # file) meant one bad covers target destroyed the registry it was refilling:
    # 2026-08-21, /domains stopped serving 'deploys' and the ingest wiped
    # 1300+ Tests down to a partial 270. Now the ENTIRE corpus builds (and every
    # covers assert runs) first; only a fully-valid corpus clears and writes.
    batches, batch, seen, ntests = [], [], set(), 0
    def flush():
        nonlocal batch
        if batch:
            batches.append(batch)
            batch = []
    for p in files:
        cs, c = case_names(p)
        layer, herm, concern = classify(p, c)
        inferred = "true"
        d = declared(c)
        if d:
            # authored beats heuristic — even when the path signal disagrees
            layer = d[0]
            if d[1]: concern = d[1]
            inferred = "false"
        cov = "security" if concern == 'security' else covers_for(p)
        assert cov.lower() in GEN_CI, f"covers target {cov!r} is not a generated V2 domain"   # no invented domains
        cov = GEN_CI[cov.lower()]
        sf = f"{NS}sf-{slug(p)}"
        batch.append(f'<{sf}> a chorus:SourceFile ; chorus:filePath "{esc(p)}" .')
        for nm in cs:
            ti = f"{NS}test-{slug(p)}-{slug(nm)}"
            if ti in seen: continue
            seen.add(ti)
            t = (f'<{ti}> a chorus:Test ; chorus:filePath "{esc(p)}" ; chorus:testName "{esc(nm[:160])}" ; '
                 f'chorus:pyramidLayer "{layer}" ; chorus:hermeticity "{herm}" ; chorus:inferred "{inferred}" ; '
                 f'chorus:inFile <{sf}> ; chorus:inDomain <{HOME}> ; chorus:covers <{NS}{cov}>')
            if concern: t += f' ; chorus:testConcern "{concern}"'
            batch.append(t + " .")
            ntests += 1
        if len(batch) >= 300: flush()
    # #3811 AC5 — the stamp rides the same run (same clear+insert lifecycle).
    import datetime, subprocess
    now_iso = datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
    commit = (subprocess.run(['git', 'rev-parse', 'HEAD'], capture_output=True, text=True)
              .stdout.strip() or "unknown")
    batch.append(freshness_stamp(now_iso, commit))
    flush()
    # Every file classified, every covers target validated — NOW touch the store.
    clear_graph(DG)  # bounded + typed (#3560/#3825) — Test/SourceFile/stamp only
    for b in batches:
        post(f"PREFIX chorus: <{NS}> INSERT DATA {{ GRAPH <{DG}> {{\n" + "\n".join(b) + "\n} }")
    print(f"tests-domain ingested: {len(files)} files -> {ntests} Tests in {DG} (stamp {now_iso} @ {commit[:12]})")

if __name__ == "__main__":
    main()
