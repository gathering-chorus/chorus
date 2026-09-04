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

def local_cap(local):
    # athena-make's write door refuses local names >128 bytes (is_safe_local).
    # This SPARQL path must mint within the same law, or TestResults can never
    # reference the minted Test (ofTest edge 422s and the whole chunk is lost).
    # rstrip: a truncation ending in '-' would mint 'x--hash'; the serve collapses
    # runs of '-', so the stored name and the served name would disagree and every
    # TestResult referencing it would 422 (seen live 2026-08-27).
    if len(local) <= 128: return local
    import hashlib
    return local[:118].rstrip('-') + '-' + hashlib.sha1(local.encode()).hexdigest()[:9]

# the generated V2 domains: the ONLY legal covers targets (no invented domains)
GEN = {x['name'] for x in json.load(urllib.request.urlopen(f"{OWLAPI}/domains", timeout=6))['data']}
# #3825 — one live domain is served capitalized ('Properties'); resolve config
# names case-insensitively but always EMIT the live name, never invent a casing.
GEN_CI = {n.lower(): n for n in GEN}

HANDMAP = [("failure_class","builds"),("ac-autocheck","cicd"),("api-fragile-endpoints","services"),
 ("chorus-inject-signed-stable","messages"),("chorus-ops-triage","alerts-monitors"),("close-out","roles"),
 ("daily-signal-scan","alerts-monitors"),("domain-detail-retired","domains"),("execsync-audit","security"),
 ("ownership-partof-chain","domains"),("regression-locks","cicd"),("write-story","cards")]
PREFIX = sorted([("platform/services/chorus-hooks","cicd"),
 # #3996 — the "services" bucket held 43% of the corpus because these trees had
 # no rule and fell to the global default. Real homes, all generated domains:
 ("directing/products/cards","cards"),("directing/clearing","messages"),
 ("platform/services/athena-make","domains"),("platform/services/athena-model","domains"),
 ("platform/services/chorus-oidc","identity"),("platform/apps","products"),
 ("proving/flows","builds"),("platform/services/owl-api","domains"),
 ("platform/services/chorus-model","domains"),("platform/services/athena-deploy","deploys"),
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
 (r'session|correlation|frustration','messages'),(r'operating-model|reference-model','domains'),
 (r'git|commit|merge|branch','version-control')]

def cardlookup(n):
    if os.environ.get("TESTS_COVERS_OFFLINE") == "1":
        return None  # hermetic mode (#3528): no network, deterministic fallback
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
    # #3996 — basename keywords BEFORE package prefixes: an eventloop-alert test
    # under platform/api is about alerts, not "everything the api serves". The
    # prefix stays as the package-level fallback, not the first answer.
    for pat, dom in KW:
        if re.search(pat, b): return dom
    for pre, dom in PREFIX:
        if path.startswith(pre): return dom
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

# #4022 — a jest name is the WHOLE first string argument. The old pattern
# stopped at the first quote of any kind inside the name, so
#   it('eventFrame is NIP-01 ["EVENT", event]')  registered as  'eventFrame is NIP-01 ['
#   it(`has zero = ${n}`)                        registered as  'has zero = ${n'
# and the runner's fullName could never join them: 887 "never ran" in the
# census and 594 results per nightly with no identity to save under. Match a
# real string literal (same-quote delimited, backslash escapes honoured).
# #4106 — `\b` also matched `test(` in `/Log in/.test('<button>Log in</button>')`,
# so a regex call registered its ARGUMENT as a test case. Two such phantoms sat
# in the registry as permanent never-ran rows. A declaration is never preceded
# by a dot or an identifier character.
JEST_NAME_RE = re.compile(
    r'(?<![.\w$])(?:it|test)(?:\.(?:only|skip|each|concurrent))?\s*\(\s*'
    r"(?:'((?:[^'\\]|\\.)*)'|\"((?:[^\"\\]|\\.)*)\"|`((?:[^`\\]|\\.)*)`)")
def jest_case_names(source):
    out = []
    for m in JEST_NAME_RE.finditer(source):
        nm = next(g for g in m.groups() if g is not None)
        nm = nm.replace("\\'", "'").replace('\\"', '"').replace('\\`', '`')
        # #4106 — a name built by interpolation is a template, not a name. The
        # runner emits the interpolated value ("…port 51873"), so a row holding
        # the raw "…port ${TEST_PORT}" can never be joined to a result and sits
        # in the census as never-ran forever. Four of them did.
        if '${' in nm:
            continue
        out.append(nm)
    return out

def case_names(path):
    try: c = open(path, errors='ignore').read()
    except Exception: return [os.path.basename(path)], ''
    if path.endswith('.rs'): r = re.findall(r'#\[(?:tokio::)?test\][^\n]*\n\s*(?:async\s+)?fn\s+(\w+)', c)
    # #4106 — anchored to line start: the unanchored pattern also matched a
    # @test declaration written INSIDE a string fixture (a bats suite that
    # builds a little .bats file to run the tagger against registered its
    # fixture's name as a real test — 5 such phantoms, one of them called "x").
    elif path.endswith('.bats'): r = re.findall(r'(?m)^[ \t]*@test\s+"([^"]+)"', c)
    elif re.search(r'\.(test|spec)\.[tj]s$', path): r = jest_case_names(c)
    else: r = []
    # #4106 — the old fallback returned [basename], inventing a case name for
    # every kind with no extractor (.sh, .feature, .py). No runner emits a case
    # called "daemon-env-3197.test.sh", so all 90 were permanent never-ran rows
    # that read as missing coverage. The FILE is still registered (its
    # SourceFile row is written regardless); what is not invented is a case.
    return r, c

# #3924 (with Wren) — discovery walks every test-bearing root, not just
# platform/. proving/ (browser flows) and directing/ (product tests) were
# invisible: SPARQL showed ZERO browser tests in the graph, which is how a
# green land could skip Jeff's phone entirely (#3872). Roots are explicit so
# a new test-bearing tree is a one-line, reviewed widening.
TEST_ROOTS = ("platform", "proving", "directing", "skills")

def discover(roots=TEST_ROOTS):
    # `/dist/` alone missed every SIBLING build directory — platform/pulse has
    # dist.prev/, dist.prev-3130/ and dist.prev-l2/, and their compiled .js
    # copies registered as 338 real tests that nothing runs and nothing can run.
    # They were a third of the nightly's "registered tests never ran" gap.
    excl = re.compile(r'node_modules|/dist(\.[-\w]+)?/|/spikes/|/target/|/\.git/')
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

# #3996 — share gate. One domain holding an outsized share of the corpus makes
# werk-test's covers-union select half the registry for any touching diff
# (2026-08-23: "services" at 43% → a 2-unit run exploded to 166). The ingest
# REFUSES to write a corpus where any domain exceeds MAX_DOMAIN_SHARE.
# Threshold is config (tests-covers.conf beside this script), env-overridable.
def max_domain_share():
    v = os.environ.get("MAX_DOMAIN_SHARE")
    if not v:
        conf = os.path.join(os.path.dirname(os.path.abspath(__file__)), "tests-covers.conf")
        try:
            for line in open(conf):
                line = line.strip()
                if line.startswith("MAX_DOMAIN_SHARE="):
                    v = line.split("=", 1)[1].strip().strip('"')
        except OSError:
            pass
    return float(v or "0.30")

# #4022 — the share gate is about CORPUS shape, and a corpus of one file is
# always 100% one domain. The tagger's own validate-first test (one fixture
# test) tripped the gate on 2026-08-29 (services 1/1 > 30%) and went red in
# the nightly. Below MIN_CORPUS_FOR_SHARES the gate has no meaning and stands down.
MIN_CORPUS_FOR_SHARES = int(os.environ.get("MIN_CORPUS_FOR_SHARES", "20"))

def assert_shares(counts):
    total = sum(counts.values()) or 1
    if total < MIN_CORPUS_FOR_SHARES:
        return
    cap = max_domain_share()
    worst = sorted(counts.items(), key=lambda x: -x[1])
    over = [(d, n) for d, n in worst if n / total > cap]
    if over:
        hist = " ".join(f"{d}={n}({n*100//total}%)" for d, n in worst[:6])
        raise SystemExit(
            f"covers-share gate RED (#3996): {over[0][0]} holds {over[0][1]}/{total} "
            f"(> {cap:.0%}) — refusing to write an over-broad corpus. top: {hist}")

def no_case_report(paths):
    """#4106 — one line naming the files that yield no runnable case, by kind."""
    if not paths:
        return "no-case files: none — every registered file names at least one case"
    counts = {}
    for p in paths:
        counts[p.rsplit('.', 1)[-1]] = counts.get(p.rsplit('.', 1)[-1], 0) + 1
    kinds = ", ".join(f"{k} {v}" for k, v in sorted(counts.items(), key=lambda kv: (-kv[1], kv[0])))
    return (f"no-case files: {len(paths)} registered file(s) yield no runnable case "
            f"({kinds}) — they need a lane or an extractor, not an invented name")


def main():
    files = discover()
    # #3970 — VALIDATE BEFORE CLEAR. The old order (clear, then build+assert per
    # file) meant one bad covers target destroyed the registry it was refilling:
    # 2026-08-21, /domains stopped serving 'deploys' and the ingest wiped
    # 1300+ Tests down to a partial 270. Now the ENTIRE corpus builds (and every
    # covers assert runs) first; only a fully-valid corpus clears and writes.
    batches, batch, seen, ntests = [], [], set(), 0
    cover_counts = {}
    def flush():
        nonlocal batch
        if batch:
            batches.append(batch)
            batch = []
    no_case = []   # #4106 — files the registry can see but cannot name a case in
    for p in files:
        cs, c = case_names(p)
        if not cs:
            no_case.append(p)
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
        cover_counts[cov] = cover_counts.get(cov, 0) + len(cs)
        sf = f"{NS}sf-{slug(p)}"
        batch.append(f'<{sf}> a chorus:SourceFile ; chorus:filePath "{esc(p)}" .')
        for nm in cs:
            ti = f"{NS}{local_cap(f'test-{slug(p)}-{slug(nm)}')}"
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
    # Every file classified, every covers target validated — NOW the share gate,
    # then the store (#3996: an over-broad corpus never lands).
    assert_shares(cover_counts)
    clear_graph(DG)  # bounded + typed (#3560/#3825) — Test/SourceFile/stamp only
    for b in batches:
        post(f"PREFIX chorus: <{NS}> INSERT DATA {{ GRAPH <{DG}> {{\n" + "\n".join(b) + "\n} }")
    print(f"tests-domain ingested: {len(files)} files -> {ntests} Tests in {DG} (stamp {now_iso} @ {commit[:12]})")
    # #4106 — VISIBLE, never silent. These files are discovered and registered as
    # SourceFiles but yield no case name, so nothing joins a result to them and
    # no lane necessarily runs them. Before #4106 each one minted a case named
    # after the file, which read in the census as a test that never ran. Dropping
    # the invention without printing the list would just make them disappear —
    # the point is the opposite, so the count and the kinds are always stated.
    print(no_case_report(no_case))

if __name__ == "__main__":
    # #3996 — hermetic test seams (a test brings its own world, #3528):
    #   --covers-of <path>       print the inferred domain for one path, no store
    #   --check-shares <json>    apply the share gate to a {"domain": count} fixture
    if len(sys.argv) >= 3 and sys.argv[1] == "--covers-of":
        print(covers_for(sys.argv[2])); sys.exit(0)
    if len(sys.argv) >= 3 and sys.argv[1] == "--check-shares":
        assert_shares(json.load(open(sys.argv[2]))); print("shares ok"); sys.exit(0)
    #   --names-of <path>        print the case names the registry would hold, no store (#4022)
    #   --no-case-files          list the discovered files that name no case (#4106)
    if len(sys.argv) >= 2 and sys.argv[1] == "--no-case-files":
        nc = [p for p in discover() if not case_names(p)[0]]
        for p in nc:
            print(p)
        print(no_case_report(nc), file=sys.stderr)
        sys.exit(0)
    if len(sys.argv) >= 3 and sys.argv[1] == "--names-of":
        for nm in case_names(sys.argv[2])[0]: print(nm)
        sys.exit(0)
    main()
