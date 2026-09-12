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
import os, re, hashlib

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

def discover(roots=None):
    """roots=None walks the whole tree (direction 4: kind=test comes from the file, not a root list)."""
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
            if re.search(r'\.bats$|\.(test|spec)\.[cm]?[tj]s$|\.test\.sh$|(_test|test_).*\.py$|\.feature$', f): out.append(p)
            elif f.startswith('test-') and f.endswith('.sh') and d.rstrip('/').endswith('platform/scripts'): out.append(p)
            # #4131 — platform/services/shared/ is a SOURCE directory other crates
            # include, not a crate (no Cargo.toml, #4012); its #[test] fns run under
            # the including crate's names, so registering them here minted three
            # rows no lane could ever emit (LANE SILENT every night).
            elif f.endswith('.rs') and 'platform/services/shared/' in p: continue   # relative paths: no leading slash (20:56 run still carried 3 rows)
            elif f.endswith('.rs') and re.search(r'#\[(?:tokio::)?test\]', open(p, errors='ignore').read()): out.append(p)
    return out

