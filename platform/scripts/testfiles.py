#!/usr/bin/env python3
"""testfiles — the test-file parsers, as a LIBRARY (#4154, B3).

Lifted verbatim from tag-tests-domain.py (#2818 → #4136), which was a second
walker: it discovered test files, classified them and wrote Test rows by raw
SPARQL after clearing the tests graph on every run (the defect Jeff named
2026-09-03). The one walker (crawler-hydrate-graph.sh) now owns discovery and
persistence through the generated API; this module owns only the pure parts:

  is_test_file(path)      is this path a test file (by name; .rs by #[test] in content)
  declared(content)       the @test-type header (layer, concern) or None
  classify(path, content) (layer, hermeticity, concern) — the heuristic fallback
  case_names(path)        the case names a runner will actually emit, and the content
  discover(roots=None)    every test file under the tree (roots kept for callers; None = the whole tree)

No network, no store, no side effects. Every regex and its receipts (#4022,
#4106, #4111, #4135, #3872, #3974, #4131) are unchanged from the tagger.
"""
import json, os, re, sys, hashlib

TEST_FILE_RE = re.compile(r'\.bats$|\.(test|spec)\.[cm]?[tj]s$|\.test\.sh$|(_test|test_).*\.py$|\.feature$')
EXCLUDE_RE = re.compile(r'node_modules|/dist(\.[-\w]+)?/|/spikes/|/target/|/\.git/')
RUST_TEST_RE = re.compile(r'#\[(?:tokio::)?test\]')

def is_test_file(path, content=None):
    """A test file by NAME, or a .rs file carrying #[test] in CONTENT. The old
    tagger only looked under TEST_ROOTS, so a .bats under docs/ or a cargo
    tests/*.rs was invisible (9 files missing from the graph, Wren 2026-09-12)."""
    if EXCLUDE_RE.search(path): return False
    f = os.path.basename(path)
    if TEST_FILE_RE.search(f): return True
    if f.startswith('test-') and f.endswith('.sh') and os.path.dirname(path).rstrip('/').endswith('platform/scripts'): return True
    if f.endswith('.rs'):
        if 'platform/services/shared/' in path: return False   # #4131 source dir, not a crate
        if content is None:
            try: content = open(path, errors='ignore').read()
            except Exception: return False
        return bool(RUST_TEST_RE.search(content))
    return False

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
    return local[:118].rstrip('-') + '-' + hashlib.sha1(local.encode()).hexdigest()[:9]


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
    # #4136 — gate-test-type accepts `@test-type: perf` / `security` on their own
    # (a concern with no layer). Here that read as junk and fell to the
    # heuristic, so werk-phase-budgets.test.sh registered as unit/'' and the
    # nightly counted a speed measurement as a broken test.
    if layer in VALID_CONCERNS and concern is None:
        concern, layer = layer, 'fitness'
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

# #4111 — the registry stored the SOURCE spelling of a name; the runner emits
# the EVALUATED one. Ten registered rows could never join because of it:
#   source  it('escapes newlines to literal \\n')   runner  escapes newlines to literal \n
#   source  @test "the \\$\\$ name differs"          runner  the $$ name differs
# A name that survives its own escapes is the whole point of #4106: a
# registered test must be a test that can actually run.
def _unescape_js(nm):
    """A JS string literal's value, not its source text."""
    out = []
    i = 0
    simple = {'n': '\n', 't': '\t', 'r': '\r', '0': '\0',
              '\\': '\\', "'": "'", '"': '"', '`': '`', '$': '$', '/': '/'}
    while i < len(nm):
        ch = nm[i]
        if ch == '\\' and i + 1 < len(nm):
            nxt = nm[i + 1]
            out.append(simple.get(nxt, nxt))
            i += 2
            continue
        out.append(ch)
        i += 1
    return ''.join(out)

# The escaped-quote-aware form. The old pattern was `"([^"]+)"`, which stopped
# at the first `\"` inside the name and registered a truncated string —
# `lock: chorus-hooks contains no direct Command::new(\` was a real row.
BATS_NAME_RE = re.compile(r'(?m)^[ \t]*@test\s+"((?:[^"\\]|\\.)*)"')

def _unescape_bats(nm):
    """What bash prints for a double-quoted @test name."""
    out = []
    i = 0
    while i < len(nm):
        if nm[i] == '\\' and i + 1 < len(nm) and nm[i + 1] in '"\\$`':
            out.append(nm[i + 1])
            i += 2
            continue
        out.append(nm[i])
        i += 1
    return ''.join(out)

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
        nm = _unescape_js(nm)
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
    if path.endswith('.rs'):
        # #4135 — an `#[ignore]` fn never runs, so registering it mints a name no
        # lane can ever emit (werk-deploy e2e_shared_lib_cascade_and_anti_stale:
        # never-ran every night since #3222 parked it). Skip fns whose attribute
        # block carries #[ignore …] between #[test] and fn.
        # (#[ignore] may sit before OR after #[test]: take the whole attribute block.)
        r = [m.group(2) for m in re.finditer(r'((?:[ \t]*#\[[^\n]*\][ \t]*\n)+)[ \t]*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)', c)
             if re.search(r'#\[(?:tokio::)?test\]', m.group(1)) and '#[ignore' not in m.group(1)]
    # #4106 — anchored to line start: the unanchored pattern also matched a
    # @test declaration written INSIDE a string fixture (a bats suite that
    # builds a little .bats file to run the tagger against registered its
    # fixture's name as a real test — 5 such phantoms, one of them called "x").
    elif path.endswith('.bats'): r = [_unescape_bats(m) for m in BATS_NAME_RE.findall(c)]
    elif re.search(r'\.(test|spec)\.[tj]s$', path): r = jest_case_names(c)
    # #4063 — a shell suite has no per-case grain, so the runner stores ONE
    # verdict per script named by the file (`shell_suite_case`). Registering
    # the basename for .sh is therefore not an invention: it is the identity
    # the runner actually emits, and the two join.
    elif path.endswith('.sh'): return [os.path.basename(path)], c
    else: r = []
    # #4106 — for every OTHER kind the old fallback returned [basename], and
    # nothing emits that name: 90 rows that could only ever read never-ran.
    # The file stays registered (its SourceFile row is written regardless);
    # what is not invented is a case name no runner will ever produce.
    return r, c

# #3924 (with Wren) — discovery walks every test-bearing root, not just
# platform/. proving/ (browser flows) and directing/ (product tests) were
# invisible: SPARQL showed ZERO browser tests in the graph, which is how a
# green land could skip Jeff's phone entirely (#3872). Roots are explicit so
# a new test-bearing tree is a one-line, reviewed widening.
TEST_ROOTS = ("platform", "proving", "directing", "skills")

def _rel(p):
    """Repo-relative, no "./" prefix — the spelling every registry row uses."""
    return p[2:] if p.startswith("./") else p

def discover(roots=None):
    """Every test file under `roots`; roots=None walks the whole tree (#4154:
    kind=test comes from the file, not from a hand-kept root list).

    Paths come back REPO-RELATIVE with no "./" prefix. The tagger walked named
    roots so its paths were naturally bare; walking "." prefixed every one, and
    a "./proving/x.spec.cjs" matches no registry row and no filePath — the
    zero-browser-tests hole (#3872) in a new spelling. Caught by
    3924-declared-wins.bats on 2026-09-13.
    """
    roots = roots if roots is not None else (".",)
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
            if re.search(r'\.bats$|\.(test|spec)\.[cm]?[tj]s$|\.test\.sh$|(_test|test_).*\.py$|\.feature$', f): out.append(_rel(p))
            elif f.startswith('test-') and f.endswith('.sh') and d.rstrip('/').endswith('platform/scripts'): out.append(_rel(p))
            # #4131 — platform/services/shared/ is a SOURCE directory other crates
            # include, not a crate (no Cargo.toml, #4012); its #[test] fns run under
            # the including crate's names, so registering them here minted three
            # rows no lane could ever emit (LANE SILENT every night).
            elif f.endswith('.rs') and 'platform/services/shared/' in p: continue   # relative paths: no leading slash (20:56 run still carried 3 rows)
            elif f.endswith('.rs') and re.search(r'#\[(?:tokio::)?test\]', open(p, errors='ignore').read()): out.append(_rel(p))
    return out

# ── covers-inference and the share gate (#4159) ───────────────────────────────
# Lifted from tag-tests-domain.py when #4154 retired it. These decide WHICH
# domain a test covers, and refuse a corpus where one domain holds too much of
# it. The tagger fetched the legal domain set at import; here it is a parameter,
# so the module stays pure and testable and the caller (the walker, when it
# writes Test rows) supplies the live set.

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


def cardlookup(n, domains=None, fetch=None):
    """The domain a card belongs to, or None. `fetch` is the lookup (injected;
    the tagger hardcoded an HTTP call at import and could not be tested offline)."""
    if fetch is None:
        return None
    try:
        dom = str(fetch(n) or '').lower()
        return dom if (domains is None or dom in domains) else None
    except Exception:
        return None

def covers_for(path, domains=None, fetch=None):
    b = os.path.basename(path).lower()
    for sub, dom in HANDMAP:
        if sub in b: return dom
    if path.startswith("platform/api/tests/handlers/"): return "domains"
    m = re.match(r'platform/tests/(\d{3,4})-', path)
    if m: return cardlookup(m.group(1), domains, fetch) or "services"
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


if __name__ == "__main__":
    # #4159 — the hermetic seams the test suites drive, carried over verbatim from
    # tag-tests-domain.py's __main__ when #4154 retired it. A test brings its own
    # world (#3528): every seam reads a path or a fixture and touches no store.
    #   --names-of <path>     the case names the registry would hold (#4022)
    #   --covers-of <path>    the domain one path covers (#3996)
    #   --check-shares <json> the share gate over a {"domain": count} fixture
    #   --no-case-files       discovered files that name no case (#4106)
    if len(sys.argv) >= 3 and sys.argv[1] == "--names-of":
        for nm in case_names(sys.argv[2])[0]:
            print(nm)
        sys.exit(0)
    if len(sys.argv) >= 3 and sys.argv[1] == "--covers-of":
        print(covers_for(sys.argv[2]))
        sys.exit(0)
    if len(sys.argv) >= 3 and sys.argv[1] == "--check-shares":
        assert_shares(json.load(open(sys.argv[2])))
        print("shares ok")
        sys.exit(0)
    if len(sys.argv) >= 2 and sys.argv[1] == "--no-case-files":
        nc = [p for p in discover() if not case_names(p)[0]]
        for p in nc:
            print(p)
        print(no_case_report(nc), file=sys.stderr)
        sys.exit(0)
    sys.exit("testfiles.py — a library. Seams: --names-of | --covers-of | --check-shares | --no-case-files")
