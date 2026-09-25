//! Test CASE rows (#4185) — the pure half.
//!
//! #4154 and #4173 said "test file" and the crawler was built to that word: it
//! tags a file kind=test and writes nothing about the cases inside it. The
//! per-case registry (7,883 rows the runner selects from) was written by the
//! hydrator #4173 deleted, and nothing replaced it — frozen at 2026-09-13 01:42.
//! Jeff, 2026-09-16: "to me the crawler writes and updates tests graph data."
//!
//! Everything here is ported from platform/scripts/testfiles.py, receipts and
//! all (#4022 #4106 #4111 #4135 #3924 #3996 #3872 #3974 #4131), so the rows the
//! crawler writes are the rows the registry already holds — one new writer, not
//! a new dialect. std only, no regex crate: each rule is a small matcher with
//! the Python pattern it stands for named beside it.

use crate::TreeRead;

pub const LAYERS: [&str; 7] = [
    "unit",
    "integration",
    "bdd",
    "e2e",
    "contract",
    "fitness",
    "smoke",
];
pub const CONCERNS: [&str; 4] = ["api", "ui", "perf", "security"];

fn layer_of(s: &str) -> Option<&'static str> {
    LAYERS.iter().find(|l| **l == s).copied()
}
fn concern_of(s: &str) -> Option<&'static str> {
    CONCERNS.iter().find(|c| **c == s).copied()
}

// ─────────────────────────── the authored header ───────────────────────────

/// #3924 — the AUTHORED declaration wins. `^\s*(?://|#|\*)\s*@test-type:\s*
/// ([a-z0-9-]+)(?::([a-z0-9-]+))?` with re.I|re.M over the first 2,000 chars.
/// The first line shaped like a header decides: a junk layer is `None` (falls
/// to the heuristic, flagged inferred), never a fabricated row. #4136 — a bare
/// concern (`@test-type: perf`) reads as fitness:perf, the way gate-test-type
/// accepts it.
pub fn declared(content: &str) -> Option<(&'static str, Option<&'static str>)> {
    let head: String = content.chars().take(2000).collect();
    for line in head.lines() {
        let t = line.trim_start();
        let t = if let Some(r) = t.strip_prefix("//") {
            r
        } else if let Some(r) = t.strip_prefix('#') {
            r
        } else if let Some(r) = t.strip_prefix('*') {
            r
        } else {
            continue;
        };
        let t = t.trim_start();
        const TAG: &str = "@test-type:";
        if !t.is_char_boundary(TAG.len().min(t.len()))
            || t.len() < TAG.len()
            || !t[..TAG.len()].eq_ignore_ascii_case(TAG)
        {
            continue;
        }
        let t = t[TAG.len()..].trim_start();
        let word = |s: &str| -> String {
            s.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect::<String>()
                .to_ascii_lowercase()
        };
        let layer = word(t);
        if layer.is_empty() {
            continue; // `@test-type:` with nothing after it is not a match for [a-z0-9-]+
        }
        let rest = &t[layer.len()..];
        let concern = rest.strip_prefix(':').map(word).filter(|c| !c.is_empty());
        let (layer, concern) = match (layer_of(&layer), concern) {
            // #4136 — a concern with no layer
            (None, None) if concern_of(&layer).is_some() => ("fitness", concern_of(&layer)),
            (None, _) => return None,
            (Some(l), c) => (l, c.and_then(|c| concern_of(&c))),
        };
        return Some((layer, concern));
    }
    None
}

// ─────────────────────────── the heuristic ───────────────────────────

/// `a` followed by `b` with at most `gap` chars between them and no newline in
/// the gap — the `[^\n]{0,N}` / `.{0,N}` / `.?` shapes. Case-insensitive on an
/// already-lowercased haystack.
fn near(hay: &str, a: &str, gap: usize, b: &str) -> bool {
    let mut from = 0;
    while let Some(i) = hay[from..].find(a) {
        let after = from + i + a.len();
        let window_end = (after + gap + b.len()).min(hay.len());
        let mut end = window_end;
        while !hay.is_char_boundary(end) {
            end -= 1;
        }
        let window = &hay[after..end];
        if let Some(j) = window.find(b) {
            if !window[..j].contains('\n') {
                return true;
            }
        }
        from = after;
    }
    false
}

fn any_of(hay: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| hay.contains(n))
}

/// `\bfetch\(`: the char before is not [A-Za-z0-9_].
fn word_start_call(hay: &str, word: &str) -> bool {
    let mut from = 0;
    while let Some(i) = hay[from..].find(word) {
        let at = from + i;
        let prev_ok = at == 0
            || !hay[..at]
                .chars()
                .next_back()
                .map(|c| c.is_ascii_alphanumeric() || c == '_')
                .unwrap_or(false);
        if prev_ok {
            return true;
        }
        from = at + word.len();
    }
    false
}

/// `await\s+[\w.]*(get|post|request|query)\(`
fn await_call(hay: &str) -> bool {
    let mut from = 0;
    while let Some(i) = hay[from..].find("await") {
        let at = from + i + "await".len();
        let rest = &hay[at..];
        let ws = rest.len() - rest.trim_start().len();
        if ws > 0 {
            let rest = rest.trim_start();
            let ident: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.')
                .collect();
            let tail = &rest[ident.len()..];
            if tail.starts_with('(')
                && ["get", "post", "request", "query"]
                    .iter()
                    .any(|v| ident.ends_with(v))
            {
                return true;
            }
        }
        from = at;
    }
    false
}

/// `curl\s+(-\w+\s+|--\S+\s+)*["']?https?://(localhost|127\.)`
fn curl_localhost(hay: &str) -> bool {
    let mut from = 0;
    while let Some(i) = hay[from..].find("curl") {
        let mut rest = &hay[from + i + 4..];
        from = from + i + 4;
        if !rest.starts_with(|c: char| c.is_whitespace()) {
            continue;
        }
        rest = rest.trim_start();
        // flags
        loop {
            if rest.starts_with('-') {
                let tok: &str = rest.split_whitespace().next().unwrap_or("");
                let ok = if tok.starts_with("--") {
                    tok.len() > 2
                } else {
                    tok.len() > 1
                        && tok[1..]
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_')
                };
                if !ok {
                    break;
                }
                let after = &rest[tok.len()..];
                if !after.starts_with(|c: char| c.is_whitespace()) {
                    break;
                }
                rest = after.trim_start();
            } else {
                break;
            }
        }
        let rest = rest
            .strip_prefix('"')
            .or_else(|| rest.strip_prefix('\''))
            .unwrap_or(rest);
        let rest = rest
            .strip_prefix("https://")
            .or_else(|| rest.strip_prefix("http://"));
        if let Some(r) = rest {
            if r.starts_with("localhost") || r.starts_with("127.") {
                return true;
            }
        }
    }
    false
}

/// `^\s*launchctl\s+(kickstart|bootstrap|bootout|list|print|kill)` (re.M)
fn launchctl_line(hay: &str) -> bool {
    hay.lines().any(|l| {
        let t = l.trim_start();
        t.strip_prefix("launchctl")
            .filter(|r| r.starts_with(|c: char| c.is_whitespace()))
            .map(|r| {
                let v = r.trim_start();
                ["kickstart", "bootstrap", "bootout", "list", "print", "kill"]
                    .iter()
                    .any(|k| v.starts_with(k))
            })
            .unwrap_or(false)
    })
}

/// `Command::new\(\s*["'](launchctl|osascript|curl|kickstart|fuseki|gitleaks|git)`
fn command_new(hay: &str) -> bool {
    let mut from = 0;
    while let Some(i) = hay[from..].find("command::new(") {
        let rest = hay[from + i + "command::new(".len()..].trim_start();
        from = from + i + "command::new(".len();
        if let Some(r) = rest.strip_prefix('"').or_else(|| rest.strip_prefix('\'')) {
            if [
                "launchctl",
                "osascript",
                "curl",
                "kickstart",
                "fuseki",
                "gitleaks",
                "git",
            ]
            .iter()
            .any(|k| r.starts_with(k))
            {
                return true;
            }
        }
    }
    false
}

/// `X\s+Y` — word, whitespace, word.
fn ws_pair(hay: &str, a: &str, b: &str) -> bool {
    let mut from = 0;
    while let Some(i) = hay[from..].find(a) {
        let rest = &hay[from + i + a.len()..];
        from = from + i + a.len();
        if rest.starts_with(|c: char| c.is_whitespace()) && rest.trim_start().starts_with(b) {
            return true;
        }
    }
    false
}

/// The EXEC regex from testfiles.py, clause by clause.
fn exec_signal(c: &str) -> bool {
    curl_localhost(c)
        || curl_port(c)
        || command_new(c)
        || launchctl_line(c)
        || c.contains("sparqlclient")
        || c.contains(".query(")
        || word_start_call(c, "fetch(")
        || await_call(c)
        || near(c, "post", 40, "fuseki")
        || near(c, "post", 40, "3030")
        || localhost_3ddd(c)
        || ws_pair(c, "run", "gitleaks")
        || ws_pair(c, "gitleaks", "detect")
        || ws_pair(c, "gitleaks", "protect")
        || ws_pair(c, "gitleaks", "--")
        || ws_pair(c, "pre-commit", "run")
        || git_commit(c)
}

/// `\bgit\s+commit`
fn git_commit(c: &str) -> bool {
    let mut from = 0;
    while let Some(i) = c[from..].find("git") {
        let at = from + i;
        from = at + 3;
        let prev_ok = at == 0
            || !c[..at]
                .chars()
                .next_back()
                .map(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                .unwrap_or(false);
        let rest = &c[at + 3..];
        if prev_ok
            && rest.starts_with(|ch: char| ch.is_whitespace())
            && rest.trim_start().starts_with("commit")
        {
            return true;
        }
    }
    false
}

/// `curl[^\n]{0,40}:3[0-9]{3}`
fn curl_port(c: &str) -> bool {
    let mut from = 0;
    while let Some(i) = c[from..].find("curl") {
        let after = from + i + 4;
        let line_end = c[after..].find('\n').map(|j| after + j).unwrap_or(c.len());
        let window = &c[after..line_end.min(after + 46)];
        let b = window.as_bytes();
        for k in 0..b.len().saturating_sub(4) {
            if b[k] == b':'
                && b[k + 1] == b'3'
                && b[k + 2].is_ascii_digit()
                && b[k + 3].is_ascii_digit()
                && b[k + 4].is_ascii_digit()
            {
                return true;
            }
        }
        from = after;
    }
    false
}

/// `http://localhost:3[0-9]{3}`
fn localhost_3ddd(c: &str) -> bool {
    let mut from = 0;
    while let Some(i) = c[from..].find("http://localhost:3") {
        let after = from + i + "http://localhost:3".len();
        let b = c.as_bytes();
        if after + 3 <= b.len() && b[after..after + 3].iter().all(|x| x.is_ascii_digit()) {
            return true;
        }
        from = after;
    }
    false
}

/// What the heuristic says about a test file: (layer, hermeticity, concern).
/// Ported from testfiles.py classify(): same order, same defaults. The final
/// `unit / hermetic` is the FALLBACK — a file no rule matched — and the caller
/// reports it as inferred, not declared.
pub fn classify_case(
    path: &str,
    content: &str,
) -> (&'static str, &'static str, Option<&'static str>) {
    let pc = format!("{path}\n{content}").to_ascii_lowercase();
    let c = content.to_ascii_lowercase();
    let concern = if any_of(&pc, &["gitleaks", "write_scrubber", "sensitive-path"]) {
        Some("security")
    } else if any_of(&pc, &["#[bench]", "criterion"])
        || near(&pc, "latency", 1, "budget")
        || near(&pc, "throughput", 1, "budget")
    {
        Some("perf")
    } else {
        None
    };
    let in_crate = path.ends_with(".rs") && path.contains("/src/");
    if any_of(&pc, &[".feature", "cucumber"])
        || near(&pc, "flow", 12, "validator")
        || near(&pc, "scenario", 1, "runner")
    {
        return ("bdd", "hermetic", concern);
    }
    if near(&c, "env", 1, "up") && env_up_teardown(&c)
        || near(&c, "launchd", 20, "lifecycle")
        || near(&c, "full", 1, "pipeline")
        || c.contains("both_slots")
    {
        return ("e2e", "needs-stack", concern);
    }
    if exec_signal(&c) && !in_crate {
        return ("integration", "needs-stack", concern);
    }
    ("unit", "hermetic", concern)
}

/// `env.?up[^\n]{0,40}teardown`
fn env_up_teardown(c: &str) -> bool {
    let mut from = 0;
    while let Some(i) = c[from..].find("env") {
        let at = from + i + 3;
        from = at;
        let rest = &c[at..];
        let rest = if let Some(r) = rest.strip_prefix("up") {
            r
        } else {
            // `.?` — exactly one non-newline char between "env" and "up"
            match rest.chars().next() {
                Some(ch) if ch != '\n' => match rest[ch.len_utf8()..].strip_prefix("up") {
                    Some(r) => r,
                    None => continue,
                },
                _ => continue,
            }
        };
        let line_end = rest.find('\n').unwrap_or(rest.len());
        let mut end = line_end.min(48);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        if rest[..end].contains("teardown") {
            return true;
        }
    }
    false
}

/// The whole classification for one file: the authored header if it declares
/// one, else the heuristic. `declared` says which, so the run can count them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileClass {
    pub layer: &'static str,
    pub hermeticity: &'static str,
    pub concern: Option<&'static str>,
    pub declared: bool,
}

/// #4292 — the `@test-type:` header line itself names `needs-stack`.
fn header_says_needs_stack(content: &str) -> bool {
    content
        .lines()
        .take(40)
        .any(|l| l.contains("@test-type:") && l.contains("needs-stack"))
}

pub fn file_class(path: &str, content: &str) -> FileClass {
    let (h_layer, hermeticity, h_concern) = classify_case(path, content);
    match declared(content) {
        // authored wins on BOTH axes it declares; hermeticity stays heuristic
        // unless the header line says `needs-stack` (#4292: a test that reads
        // Fuseki through a library call has no exec signal to infer from)
        Some((layer, concern)) => FileClass {
            layer,
            hermeticity: if header_says_needs_stack(content) { "needs-stack" } else { hermeticity },
            concern: concern.or(h_concern),
            declared: true,
        },
        None => FileClass {
            layer: h_layer,
            hermeticity,
            concern: h_concern,
            declared: false,
        },
    }
}

// ─────────────────────────── the case names ───────────────────────────

/// A JS string literal's VALUE, not its source (#4111).
fn unescape_js(nm: &str) -> String {
    let mut out = String::with_capacity(nm.len());
    let mut it = nm.chars();
    while let Some(ch) = it.next() {
        if ch == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('0') => out.push('\0'),
                Some(o) => out.push(o),
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// What bash prints for a double-quoted @test name (#4111).
fn unescape_bats(nm: &str) -> String {
    let mut out = String::with_capacity(nm.len());
    let mut it = nm.chars().peekable();
    while let Some(ch) = it.next() {
        if ch == '\\' {
            match it.peek() {
                Some(&n) if n == '"' || n == '\\' || n == '$' || n == '`' => {
                    out.push(n);
                    it.next();
                }
                _ => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// The body of a string literal opened at `s[0]` (the quote char), honouring
/// backslash escapes; None if unterminated.
fn string_literal(s: &str) -> Option<&str> {
    let q = s.chars().next()?;
    let b = s.as_bytes();
    let mut i = 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            c if c == q as u8 => return Some(&s[1..i]),
            _ => i += 1,
        }
    }
    None
}

/// #4022 / #4106 — jest names. `(?<![.\w$])(?:it|test)(?:\.(?:only|skip|each|
/// concurrent))?\s*\(\s*` then a same-quote-delimited literal. A name built by
/// interpolation is a template, not a name, and is dropped.
pub fn jest_case_names(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let b = source.as_bytes();
    let mut i = 0;
    // #4199 — a file may declare its cases through an ALIAS of test/it:
    // `const testWhenWritable = storeWritable ? test : test.skip;` (two api
    // integration suites, 2026-09-17). The alias is a declaration keyword too.
    let aliases = test_aliases(source);
    while i < b.len() {
        if !source.is_char_boundary(i) {
            i += 1;
            continue;
        }
        // an alias is checked FIRST: `testWhenWritable(` starts with `test`, and
        // the shorter match would read it as `test` followed by junk
        let kw = if let Some(a) = aliases.iter().find(|a| source[i..].starts_with(a.as_str())) {
            a.len()
        } else if source[i..].starts_with("it") {
            2
        } else if source[i..].starts_with("test") {
            4
        } else {
            i += 1;
            continue;
        };
        let prev_ok = i == 0
            || !matches!(b[i - 1], b'.' | b'$' | b'_' | b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z');
        if !prev_ok {
            i += kw;
            continue;
        }
        let mut rest = &source[i + kw..];
        for m in [".only", ".skip", ".each", ".concurrent"] {
            if let Some(r) = rest.strip_prefix(m) {
                rest = r;
                break;
            }
        }
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('(') else {
            i += kw;
            continue;
        };
        let rest = rest.trim_start();
        if !rest.starts_with('\'') && !rest.starts_with('"') && !rest.starts_with('`') {
            i += kw;
            continue;
        }
        if let Some(lit) = string_literal(rest) {
            let nm = unescape_js(lit);
            if !nm.contains("${") {
                out.push(nm);
            }
        }
        i += kw;
    }
    out
}

/// Names bound to `test` / `it` (or their `.skip` / `.only` forms) by a const:
/// `const X = cond ? test : test.skip;` — X declares cases like `test` does.
pub fn test_aliases(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        let l = line.trim_start();
        let Some(rest) = l.strip_prefix("const ") else {
            continue;
        };
        let Some((name, rhs)) = rest.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
        {
            continue;
        }
        let rhs = rhs.trim();
        let mentions_test = rhs
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
            .any(|tok| {
                matches!(
                    tok,
                    "test" | "it" | "test.skip" | "it.skip" | "test.only" | "it.only"
                )
            });
        if mentions_test && name != "test" && name != "it" {
            out.push(name.to_string());
        }
    }
    out
}

/// `(?m)^[ \t]*@test\s+"((?:[^"\\]|\\.)*)"` — anchored to line start so a
/// fixture built inside a string is not a declaration (#4106).
fn bats_case_names(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines() {
        let t = line.trim_start_matches([' ', '\t']);
        let Some(r) = t.strip_prefix("@test") else {
            continue;
        };
        if !r.starts_with(|c: char| c.is_whitespace()) {
            continue;
        }
        let r = r.trim_start();
        if !r.starts_with('"') {
            continue;
        }
        if let Some(lit) = string_literal(r) {
            out.push(unescape_bats(lit));
        }
    }
    out
}

/// `((?:[ \t]*#\[[^\n]*\][ \t]*\n)+)[ \t]*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)`
/// with `#[test]` / `#[tokio::test]` in the block and no `#[ignore` (#4135).
fn rust_case_names(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut block: Vec<&str> = Vec::new();
    for line in source.lines() {
        let t = line.trim_matches([' ', '\t']);
        if t.starts_with("#[") && t.ends_with(']') {
            block.push(t);
            continue;
        }
        if !block.is_empty() {
            let is_test = block.iter().any(|a| {
                *a == "#[test]"
                    || *a == "#[tokio::test]"
                    || a.contains("#[test]")
                    || a.contains("#[tokio::test]")
            });
            let ignored = block.iter().any(|a| a.contains("#[ignore"));
            let mut sig = t;
            for kw in ["pub", "async"] {
                if let Some(r) = sig.strip_prefix(kw) {
                    if r.starts_with(|c: char| c.is_whitespace()) {
                        sig = r.trim_start();
                    }
                }
            }
            if let Some(r) = sig.strip_prefix("fn") {
                if r.starts_with(|c: char| c.is_whitespace()) {
                    let name: String = r
                        .trim_start()
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    if is_test && !ignored && !name.is_empty() {
                        out.push(name);
                    }
                }
            }
            block.clear();
        }
    }
    out
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The case names a runner will actually emit for this file (#4106: never an
/// invented one). A .sh suite is ONE case named by its file — that is the
/// identity the runner's shell lane stores (#4063). Kinds with no extractor
/// and no lane (.py, .feature, .spec.cjs) yield nothing and are REPORTED.
pub fn case_names(path: &str, content: &str) -> Vec<String> {
    let b = basename(path);
    if path.ends_with(".rs") {
        rust_case_names(content)
    } else if path.ends_with(".bats") {
        bats_case_names(content)
    } else if [".test.ts", ".test.js", ".spec.ts", ".spec.js"]
        .iter()
        .any(|s| b.ends_with(s))
    {
        jest_case_names(content)
    } else if path.ends_with(".sh") {
        vec![b.to_string()]
    } else if path.ends_with(".feature") && path.starts_with(BDD_LANE_DIR) {
        // #4292 — the scenarios the bdd lane actually runs, by the name
        // cucumber reports them under. Only files in the lane's directory;
        // a scenario the default profile excludes (@wip, @e2e) is not run,
        // so it is not registered (#4106: never a name no runner emits).
        feature_scenario_names(content)
    } else if b.starts_with("test_") && b.ends_with(".py") {
        // #4292 — unittest cases as `Class.test_method`, the identity the
        // python lane reports (`python3 -m unittest -v`). A file with no
        // unittest.TestCase class yields nothing: no lane runs pytest style.
        unittest_case_names(content)
    } else if b.ends_with(".spec.cjs") || b.ends_with(".spec.mjs") {
        // #4185 — a playwright spec is ONE case named by its file: that is the
        // identity the ui lane stores and quarantines by (werk-test lib.rs,
        // #4045: "the tests domain registers ui specs at FILE granularity with
        // testName = the file's basename"). testfiles.py returned nothing here,
        // which is why 13 browser flows had no row at all.
        vec![b.to_string()]
    } else {
        Vec::new()
    }
}

/// #4292 — the directory the bdd lane runs cucumber over (platform/tests is
/// the cucumber package; its default profile reads features/**).
pub const BDD_LANE_DIR: &str = "platform/tests/features/";

/// The tags the cucumber default profile excludes (platform/tests/cucumber.js:
/// `not @e2e and not @wip` unless RUN_INTEGRATION is set).
const CUKE_EXCLUDED: [&str; 2] = ["@wip", "@e2e"];

fn tag_line_excludes(line: &str) -> bool {
    line.split_whitespace().any(|t| CUKE_EXCLUDED.contains(&t))
}

/// Scenario and Scenario Outline names in file order, deduplicated (an
/// outline's examples all report under the outline's name), skipping any
/// scenario whose own tags or whose feature's tags the default profile
/// excludes.
pub fn feature_scenario_names(source: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut feature_off = false;
    let mut pending_tags_off = false;
    for line in source.lines() {
        let t = line.trim();
        if t.starts_with('@') {
            pending_tags_off |= tag_line_excludes(t);
            continue;
        }
        if t.starts_with("Feature:") {
            feature_off = pending_tags_off;
            pending_tags_off = false;
            continue;
        }
        let name = t
            .strip_prefix("Scenario Outline:")
            .or_else(|| t.strip_prefix("Scenario Template:"))
            .or_else(|| t.strip_prefix("Scenario:"))
            .or_else(|| t.strip_prefix("Example:"));
        if let Some(n) = name {
            let n = n.trim().to_string();
            if !feature_off && !pending_tags_off && !n.is_empty() && !out.contains(&n) {
                out.push(n);
            }
            pending_tags_off = false;
        } else if !t.is_empty() && !t.starts_with('#') {
            // a tag line only binds to the scenario directly below it
            pending_tags_off = false;
        }
    }
    out
}

/// `Class.test_method` for every test method of every class that derives a
/// unittest TestCase (directly: `(unittest.TestCase)` or `(TestCase)`).
pub fn unittest_case_names(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut class: Option<String> = None;
    for line in source.lines() {
        if let Some(rest) = line.strip_prefix("class ") {
            class = rest.split_once('(').and_then(|(name, bases)| {
                bases.contains("TestCase").then(|| name.trim().to_string())
            });
            continue;
        }
        if !line.starts_with(' ') && !line.starts_with('\t') && !line.trim().is_empty() {
            class = None;
            continue;
        }
        let Some(c) = &class else { continue };
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("def test").or_else(|| t.strip_prefix("async def test")) {
            if let Some((tail, _)) = rest.split_once('(') {
                out.push(format!("{c}.test{tail}"));
            }
        }
    }
    out
}

/// #4199 — WHY a test file yields no runnable case. The morning line counts
/// only `unextracted` as missing; the rest are what they are:
///   ignored-only   .rs whose every #[test] is #[ignore]d — nothing to run
///   no-tests       .rs that declares test modules or mentions #[test] in prose, no fn
///   no-lane        .py / .feature no runner lane executes by case
///   unextracted    a real suite whose cases the extractors cannot read yet
pub fn no_case_bucket(path: &str, content: &str) -> &'static str {
    if path.ends_with(".rs") {
        let has_attr = content.contains("#[test]") || content.contains("#[tokio::test]");
        let has_ignore = content.contains("#[ignore");
        if has_attr && has_ignore {
            return "ignored-only";
        }
        return "no-tests";
    }
    // #4292 — a feature in the lane's directory whose every scenario the
    // default profile excludes (@wip, @e2e) is switched off, exactly like a
    // .rs whose every test is #[ignore]d: nothing to run.
    if path.ends_with(".feature") && path.starts_with(BDD_LANE_DIR) {
        return "ignored-only";
    }
    if path.ends_with(".py") || path.ends_with(".feature") {
        return "no-lane";
    }
    "unextracted"
}

/// The buckets summarised: `unextracted 2 · ignored-only 2 · no-tests 2 · no-lane 3`.
pub fn no_case_summary(buckets: &[&str]) -> (usize, String) {
    let order = ["unextracted", "ignored-only", "no-tests", "no-lane"];
    let parts: Vec<String> = order
        .iter()
        .map(|k| (k, buckets.iter().filter(|b| *b == k).count()))
        .filter(|(_, n)| *n > 0)
        .map(|(k, n)| format!("{k} {n}"))
        .collect();
    (
        buckets.iter().filter(|b| **b == "unextracted").count(),
        parts.join(" · "),
    )
}

/// #4106 — one line naming the files that yield no runnable case, by kind.
pub fn no_case_report(paths: &[String]) -> String {
    if paths.is_empty() {
        return "no-case files: none — every registered file names at least one case".to_string();
    }
    let mut counts: Vec<(String, usize)> = Vec::new();
    for p in paths {
        let ext = p.rsplit('.').next().unwrap_or(p).to_string();
        match counts.iter_mut().find(|(k, _)| *k == ext) {
            Some(c) => c.1 += 1,
            None => counts.push((ext, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let kinds: Vec<String> = counts.iter().map(|(k, v)| format!("{k} {v}")).collect();
    format!(
        "no-case files: {} registered file(s) yield no runnable case ({}) — they need a lane or an extractor, not an invented name",
        paths.len(),
        kinds.join(", ")
    )
}

// ─────────────────────────── covers ───────────────────────────

// #4201 — the folder rule (HANDMAP / PREFIX / KW / covers_for) is retired. The
// domain a test covers is read from the FILE by the five rules in domain.rs;
// the folder is never a rule and "services" is never a default.

/// The home of a test the five rules cannot place, or place two ways. Jeff,
/// 2026-09-17 14:09: "we have a tests domain" — the set of all tests is a real
/// Domain row, so an unplaced test covers `tests`, never a folder, never a guess.
pub const UNPLACED_HOME: &str = "tests";

/// The domain a case row COVERS: the security lane (#3922) selects its rows by
/// `covers=security`, so a declared security concern pins it; otherwise the
/// five rules' placement, or the tests domain when the file is unplaced or in
/// conflict (both still listed by name on the line).
pub fn covers_from(placement: Option<&str>, concern: Option<&str>) -> String {
    if concern == Some("security") {
        return "security".to_string();
    }
    placement.unwrap_or(UNPLACED_HOME).to_string()
}

// ─────────────────────────── the share gate ───────────────────────────

/// #3996 / #4022 — refuse a corpus one domain holds too much of; stand down
/// below the corpus floor (a one-file corpus is always 100% one domain).
pub fn check_shares(counts: &[(String, usize)], cap: f64, min_corpus: usize) -> Result<(), String> {
    let total: usize = counts.iter().map(|(_, n)| *n).sum();
    if total < min_corpus {
        return Ok(());
    }
    let total_f = total.max(1) as f64;
    let mut worst: Vec<&(String, usize)> = counts.iter().collect();
    worst.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    if let Some((d, n)) = worst.iter().find(|(_, n)| (*n as f64) / total_f > cap) {
        let hist: Vec<String> = worst
            .iter()
            .take(6)
            .map(|(d, n)| format!("{d}={n}({}%)", n * 100 / total))
            .collect();
        return Err(format!(
            "covers-share gate RED (#3996): {d} holds {n}/{total} (> {:.0}%) — refusing to write an over-broad corpus. top: {}",
            cap * 100.0,
            hist.join(" ")
        ));
    }
    Ok(())
}

// ─────────────────────────── the rows ───────────────────────────

/// One case as this run wants it in the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseRow {
    pub file: String,
    pub case: String,
    pub covers: String,
    pub layer: String,
    pub hermeticity: String,
    pub concern: Option<String>,
    /// The CodeFile row this case lives in (stable_name of the path).
    pub in_file: String,
}

/// #4162 — the fields a Test row no longer carries. The API refuses them as
/// off-model; a replace strips any the served row still has.
pub const RETIRED_TEST_FIELDS: [&str; 2] = ["pyramidLayer", "testConcern"];

/// #4162 — the ONE answer to "what kind of proving is this": the declared
/// concern when it is a kind of proving (ui, perf, security), else the layer,
/// else `unclassified` — never empty, never a guessed default. `api` is not a
/// kind of proving (it restated covers) and falls through to the layer.
pub fn test_type(layer: &str, concern: Option<&str>) -> String {
    match concern {
        Some(c @ ("ui" | "perf" | "security")) => c.to_string(),
        _ if !layer.trim().is_empty() => layer.to_string(),
        _ => "unclassified".to_string(),
    }
}

impl CaseRow {
    pub fn test_type(&self) -> String {
        test_type(&self.layer, self.concern.as_deref())
    }

    /// The fields the crawler OWNS on a Test row. Everything else the row
    /// carries (quarantine, validityClass) is a person's and is preserved.
    pub fn owned_fields(&self) -> Vec<(String, String)> {
        let mut v = vec![
            ("filePath".to_string(), self.file.clone()),
            ("testName".to_string(), self.case.clone()),
            ("inFile".to_string(), self.in_file.clone()),
            ("testType".to_string(), self.test_type()),
            ("hermeticity".to_string(), self.hermeticity.clone()),
        ];
        // #4201 — an unplaced or conflicted file carries NO covers, never a guess
        if !self.covers.is_empty() {
            v.push(("covers".to_string(), self.covers.clone()));
        }
        v
    }
}

/// One Test row as the door serves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseInGraph {
    pub name: String,
    pub file: String,
    pub case: String,
    pub fields: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaseAction {
    Post(CaseRow),
    /// The served row's name, and what to write over it.
    Replace {
        name: String,
        row: CaseRow,
    },
    Unchanged {
        file: String,
        case: String,
    },
    Delete {
        name: String,
        file: String,
        case: String,
    },
}

/// Deterministic row key for a NEW case: a readable slug plus a digest of the
/// exact (path, name) pair, the same reasoning as `stable_name` — a slug alone
/// maps two names differing only in case or punctuation to one row.
pub fn case_row_name(file: &str, case: &str) -> String {
    let raw = format!("{file}\u{0}{case}");
    let mut slug = String::new();
    let mut last_dash = false;
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    let mut end = slug.len().min(100);
    while !slug.is_char_boundary(end) {
        end -= 1;
    }
    let slug = slug[..end].trim_end_matches('-');
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in raw.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    format!("test-{slug}-{:08x}", (h & 0xffff_ffff) as u32)
}

fn served(fields: &[(String, String)], k: &str) -> String {
    fields
        .iter()
        .find(|(f, _)| f == k)
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

/// An edge as the door SERVES it carries the target kind's mint prefix
/// (`code-file-file-platform-…`) while the value the door ACCEPTS is bare
/// (`file-platform-…`) — the same asymmetry strip_named_prefix answers on
/// write. Measured on the variant 2026-09-16: every row read as changed every
/// run, so the second run rewrote all three rows and idempotence was false.
fn same_target(served_value: &str, want: &str) -> bool {
    served_value == want
        || (served_value.len() > want.len()
            && served_value.ends_with(want)
            && served_value[..served_value.len() - want.len()].ends_with('-'))
}

/// Does the served row already say what this run would write? Compared on the
/// owned fields only. #4162 — a row still carrying a retired field does NOT
/// match, so the next pass rewrites it with testType (the migration).
pub fn row_matches(row: &CaseRow, g: &CaseInGraph) -> bool {
    same_target(&served(&g.fields, "inFile"), &row.in_file)
        && same_target(&served(&g.fields, "covers"), &row.covers)
        && served(&g.fields, "testType") == row.test_type()
        && served(&g.fields, "hermeticity") == row.hermeticity
        && RETIRED_TEST_FIELDS.iter().all(|f| served(&g.fields, f).is_empty())
}

/// The whole decision for case rows, pure.
///
/// `desired` — every case this run parsed out of `parsed_files` (the test
/// files it positively READ this run). `removed_files` — paths git says left
/// the tree (a delta's D / R entries). `read` — how well the tree was read.
///
/// Deletes come from three positive facts and nothing else: a file this run
/// parsed no longer contains the case; git said the file is gone; or, on a
/// COMPLETE walk, the graph names a file the tree does not have. A Partial
/// read deletes nothing (#4022: "I cannot see it" is not "it is gone").
pub fn plan_cases(
    desired: &[CaseRow],
    parsed_files: &[String],
    removed_files: &[String],
    graph: &[CaseInGraph],
    read: TreeRead,
) -> Vec<CaseAction> {
    let mut out = Vec::new();
    let mut matched: Vec<bool> = vec![false; graph.len()];
    let mut seen: Vec<(&str, &str)> = Vec::new();
    for row in desired {
        // two `it('x')` in one file are one identity; the second is not a second row
        if seen.iter().any(|(f, c)| *f == row.file && *c == row.case) {
            continue;
        }
        seen.push((&row.file, &row.case));
        let mut hits = graph
            .iter()
            .enumerate()
            .filter(|(_, g)| g.file == row.file && g.case == row.case);
        match hits.next() {
            None => out.push(CaseAction::Post(row.clone())),
            Some((i, g)) => {
                matched[i] = true;
                if row_matches(row, g) {
                    out.push(CaseAction::Unchanged {
                        file: row.file.clone(),
                        case: row.case.clone(),
                    });
                } else {
                    out.push(CaseAction::Replace {
                        name: g.name.clone(),
                        row: row.clone(),
                    });
                }
                // a legacy duplicate of the same identity is an extra row
                for (j, g2) in hits {
                    matched[j] = true;
                    out.push(CaseAction::Delete {
                        name: g2.name.clone(),
                        file: g2.file.clone(),
                        case: g2.case.clone(),
                    });
                }
            }
        }
    }
    if read == TreeRead::Partial {
        return out;
    }
    for (i, g) in graph.iter().enumerate() {
        if matched[i] {
            continue;
        }
        let file_parsed = parsed_files.contains(&g.file);
        let file_removed = removed_files.contains(&g.file);
        let orphan_on_full = read == TreeRead::Complete;
        if file_parsed || file_removed || orphan_on_full {
            out.push(CaseAction::Delete {
                name: g.name.clone(),
                file: g.file.clone(),
                case: g.case.clone(),
            });
        }
    }
    out
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CaseCounts {
    pub posted: usize,
    pub replaced: usize,
    pub unchanged: usize,
    pub deleted: usize,
}

pub fn case_counts(actions: &[CaseAction]) -> CaseCounts {
    let mut c = CaseCounts::default();
    for a in actions {
        match a {
            CaseAction::Post(_) => c.posted += 1,
            CaseAction::Replace { .. } => c.replaced += 1,
            CaseAction::Unchanged { .. } => c.unchanged += 1,
            CaseAction::Delete { .. } => c.deleted += 1,
        }
    }
    c
}

pub fn cases_write_anything(actions: &[CaseAction]) -> bool {
    actions
        .iter()
        .any(|a| !matches!(a, CaseAction::Unchanged { .. }))
}

// ─────────────────────────── the reconcile ───────────────────────────

/// What the nightly full pass found about case rows, both directions, by name.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CaseDrift {
    /// Rows naming a file the tree does not have (distinct files).
    pub rows_without_file: Vec<String>,
    /// Test files on disk that name cases and have no row at all.
    pub files_without_rows: Vec<String>,
    /// Cases in a file with no row, as `file :: case`.
    pub cases_without_rows: Vec<String>,
    /// Rows for a case the file no longer contains, as `file :: case`.
    pub rows_without_case: Vec<String>,
}

impl CaseDrift {
    pub fn is_clean(&self) -> bool {
        self.rows_without_file.is_empty()
            && self.files_without_rows.is_empty()
            && self.cases_without_rows.is_empty()
            && self.rows_without_case.is_empty()
    }
    pub fn report(&self) -> String {
        if self.is_clean() {
            return "reconcile cases: clean — every case in the tree has its row and every row has its case".to_string();
        }
        let mut parts = Vec::new();
        if !self.rows_without_file.is_empty() {
            parts.push(format!(
                "{} file(s) with rows but no file on disk: {}",
                self.rows_without_file.len(),
                self.rows_without_file.join(", ")
            ));
        }
        if !self.files_without_rows.is_empty() {
            parts.push(format!(
                "{} test file(s) with no row: {}",
                self.files_without_rows.len(),
                self.files_without_rows.join(", ")
            ));
        }
        if !self.cases_without_rows.is_empty() {
            parts.push(format!(
                "{} case(s) with no row: {}",
                self.cases_without_rows.len(),
                self.cases_without_rows.join(", ")
            ));
        }
        if !self.rows_without_case.is_empty() {
            parts.push(format!(
                "{} row(s) whose case is gone from its file: {}",
                self.rows_without_case.len(),
                self.rows_without_case.join(", ")
            ));
        }
        format!("reconcile cases: DRIFT — {}", parts.join(" · "))
    }
}

/// `desired` = every case parsed from every test file on disk (a FULL read);
/// `test_files` = every kind=test file on disk, including no-case ones.
pub fn reconcile_cases(
    desired: &[CaseRow],
    test_files: &[String],
    graph: &[CaseInGraph],
) -> CaseDrift {
    let mut d = CaseDrift::default();
    for g in graph {
        if !test_files.contains(&g.file) {
            if !d.rows_without_file.contains(&g.file) {
                d.rows_without_file.push(g.file.clone());
            }
        } else if !desired.iter().any(|r| r.file == g.file && r.case == g.case) {
            d.rows_without_case
                .push(format!("{} :: {}", g.file, g.case));
        }
    }
    for f in test_files {
        let wants: Vec<&CaseRow> = desired.iter().filter(|r| r.file == *f).collect();
        if wants.is_empty() {
            continue; // a no-case file has no row BY DESIGN (#4106); it is reported, not drift
        }
        let has_any = graph.iter().any(|g| g.file == *f);
        if !has_any {
            d.files_without_rows.push(f.clone());
            continue;
        }
        for r in wants {
            if !graph.iter().any(|g| g.file == r.file && g.case == r.case) {
                d.cases_without_rows
                    .push(format!("{} :: {}", r.file, r.case));
            }
        }
    }
    d.rows_without_file.sort();
    d.files_without_rows.sort();
    d.cases_without_rows.sort();
    d.rows_without_case.sort();
    d
}

/// A `{"domain": count}` object, for the --check-shares seam and its tests.
pub fn parse_count_object(json: &str) -> Result<Vec<(String, usize)>, String> {
    let t = json.trim();
    let t = t
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or("expected a JSON object")?;
    let mut out = Vec::new();
    for part in t.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (k, v) = part
            .split_once(':')
            .ok_or_else(|| format!("bad entry: {part}"))?;
        let k = k.trim().trim_matches('"').to_string();
        let v: usize = v
            .trim()
            .parse()
            .map_err(|_| format!("count is not a number: {part}"))?;
        out.push((k, v));
    }
    Ok(out)
}

/// #4310 — a Test row's results go with it. The case Delete removed the Test
/// and left every TestResult whose required `ofTest` named it: 746 results
/// pointed at 103 missing tests on 2026-09-25, all of them this crawler's
/// deletes. The query finds a case's results by that edge; the caller deletes
/// them through the same door before it deletes the case.
pub fn results_of_case_query(case_name: &str) -> String {
    let safe: String = case_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect();
    format!(
        "PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?r WHERE {{ GRAPH <urn:chorus:domains:tests> {{ ?r chorus:ofTest <https://jeffbridwell.com/chorus#{safe}> }} }}"
    )
}

/// Result row names out of the query's CSV answer (header first). A result's
/// IRI is `…#test-result-<name>`; the door addresses it by `<name>`.
pub fn result_names_from_csv(csv: &str) -> Vec<String> {
    csv.lines()
        .skip(1)
        .filter_map(|l| {
            let iri = l.trim().trim_matches('"');
            iri.rsplit_once("#test-result-").map(|(_, n)| n.to_string())
        })
        .filter(|n| !n.is_empty())
        .collect()
}

#[cfg(test)]
mod cases_4185 {
    // #4310 — a deleted case must take its results with it.
    #[test]
    fn a_case_query_asks_for_results_by_their_oftest_edge() {
        let q = results_of_case_query("test-a-bats-one-1a2b");
        assert!(q.contains("chorus:ofTest <https://jeffbridwell.com/chorus#test-a-bats-one-1a2b>"));
        assert!(q.contains("GRAPH <urn:chorus:domains:tests>"));
    }

    #[test]
    fn a_case_name_cannot_break_out_of_the_query() {
        let q = results_of_case_query("x> } DROP ALL { <y");
        assert!(!q.contains('}') || q.matches('}').count() == 2);
        assert!(!q.contains("DROP ALL"));
    }

    #[test]
    fn result_names_come_from_the_iri_the_store_answers_with() {
        let csv = "r\nhttps://jeffbridwell.com/chorus#test-result-testresult-1-2\nhttps://jeffbridwell.com/chorus#test-result-testresult-3-4\n";
        assert_eq!(result_names_from_csv(csv), vec!["testresult-1-2", "testresult-3-4"]);
    }

    #[test]
    fn an_empty_answer_means_no_results_to_delete() {
        assert!(result_names_from_csv("r\n").is_empty());
    }

    use super::*;

    // ── the authored header (#3924, ported from 3924-declared-wins.bats) ──
    #[test]
    fn declared_layer_wins_even_when_content_screams_integration() {
        let c =
            "// @test-type: unit — hermetic despite the curl below\ncurl http://localhost:3030/x";
        assert_eq!(declared(c), Some(("unit", None)));
        assert_eq!(file_class("x.bats", c).layer, "unit");
        assert!(file_class("x.bats", c).declared);
    }
    #[test]
    fn declared_concern_wins_over_the_heuristic_concern() {
        assert_eq!(
            declared(
                "// @test-type: unit:api\n#  gitleaks mention would heuristically say security"
            ),
            Some(("unit", Some("api")))
        );
    }
    // NEGATIVE PROOF (#3734): prose that MENTIONS the header does not declare one.
    #[test]
    fn negative_proof_a_prose_mention_is_not_a_declaration() {
        assert_eq!(
            declared(
                "// the @test-type: header is required by the gate\n// @test-type: headers matter"
            ),
            None
        );
        assert_eq!(
            declared("// @test-type: banana"),
            None,
            "a junk layer is refused, never a fabricated row"
        );
        assert_eq!(declared("plain file, no header"), None);
    }
    #[test]
    fn justified_form_and_bare_concern_parse() {
        assert_eq!(
            declared("# @test-type: e2e:ui — playwright flow"),
            Some(("e2e", Some("ui")))
        );
        assert_eq!(
            declared("# @test-type: perf\n"),
            Some(("fitness", Some("perf"))),
            "#4136 — a bare concern is fitness:<concern>"
        );
        assert_eq!(
            declared("# @test-type: integration:api\n"),
            Some(("integration", Some("api")))
        );
    }

    // ── case names (#4022 #4106 #4111 #4135) ──
    #[test]
    fn a_jest_name_containing_quotes_is_registered_whole() {
        let src = "describe('#3696 relay framing (pure)', () => {\n  it('eventFrame is NIP-01 [\"EVENT\", event]', () => {});\n  it(`has zero rows`, () => {});\n  it(\"says 'hi' to it\", () => {});\n  test.only('plain name', () => {});\n});\n";
        assert_eq!(
            case_names("relay.test.ts", src),
            vec![
                "eventFrame is NIP-01 [\"EVENT\", event]",
                "has zero rows",
                "says 'hi' to it",
                "plain name"
            ]
        );
    }
    // NEGATIVE PROOF: the three ways the registry minted names nothing could run.
    #[test]
    fn negative_proof_a_regex_dot_test_call_is_not_a_declaration() {
        let src = "describe('vocab', () => {\n  test('no page ships the words Log in', () => {\n    expect(/Log in/.test('<button>Log in</button>')).toBe(true);\n    expect(/Log in/.test('handleAuthLogin login')).toBe(false);\n  });\n});\n";
        assert_eq!(
            case_names("dotcall.test.ts", src),
            vec!["no page ships the words Log in"]
        );
    }
    #[test]
    fn negative_proof_a_template_literal_name_is_not_registered() {
        let src = "it(`Clearing is running on ephemeral port ${TEST_PORT}`, () => {});\nit('a plain name that does match', () => {});\n";
        assert_eq!(
            case_names("tmpl.test.ts", src),
            vec!["a plain name that does match"]
        );
    }
    /// #4162 — one field; concern ui/perf/security wins, api falls through,
    /// nothing declared is `unclassified`, never empty.
    #[test]
    fn test_type_is_one_answer_and_never_empty() {
        assert_eq!(test_type("unit", Some("security")), "security");
        assert_eq!(test_type("integration", Some("ui")), "ui");
        assert_eq!(test_type("integration", Some("perf")), "perf");
        assert_eq!(test_type("integration", Some("api")), "integration");
        assert_eq!(test_type("bdd", None), "bdd");
        assert_eq!(test_type("", None), "unclassified");
        assert_eq!(test_type("  ", Some("api")), "unclassified");
    }

    /// #4162 NEGATIVE PROOF: a served row still carrying pyramidLayer or
    /// testConcern does not match, so the crawler rewrites it; the owned
    /// fields never carry either retired name.
    #[test]
    fn a_row_with_a_retired_field_is_rewritten_and_never_written() {
        let row = CaseRow {
            file: "a.bats".into(), case: "c".into(), covers: "tests".into(),
            layer: "unit".into(), hermeticity: "hermetic".into(), concern: Some("security".into()),
            in_file: "file-a".into(),
        };
        let owned = row.owned_fields();
        assert!(owned.iter().any(|(k, v)| k == "testType" && v == "security"));
        assert!(!owned.iter().any(|(k, _)| RETIRED_TEST_FIELDS.contains(&k.as_str())));
        let g = |extra: Vec<(&str, &str)>| {
            let mut f: Vec<(String, String)> = vec![("inFile".into(), "file-a".into()), ("covers".into(), "tests".into()),
                ("testType".into(), "security".into()), ("hermeticity".into(), "hermetic".into())];
            f.extend(extra.into_iter().map(|(k, v)| (k.to_string(), v.to_string())));
            CaseInGraph { name: "t".into(), file: "a.bats".into(), case: "c".into(), fields: f }
        };
        assert!(row_matches(&row, &g(vec![])), "control: the migrated row matches");
        assert!(!row_matches(&row, &g(vec![("pyramidLayer", "unit")])));
        assert!(!row_matches(&row, &g(vec![("testConcern", "security")])));
    }

    /// #4292 — the scenarios cucumber's default profile runs, by name.
    #[test]
    fn feature_scenarios_are_registered_by_name_and_switched_off_ones_are_not() {
        let src = "@seed\nFeature: seeds\n  Scenario: a seed lands\n    Given x\n\n  @wip @gap-1\n  Scenario: not yet\n    Given y\n\n  Scenario Outline: each kind <k>\n    Given <k>\n    Examples:\n      | k |\n      | a |\n";
        assert_eq!(
            case_names("platform/tests/features/seeds/x.feature", src),
            vec!["a seed lands".to_string(), "each kind <k>".to_string()]
        );
        // NEGATIVE PROOF: a feature switched off at feature level mints nothing
        let off = "@memory @e2e\nFeature: recall\n  Scenario: remembers\n    Given z\n";
        assert!(case_names("platform/tests/features/memory/y.feature", off).is_empty());
        assert_eq!(no_case_bucket("platform/tests/features/memory/y.feature", off), "ignored-only");
        // NEGATIVE PROOF: a feature outside the lane's directory mints nothing
        assert!(case_names("designing/docs/z.feature", "Feature: f\n  Scenario: s\n").is_empty());
        assert_eq!(no_case_bucket("designing/docs/z.feature", "Feature: f"), "no-lane");
    }

    /// #4292 — a declared header that says needs-stack is believed; without it
    /// a library-call live test reads hermetic (no exec signal).
    #[test]
    fn a_header_that_says_needs_stack_is_believed() {
        let live = "// @test-type: integration — needs-stack: reads Fuseki\n#[test]\nfn t() {}\n";
        assert_eq!(file_class("x/tests/live.rs", live).hermeticity, "needs-stack");
        let plain = "// @test-type: integration — reads a tmpdir\n#[test]\nfn t() {}\n";
        assert_eq!(file_class("x/tests/plain.rs", plain).hermeticity, "hermetic");
    }

    /// #4292 — unittest cases as Class.method; pytest style and helpers mint nothing.
    #[test]
    fn unittest_classes_register_class_dot_method() {
        let src = "import unittest\n\nclass PathTests(unittest.TestCase):\n    def test_a(self):\n        pass\n    def helper(self):\n        pass\n\ndef test_top():\n    pass\n\nclass Other(TestCase):\n    def test_b(self): pass\n";
        assert_eq!(
            case_names("platform/tests/test_x.py", src),
            vec!["PathTests.test_a".to_string(), "Other.test_b".to_string()]
        );
        // NEGATIVE PROOF (#4106): a pytest-style file no lane runs mints nothing
        assert!(case_names("platform/tests/test_y.py", "def test_top():\n    assert 1\n").is_empty());
    }

    #[test]
    fn negative_proof_a_kind_with_no_extractor_and_no_lane_mints_nothing() {
        assert!(case_names("helper.py", "def helper():\n    return 1\n").is_empty());
        // a playwright spec IS registered — at file grain, the ui lane's identity (#4045)
        assert_eq!(
            case_names("proving/flows/flow.spec.cjs", "test('x', () => {});"),
            vec!["flow.spec.cjs"]
        );
        assert!(
            !case_names("proving/flows/flow.spec.cjs", "test('x', () => {});")
                .contains(&"x".to_string()),
            "never the inner title — the lane does not emit it"
        );
    }
    #[test]
    fn a_shell_suite_is_one_case_named_by_its_file() {
        assert_eq!(
            case_names(
                "platform/tests/daemon-env.test.sh",
                "#!/bin/sh\necho checking\nexit 0\n"
            ),
            vec!["daemon-env.test.sh"]
        );
    }
    #[test]
    fn a_bats_name_keeps_everything_after_an_escaped_quote_and_unescapes_dollar() {
        let src = "@test \"lock: no direct Command::new(\\\"osascript\\\") — route via inject\" {\n  true\n}\n";
        assert_eq!(
            case_names("locks.bats", src),
            vec!["lock: no direct Command::new(\"osascript\") — route via inject"]
        );
        assert_eq!(
            case_names(
                "iso.bats",
                "@test \"NEGATIVE PROOF: the \\$\\$ name differs\" {\n  true\n}\n"
            ),
            vec!["NEGATIVE PROOF: the $$ name differs"]
        );
        assert_eq!(
            case_names(
                "g.bats",
                "@test \"no file hardcodes /Users/<name>/ (use \\$CHORUS_ROOT)\" {\n  true\n}\n"
            ),
            vec!["no file hardcodes /Users/<name>/ (use $CHORUS_ROOT)"]
        );
    }
    #[test]
    fn a_jest_name_is_the_strings_value_not_its_source() {
        assert_eq!(
            case_names(
                "esc.test.ts",
                "it('escapes newlines to literal \\\\n', () => {});\n"
            ),
            vec!["escapes newlines to literal \\n"]
        );
        assert_eq!(
            case_names(
                "q.test.ts",
                "it(\"a name with 'inner' quotes\", () => {});\n"
            ),
            vec!["a name with 'inner' quotes"]
        );
    }
    // NEGATIVE PROOF: an @test written INSIDE a string fixture is not a declaration.
    #[test]
    fn negative_proof_an_at_test_inside_a_string_fixture_is_not_a_case() {
        let src = "@test \"the real case\" {\n  printf '@test \"a fixture case\" {\\n  true\\n}\\n' > \"$BATS_TEST_TMPDIR/x.bats\"\n}\n";
        assert_eq!(
            case_names("fixture-builder.bats", src),
            vec!["the real case"]
        );
    }
    #[test]
    fn a_rust_test_fn_is_registered_and_an_ignored_one_is_not() {
        assert_eq!(
            case_names("units.rs", "#[test]\nfn walks_the_ledger() { }\n"),
            vec!["walks_the_ledger"]
        );
        assert_eq!(case_names("c.rs", "#[test]\nfn keeps() {}\n#[test]\n#[ignore]\nfn skipped() {}\n#[tokio::test]\nasync fn async_one() {}\n"), vec!["keeps", "async_one"]);
    }
    // #4199 — an alias of `test` declares cases; NEGATIVE PROOF: a const that
    // does not bind test/it is not an alias, and a plain call to it mints nothing.
    #[test]
    fn a_const_alias_of_test_declares_cases_and_an_unrelated_const_does_not() {
        let src = "const storeWritable = process.env.X === '1';\nconst testWhenWritable = storeWritable ? test : test.skip;\nconst helper = (x) => x;\ntestWhenWritable('POST /api/x returns count > 0', async () => {});\nhelper('not a case');\n";
        assert_eq!(test_aliases(src), vec!["testWhenWritable".to_string()]);
        assert_eq!(
            case_names("a.integration.test.ts", src),
            vec!["POST /api/x returns count > 0"]
        );
    }

    #[test]
    fn no_case_buckets_say_why_a_test_file_has_no_case() {
        assert_eq!(
            no_case_bucket("t/live.rs", "#[test]\n#[ignore]\nfn x() {}"),
            "ignored-only"
        );
        assert_eq!(
            no_case_bucket("src/mod.rs", "#[cfg(test)]\nmod a_test;"),
            "no-tests"
        );
        assert_eq!(
            no_case_bucket("tests/test_x.py", "def test_a(): pass"),
            "no-lane"
        );
        assert_eq!(no_case_bucket("f.feature", "Feature: x"), "no-lane");
        assert_eq!(no_case_bucket("a.spec.ts", "weird()"), "unextracted");
        let (missing, summary) =
            no_case_summary(&["unextracted", "no-lane", "no-lane", "ignored-only"]);
        assert_eq!(missing, 1);
        assert_eq!(summary, "unextracted 1 · ignored-only 1 · no-lane 2");
    }

    #[test]
    fn no_case_report_names_the_kinds() {
        let r = no_case_report(&["a/x.sh".into(), "b/y.sh".into(), "c/z.feature".into()]);
        assert!(
            r.contains("3 registered file(s) yield no runnable case"),
            "{r}"
        );
        assert!(r.contains("sh 2, feature 1"), "{r}");
        assert!(no_case_report(&[]).contains("none"));
    }

    // ── covers (#4201: from the file, never the folder) ──
    #[test]
    fn a_security_concern_covers_the_security_domain_so_the_lane_keeps_its_rows() {
        assert_eq!(covers_from(Some("messages"), Some("security")), "security");
        assert_eq!(covers_from(Some("messages"), None), "messages");
        assert_eq!(covers_from(Some("messages"), Some("api")), "messages");
    }
    #[test]
    fn negative_proof_an_unplaced_file_covers_the_tests_domain_never_a_folder() {
        assert_eq!(covers_from(None, None), "tests");
        assert_ne!(covers_from(None, None), "services");
        let row = CaseRow {
            file: "platform/tests/4099-x.bats".into(),
            case: "a".into(),
            covers: covers_from(None, None),
            layer: "integration".into(),
            hermeticity: "hermetic".into(),
            concern: None,
            in_file: "file-x".into(),
        };
        assert!(row.owned_fields().contains(&("covers".to_string(), "tests".to_string())));
    }

    // ── the share gate ──
    #[test]
    fn share_gate_stands_down_below_the_floor_and_fires_above_it() {
        assert!(check_shares(&[("services".into(), 1)], 0.30, 20).is_ok());
        let e = check_shares(&[("services".into(), 40), ("x".into(), 5)], 0.30, 20).unwrap_err();
        assert!(e.contains("covers-share gate RED"), "{e}");
    }
    // NEGATIVE PROOF: an over-share corpus REFUSES; a healthy one passes; the cap is config.
    #[test]
    fn negative_proof_an_over_share_corpus_refuses_and_a_healthy_one_passes() {
        let e = check_shares(
            &[
                ("services".into(), 500),
                ("cards".into(), 100),
                ("messages".into(), 100),
            ],
            0.30,
            20,
        )
        .unwrap_err();
        assert!(e.contains("services holds 500/700"), "{e}");
        let ok = [
            ("services".into(), 200),
            ("cards".into(), 180),
            ("messages".into(), 170),
            ("builds".into(), 150),
            ("cicd".into(), 140),
        ];
        assert!(check_shares(&ok, 0.30, 20).is_ok());
        assert!(
            check_shares(&ok, 0.10, 20).is_err(),
            "a tighter cap flips the same fixture red"
        );
    }

    // ── the heuristic ──
    #[test]
    fn a_live_curl_reads_as_integration_needs_stack_and_a_plain_file_as_unit() {
        assert_eq!(
            classify_case("x.bats", "curl -s http://localhost:3340/x"),
            ("integration", "needs-stack", None)
        );
        assert_eq!(
            classify_case("x.bats", "curl -s -X POST \"http://127.0.0.1:3030/ds\""),
            ("integration", "needs-stack", None)
        );
        assert_eq!(
            classify_case("platform/x/tests/y.rs", "Command::new(\"launchctl\")"),
            ("integration", "needs-stack", None)
        );
        assert_eq!(
            classify_case("platform/x/src/y.rs", "Command::new(\"launchctl\")"),
            ("unit", "hermetic", None),
            "an in-crate .rs is never integration by content"
        );
        assert_eq!(
            classify_case("x.bats", "@test \"a\" { [ 1 = 1 ]; }"),
            ("unit", "hermetic", None)
        );
        assert_eq!(
            classify_case("gitleaks-guard.bats", "run gitleaks detect"),
            ("integration", "needs-stack", Some("security"))
        );
        assert_eq!(
            classify_case("flows/x.feature.test.ts", "cucumber"),
            ("bdd", "hermetic", None)
        );
        assert_eq!(
            classify_case("x.test.sh", "env-up then teardown"),
            ("e2e", "needs-stack", None)
        );
    }

    // ── the plan ──
    fn row(file: &str, case: &str) -> CaseRow {
        CaseRow {
            file: file.into(),
            case: case.into(),
            covers: "services".into(),
            layer: "unit".into(),
            hermeticity: "hermetic".into(),
            concern: None,
            in_file: format!("file-{file}"),
        }
    }
    fn g(name: &str, file: &str, case: &str) -> CaseInGraph {
        let r = row(file, case);
        CaseInGraph {
            name: name.into(),
            file: file.into(),
            case: case.into(),
            fields: r.owned_fields(),
        }
    }

    /// #4292 NEGATIVE PROOF: a scenario added to a run feature with no row yet
    /// is named by the reconcile, so crawler-validate's tests row goes red and
    /// says which scenario.
    #[test]
    fn a_new_scenario_with_no_row_is_named_by_the_reconcile() {
        let f = "platform/tests/features/seeds/seed-media.feature";
        let src = "Feature: seeds\n  Scenario: old one\n  Scenario: brand new one\n";
        let desired: Vec<CaseRow> = case_names(f, src).iter().map(|c| row(f, c)).collect();
        let d = reconcile_cases(&desired, &[f.to_string()], &[g("t-old", f, "old one")]);
        assert_eq!(d.cases_without_rows, vec![format!("{f} :: brand new one")]);
        assert!(!d.is_clean());
        // control: once the row exists the file is clean
        let d = reconcile_cases(&desired, &[f.to_string()], &[g("t-old", f, "old one"), g("t-new", f, "brand new one")]);
        assert!(d.is_clean(), "{d:?}");
    }
    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn a_new_case_posts_a_changed_one_replaces_an_identical_one_is_left_alone() {
        let desired = [
            row("a.bats", "one"),
            row("a.bats", "two"),
            CaseRow {
                layer: "integration".into(),
                ..row("b.rs", "x")
            },
        ];
        let graph = [g("n1", "a.bats", "one"), g("n3", "b.rs", "x")];
        let acts = plan_cases(
            &desired,
            &s(&["a.bats", "b.rs"]),
            &[],
            &graph,
            TreeRead::Complete,
        );
        let c = case_counts(&acts);
        assert_eq!(
            (c.posted, c.replaced, c.unchanged, c.deleted),
            (1, 1, 1, 0),
            "{acts:?}"
        );
        assert!(acts.contains(&CaseAction::Replace {
            name: "n3".into(),
            row: desired[2].clone()
        }));
    }
    // AC: idempotent — a second run writes NOTHING, proven by planning against the result.
    #[test]
    fn a_second_run_over_an_unchanged_registry_writes_nothing() {
        let desired = [row("a.bats", "one"), row("b.rs", "x")];
        let graph = [g("n1", "a.bats", "one"), g("n2", "b.rs", "x")];
        let acts = plan_cases(
            &desired,
            &s(&["a.bats", "b.rs"]),
            &[],
            &graph,
            TreeRead::Complete,
        );
        assert!(!cases_write_anything(&acts), "{acts:?}");
    }
    // NEGATIVE PROOF (#3734): the same check fires when there IS work.
    #[test]
    fn negative_proof_a_run_with_real_work_reports_that_it_writes() {
        let acts = plan_cases(
            &[row("a.bats", "one")],
            &s(&["a.bats"]),
            &[],
            &[],
            TreeRead::Complete,
        );
        assert!(cases_write_anything(&acts));
    }
    #[test]
    fn a_case_that_left_its_file_is_deleted_by_name_when_the_file_was_parsed() {
        let graph = [g("keep", "a.bats", "one"), g("gone", "a.bats", "two")];
        let acts = plan_cases(
            &[row("a.bats", "one")],
            &s(&["a.bats"]),
            &[],
            &graph,
            TreeRead::Scoped,
        );
        assert_eq!(case_counts(&acts).deleted, 1);
        assert!(acts.contains(&CaseAction::Delete {
            name: "gone".into(),
            file: "a.bats".into(),
            case: "two".into()
        }));
    }
    // NEGATIVE PROOF: a delta never deletes rows of files it did not look at.
    #[test]
    fn negative_proof_a_scoped_delta_never_deletes_what_it_did_not_parse() {
        let graph = [
            g("n1", "changed.bats", "one"),
            g("n2", "untouched.rs", "x"),
            g("n3", "untouched.rs", "y"),
        ];
        let scoped = plan_cases(
            &[row("changed.bats", "one")],
            &s(&["changed.bats"]),
            &[],
            &graph,
            TreeRead::Scoped,
        );
        assert_eq!(case_counts(&scoped).deleted, 0, "{scoped:?}");
        // control: a COMPLETE walk with the same inputs treats them as orphans
        let full = plan_cases(
            &[row("changed.bats", "one")],
            &s(&["changed.bats"]),
            &[],
            &graph,
            TreeRead::Complete,
        );
        assert_eq!(case_counts(&full).deleted, 2);
    }
    #[test]
    fn rows_of_a_file_git_removed_are_deleted_on_a_delta() {
        let graph = [g("n1", "gone.bats", "one"), g("n2", "stays.bats", "x")];
        let acts = plan_cases(&[], &[], &s(&["gone.bats"]), &graph, TreeRead::Scoped);
        assert_eq!(case_counts(&acts).deleted, 1);
        assert!(acts.contains(&CaseAction::Delete {
            name: "n1".into(),
            file: "gone.bats".into(),
            case: "one".into()
        }));
    }
    // NEGATIVE PROOF (#4022): when the disk check cannot run, REFUSE to delete.
    #[test]
    fn negative_proof_a_partial_tree_read_refuses_every_delete() {
        let graph = [
            g("n1", "a.bats", "one"),
            g("gone", "a.bats", "two"),
            g("orphan", "ghost.rs", "z"),
        ];
        let complete = plan_cases(
            &[row("a.bats", "one")],
            &s(&["a.bats"]),
            &s(&["ghost.rs"]),
            &graph,
            TreeRead::Complete,
        );
        assert_eq!(
            case_counts(&complete).deleted,
            2,
            "control: a complete read DOES delete"
        );
        let partial = plan_cases(
            &[row("a.bats", "one")],
            &s(&["a.bats"]),
            &s(&["ghost.rs"]),
            &graph,
            TreeRead::Partial,
        );
        assert_eq!(case_counts(&partial).deleted, 0, "{partial:?}");
    }
    #[test]
    fn two_identical_case_names_in_one_file_are_one_row_and_a_legacy_duplicate_is_removed() {
        let desired = [row("a.bats", "dup"), row("a.bats", "dup")];
        let acts = plan_cases(&desired, &s(&["a.bats"]), &[], &[], TreeRead::Complete);
        assert_eq!(case_counts(&acts).posted, 1);
        let graph = [g("old-a", "a.bats", "dup"), g("old-b", "a.bats", "dup")];
        let acts = plan_cases(&desired, &s(&["a.bats"]), &[], &graph, TreeRead::Complete);
        let c = case_counts(&acts);
        assert_eq!((c.unchanged, c.deleted), (1, 1), "{acts:?}");
    }

    // The door serves an edge WITH its kind prefix and accepts it bare. A row
    // must read unchanged across that asymmetry, or no run is ever idempotent.
    #[test]
    fn a_served_edge_with_the_kind_prefix_matches_the_bare_value_written() {
        let r = row("a.bats", "one");
        let mut g1 = g("n", "a.bats", "one");
        for (k, v) in g1.fields.iter_mut() {
            if k == "inFile" {
                *v = format!("code-file-{v}");
            }
            if k == "covers" {
                *v = format!("domain-{v}");
            }
        }
        assert!(row_matches(&r, &g1), "{:?}", g1.fields);
    }
    // NEGATIVE PROOF (#3734): tolerance of the prefix must not swallow a real change.
    #[test]
    fn negative_proof_a_different_in_file_or_covers_still_reads_as_changed() {
        let r = row("a.bats", "one");
        let mut g1 = g("n", "a.bats", "one");
        for (k, v) in g1.fields.iter_mut() {
            if k == "inFile" {
                *v = "code-file-file-other-file-deadbeef".to_string();
            }
        }
        assert!(!row_matches(&r, &g1));
        let mut g2 = g("n", "a.bats", "one");
        for (k, v) in g2.fields.iter_mut() {
            if k == "covers" {
                *v = "security".to_string();
            }
        }
        assert!(!row_matches(&r, &g2));
        // a value that merely ENDS with ours, without the dash, is a different row
        let mut g3 = g("n", "a.bats", "one");
        for (k, v) in g3.fields.iter_mut() {
            if k == "inFile" {
                *v = format!("x{v}");
            }
        }
        assert!(!row_matches(&r, &g3));
    }

    // ── the reconcile ──
    #[test]
    fn a_registry_that_matches_the_tree_reconciles_clean() {
        let d = reconcile_cases(
            &[row("a.bats", "one")],
            &s(&["a.bats", "nocase.py"]),
            &[g("n", "a.bats", "one")],
        );
        assert!(d.is_clean(), "{}", d.report());
    }
    // NEGATIVE PROOF (#3734): one stale row makes the reconcile RED and names the file.
    #[test]
    fn negative_proof_one_stale_row_makes_the_reconcile_red_and_names_it() {
        let d = reconcile_cases(
            &[row("a.bats", "one")],
            &s(&["a.bats"]),
            &[g("n", "a.bats", "one"), g("ghost", "deleted.bats", "z")],
        );
        assert!(!d.is_clean());
        assert!(d.report().contains("deleted.bats"), "{}", d.report());
        let d2 = reconcile_cases(
            &[row("a.bats", "one")],
            &s(&["a.bats"]),
            &[g("n", "a.bats", "one"), g("stale", "a.bats", "gone-case")],
        );
        assert!(
            d2.report().contains("a.bats :: gone-case"),
            "{}",
            d2.report()
        );
    }
    #[test]
    fn a_test_file_with_cases_and_no_rows_is_drift_and_a_no_case_file_is_not() {
        let d = reconcile_cases(&[row("a.bats", "one")], &s(&["a.bats", "helper.py"]), &[]);
        assert_eq!(d.files_without_rows, vec!["a.bats".to_string()]);
        assert!(
            d.report().contains("1 test file(s) with no row: a.bats"),
            "{}",
            d.report()
        );
    }

    // ── names ──
    #[test]
    fn case_row_names_are_stable_distinct_and_within_the_doors_128_bytes() {
        assert_eq!(case_row_name("a.bats", "x"), case_row_name("a.bats", "x"));
        assert_ne!(
            case_row_name("a.bats", "Says Hi"),
            case_row_name("a.bats", "says hi"),
            "case differs → different row"
        );
        let long = "x".repeat(400);
        let n = case_row_name("platform/tests/some/very/long/path/to/a/suite.bats", &long);
        assert!(n.len() <= 128, "{}", n.len());
        assert!(n.starts_with("test-platform-tests-some"));
    }
    #[test]
    fn a_count_object_parses() {
        assert_eq!(
            parse_count_object("{\"services\": 40, \"x\": 5}").unwrap(),
            vec![("services".to_string(), 40), ("x".to_string(), 5)]
        );
        assert!(parse_count_object("[1]").is_err());
    }
}
