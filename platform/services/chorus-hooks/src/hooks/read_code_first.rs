// @domain: policies — the read-the-code-first rule's enforcement (#5027)
//! Read the code first (#5027).
//!
//! Jeff, 2026-11-21 17:37: "its unacceptable for all of u to not read the code … a stop
//! hook as the very first step of werk-code". Borg Queen measured 351 sessions (11-22):
//! 22% of first edits to a file came after reading the whole file; the rest followed a
//! partial read, a shell peek, or nothing. cclsp was called 0 times in 30 days.
//!
//! Two rules, both at PreToolUse on an Edit or Write to an existing file in a werk:
//! 1. The session has read the whole file: the line ranges its Read calls returned,
//!    together, cover 1..N of the file as it is now. A shell read (cat, sed) is not a
//!    Read and does not count. A file the session wrote itself counts as read.
//! 2. An edit that changes a definition line (a function's name or parameters, a type's
//!    name or header) needs a lookup of that symbol first: any cclsp or ast-grep call
//!    (MCP, or `ast-grep` on the shell) whose input names it. Body edits are not checked.
//!
//! The rule's words come from the policy row `read-the-code-first` (the policies
//! domain), never a copy here. A row that cannot be read refuses the edit and says so.
//!
//! Reads and lookups are recorded live by the daemon (PostToolUse Read, PreToolUse
//! lookup), as tdd_gate's test edits are (#3278): the transcript flushes a turn late. A
//! daemon restart forgets them; the cost is one re-read, named in the refusal.

use crate::types::{permission_deny_json, HookInput, HookResponse};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const POLICY: &str = "read-the-code-first";
/// agy's view_file shows at most this many lines when no range is asked for (BQ, 11-22).
const AGY_VIEW_DEFAULT: usize = 800;
/// Claude Code's Read default when neither the result nor a limit says otherwise.
const CLAUDE_READ_DEFAULT: usize = 2000;
const POLICY_TTL: Duration = Duration::from_secs(60);
const MAX_LOOKUPS: usize = 500;
const MAX_LOOKUP_CHARS: usize = 2000;

/// The policy row's words, as the door serves them.
#[derive(Clone, Debug, PartialEq)]
pub struct Policy {
    pub label: String,
    pub comment: String,
}

/// (session, file key) -> the line ranges its reads returned.
type Reads = HashMap<(String, String), Vec<(usize, usize)>>;

/// What each session has read, written and looked up. One per daemon (AppState).
#[derive(Default)]
pub struct Ledger {
    reads: Mutex<Reads>,
    authored: Mutex<HashSet<(String, String)>>,
    lookups: Mutex<HashMap<String, Vec<String>>>,
}

impl Ledger {
    pub fn record_read(&self, sid: &str, path: &str, first: usize, last: usize) {
        if sid.is_empty() || last < first { return; }
        if let Ok(mut m) = self.reads.lock() {
            // kept merged, so a long session holds a few ranges per file, not every read
            let r = m.entry((sid.to_string(), key(path))).or_default();
            r.push((first.max(1), last));
            *r = merged(r);
        }
    }
    pub fn record_authored(&self, sid: &str, path: &str) {
        if sid.is_empty() { return; }
        if let Ok(mut s) = self.authored.lock() { s.insert((sid.to_string(), key(path))); }
    }
    pub fn record_lookup(&self, sid: &str, text: &str) {
        if sid.is_empty() || text.is_empty() { return; }
        // bounded: the daemon lives for days; the newest lookups are the ones an edit needs
        let text: String = text.chars().take(MAX_LOOKUP_CHARS).collect();
        if let Ok(mut m) = self.lookups.lock() {
            let v = m.entry(sid.to_string()).or_default();
            v.push(text);
            if v.len() > MAX_LOOKUPS { v.remove(0); }
        }
    }
    fn ranges(&self, sid: &str, path: &str) -> Vec<(usize, usize)> {
        self.reads.lock().ok().and_then(|m| m.get(&(sid.to_string(), key(path))).cloned()).unwrap_or_default()
    }
    fn wrote(&self, sid: &str, path: &str) -> bool {
        self.authored.lock().map(|s| s.contains(&(sid.to_string(), key(path)))).unwrap_or(false)
    }
    fn looked_up(&self, sid: &str, symbol: &str) -> bool {
        let Ok(re) = regex::Regex::new(&format!(r"\b{}\b", regex::escape(symbol))) else { return false };
        self.lookups.lock().map(|m| m.get(sid).is_some_and(|v| v.iter().any(|t| re.is_match(t)))).unwrap_or(false)
    }
}

/// One file, wherever it is read from: a werk path and its canonical twin share a key.
fn key(path: &str) -> String {
    if let Some((_, rest)) = path.split_once("/chorus-werk/") {
        return rest.split_once('/').map(|(_, rel)| rel.to_string()).unwrap_or_default();
    }
    let root = format!("{}/", crate::shared::state_paths::chorus_root());
    path.strip_prefix(&root).unwrap_or(path).to_string()
}

/// A file in a card's werk: `…/chorus-werk/<role>-<card>/<path>`. Canonical, /tmp and
/// everything else are not werk-code.
fn in_werk(path: &str) -> bool {
    path.split_once("/chorus-werk/").is_some_and(|(_, rest)| rest.split_once('/').is_some_and(|(slot, rel)| !slot.is_empty() && !rel.is_empty()))
}

/// The lines a Read returned: the result's own count first (a token-capped read says
/// how far it got), then the range asked for (agy: StartLine/EndLine; Claude: offset/limit).
pub fn read_range(input: &serde_json::Value, response: Option<&serde_json::Value>) -> Option<(usize, usize)> {
    let num = |v: Option<&serde_json::Value>| v.and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).map(|n| n as usize);
    // a result with no line count (a re-read Claude Code answers "file unchanged") falls
    // through to the range asked for: that is the range the session already holds
    if let Some((file, n)) = response.and_then(|r| r.get("file")).and_then(|f| num(f.get("numLines")).map(|n| (f, n))) {
        let start = num(file.get("startLine")).unwrap_or(1).max(1);
        return Some((start, start + n.max(1) - 1));
    }
    if let (Some(s), Some(e)) = (num(input.get("StartLine")), num(input.get("EndLine"))) {
        return Some((s.max(1), e));
    }
    let start = num(input.get("offset")).unwrap_or(1).max(1);
    let limit = num(input.get("limit")).unwrap_or(if input.get("AbsolutePath").is_some() { AGY_VIEW_DEFAULT } else { CLAUDE_READ_DEFAULT });
    Some((start, start + limit.max(1) - 1))
}

/// Overlapping or touching ranges joined, sorted.
fn merged(ranges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut r = ranges.to_vec();
    r.sort();
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (a, b) in r {
        match out.last_mut() {
            Some(last) if a <= last.1 + 1 => last.1 = last.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// Do the ranges, together, cover 1..=lines? Returns the first line no read reached.
pub fn first_unread(ranges: &[(usize, usize)], lines: usize) -> Option<usize> {
    let next = merged(ranges).first().filter(|(a, _)| *a <= 1).map_or(1, |(_, b)| b + 1);
    (next <= lines).then_some(next)
}

/// Every definition in a piece of source: name -> its signature (name and parameters
/// for a function; the header up to its body for a type), whitespace-normalized.
pub fn definitions(src: &str, ext: &str) -> HashMap<String, String> {
    static RS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static TS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static PY: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static SH: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = match ext {
        "rs" => RS.get_or_init(|| regex::Regex::new(r#"(?m)^[ \t]*(?:pub(?:\([^)]*\))?\s+)?(?:(?:async|const|unsafe)\s+)*(?:extern\s+"[^"]*"\s+)?(?:fn|struct|enum|trait|type|union)\s+([A-Za-z_]\w*)"#).unwrap()),
        "ts" | "tsx" | "js" | "mjs" | "cjs" => TS.get_or_init(|| regex::Regex::new(r"(?m)^[ \t]*(?:export\s+)?(?:default\s+)?(?:async\s+)?(?:function\s*\*?\s*([A-Za-z_$][\w$]*)|(?:class|interface|enum|type)\s+([A-Za-z_$][\w$]*)|(?:const|let)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?\([^)]*\)\s*(?::[^=]*)?=>)").unwrap()),
        "py" => PY.get_or_init(|| regex::Regex::new(r"(?m)^[ \t]*(?:async\s+)?(?:def|class)\s+([A-Za-z_]\w*)").unwrap()),
        "sh" | "bash" | "bats" | "" => SH.get_or_init(|| regex::Regex::new(r"(?m)^[ \t]*(?:function\s+)?([A-Za-z_][\w-]*)\s*\(\)").unwrap()),
        _ => return HashMap::new(),
    };
    let mut out = HashMap::new();
    for caps in re.captures_iter(src) {
        let Some(name) = (1..caps.len()).find_map(|i| caps.get(i)) else { continue };
        let start = caps.get(0).map(|m| m.start()).unwrap_or(0);
        out.insert(name.as_str().to_string(), signature(&src[start..]));
    }
    out
}

/// From a definition's start: through its balanced parameter list (a function), or up
/// to its body or terminator (a type). Whitespace collapsed so reflowing is no change.
fn signature(from: &str) -> String {
    let mut depth = 0i32;
    let mut end = from.len();
    let mut seen_paren = false;
    for (i, c) in from.char_indices() {
        match c {
            '(' => { depth += 1; seen_paren = true; }
            ')' => { depth -= 1; if depth == 0 && seen_paren { end = i + 1; break; } }
            '{' | ';' | '=' if depth == 0 && !seen_paren => { end = i; break; }
            '\n' if depth == 0 && !seen_paren && from[..i].trim_end().ends_with(':') => { end = i; break; }
            _ => {}
        }
    }
    // whitespace and a trailing comma are layout, not signature
    from[..end].split_whitespace().collect::<String>().replace(",)", ")")
}

/// The symbols an edit changes: defined in the old text and, in the new text, gone or
/// with a different signature. A definition only the new text has is an addition.
pub fn changed_symbols(old: &str, new: &str, ext: &str) -> Vec<String> {
    let after = definitions(new, ext);
    let mut out: Vec<String> = definitions(old, ext).into_iter()
        .filter(|(name, sig)| after.get(name) != Some(sig))
        .map(|(name, _)| name).collect();
    out.sort();
    out
}

fn ext_of(path: &str) -> &str {
    let base = path.rsplit('/').next().unwrap_or(path);
    base.rsplit_once('.').map(|(_, e)| e).unwrap_or("")
}

fn deny(policy: &Policy, what: String) -> HookResponse {
    HookResponse::deny(&permission_deny_json(&format!(
        "{POLICY} ({}): {what} The rule: {}", policy.label, policy.comment)))
}

/// The decision. `policy` is asked for the row only when the edit is one the rule governs.
pub fn decide(input: &HookInput, ledger: &Ledger, policy: impl FnOnce() -> Result<Policy, String>) -> HookResponse {
    let tool = input.tool_name_str();
    if tool != "Edit" && tool != "Write" { return HookResponse::allow(); }
    let path = input.get_tool_input_str("file_path");
    if !in_werk(&path) || path.contains("/target/") || path.contains("/node_modules/") {
        return HookResponse::allow();
    }
    let Ok(current) = std::fs::read_to_string(&path) else {
        return HookResponse::allow(); // a new file (or not text): nothing to have read
    };
    let policy = match policy() {
        Ok(p) => p,
        Err(e) => return HookResponse::deny(&permission_deny_json(&format!(
            "{POLICY}: the policy could not be read ({e}), so no edit to a werk file can be checked \
             and none passes. The row is policies/{POLICY} at the door ({}).", door()))),
    };
    let sid = input.session_id.as_deref().unwrap_or("");
    if !ledger.wrote(sid, &path) {
        let lines = current.lines().count();
        if let Some(line) = first_unread(&ledger.ranges(sid, &path), lines) {
            return deny(&policy, format!(
                "{path} has not been read in full in this session: line {line} of {lines} was never read. \
                 Read the whole file with the Read tool first (no offset/limit, or ranges that together \
                 cover 1..{lines}); a shell read (cat, sed, head) does not count. (A daemon restart \
                 forgets earlier reads; read it once more.)"));
        }
    }
    let (old, new) = if tool == "Write" {
        (current, input.get_tool_input_str("content"))
    } else {
        (input.get_tool_input_str("old_string"), input.get_tool_input_str("new_string"))
    };
    let unasked: Vec<String> = changed_symbols(&old, &new, ext_of(&path)).into_iter()
        .filter(|s| !ledger.looked_up(sid, s)).collect();
    if !unasked.is_empty() {
        return deny(&policy, format!(
            "this edit changes the definition of {} in {path}, and this session never asked for its \
             callers. Look them up first: cclsp find_references / find_definition, or an ast-grep search \
             naming it. Then edit.", unasked.join(", ")));
    }
    HookResponse::allow()
}

/// Record a lookup at PreToolUse: any cclsp or ast-grep call, or `ast-grep` on the shell.
pub fn record_lookup(input: &HookInput, ledger: &Ledger) {
    let tool = input.tool_name_str();
    let sid = input.session_id.as_deref().unwrap_or("");
    let is_lookup = tool.starts_with("mcp__cclsp__") || tool.starts_with("mcp__ast-grep__")
        || (tool == "Bash" && input.get_tool_input_str("command").contains("ast-grep"));
    if is_lookup {
        ledger.record_lookup(sid, &input.tool_input.as_ref().map(|v| v.to_string()).unwrap_or_default());
    }
}

/// Record what a finished call taught the session (PostToolUse): a Read's lines, a write's file.
pub fn record_result(input: &HookInput, ledger: &Ledger) {
    if input.tool_output_is_error == Some(true) { return; }
    let sid = input.session_id.as_deref().unwrap_or("");
    let path = input.get_tool_input_str("file_path");
    if path.is_empty() { return; }
    match input.tool_name_str() {
        "Read" => {
            let empty = serde_json::Value::Null;
            if let Some((a, b)) = read_range(input.tool_input.as_ref().unwrap_or(&empty), input.tool_response.as_ref()) {
                ledger.record_read(sid, &path, a, b);
            }
        }
        "Edit" | "Write" | "MultiEdit" => ledger.record_authored(sid, &path),
        _ => {}
    }
}

static POLICY_CACHE: Mutex<Option<(String, Policy, Instant)>> = Mutex::new(None);

/// The door the gate reads the row from.
pub fn door() -> String {
    std::env::var("ATHENA_MAKE_URL").unwrap_or_else(|_| "http://localhost:3360".into())
}

/// Test seam: the row as a door at `base` would serve it, without a door.
#[cfg(test)]
pub fn prime_policy(base: &str, p: Policy) {
    if let Ok(mut c) = POLICY_CACHE.lock() { *c = Some((base.to_string(), p, Instant::now())); }
}

/// The row from the door, cached for a minute. Only a good read is cached.
pub fn fetch_policy(base: &str) -> Result<Policy, String> {
    if let Ok(c) = POLICY_CACHE.lock() {
        if let Some((b, p, at)) = c.as_ref() { if b == base && at.elapsed() < POLICY_TTL { return Ok(p.clone()); } }
    }
    let url = format!("{base}/v1/policies/policies/{POLICY}");
    let body: serde_json::Value = ureq::get(&url).timeout(Duration::from_millis(1500)).call()
        .map_err(|e| format!("GET {url}: {e}"))?
        .into_json().map_err(|e| format!("GET {url}: {e}"))?;
    let p = policy_from_body(&body)?;
    if let Ok(mut c) = POLICY_CACHE.lock() { *c = Some((base.to_string(), p.clone(), Instant::now())); }
    Ok(p)
}

pub fn policy_from_body(body: &serde_json::Value) -> Result<Policy, String> {
    let data = body.get("data").unwrap_or(body);
    let field = |k: &str| data.get(k).and_then(|v| v.as_str()).map(str::trim).unwrap_or("").to_string();
    let (label, comment) = (field("label"), field("comment"));
    if comment.is_empty() { return Err(format!("the row {POLICY} has no rule text")); }
    Ok(Policy { label, comment })
}

/// The gate the PreToolUse dispatch calls: the row comes from the door.
pub fn check(input: &HookInput, ledger: &Ledger) -> HookResponse {
    decide(input, ledger, || fetch_policy(&door()))
}

#[cfg(test)]
mod read_code_first_5027 {
    use super::*;
    use serde_json::json;

    fn werk_file(content: &str) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let slot = dir.path().join("chorus-werk/kade-5027/src");
        std::fs::create_dir_all(&slot).unwrap();
        let f = slot.join("lib.rs");
        std::fs::write(&f, content).unwrap();
        (dir, f.to_string_lossy().into_owned())
    }
    fn input(tool: &str, sid: &str, ti: serde_json::Value) -> HookInput {
        HookInput { tool_name: Some(tool.into()), tool_input: Some(ti), session_id: Some(sid.into()), ..Default::default() }
    }
    fn policy() -> impl FnOnce() -> Result<Policy, String> {
        || Ok(Policy { label: "Read the code first".into(), comment: "ROW-TEXT: read the whole file before you change it.".into() })
    }
    fn denied(r: &HookResponse) -> Option<String> { r.stdout.clone().filter(|s| s.contains("deny")) }
    const SRC: &str = "fn a(x: u8) -> u8 {\n    x\n}\n\nfn b() {}\n";

    // ---- AC2: whole-file read ----
    #[test]
    fn read_code_first_hook_a_file_read_in_full_passes() {
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        l.record_read("s1", &f, 1, 5);
        let r = decide(&input("Edit", "s1", json!({"file_path": f, "old_string": "    x", "new_string": "    x + 1"})), &l, policy());
        assert!(denied(&r).is_none(), "{r:?}");
    }
    #[test]
    fn read_code_first_hook_ranges_that_together_cover_the_file_pass() {
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        l.record_read("s1", &f, 3, 5);
        l.record_read("s1", &f, 1, 2);
        assert!(denied(&decide(&input("Edit", "s1", json!({"file_path": f, "old_string": "x", "new_string": "y"})), &l, policy())).is_none());
    }
    #[test]
    fn read_code_first_hook_negative_a_partial_read_is_refused_naming_file_and_rule() {
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        l.record_read("s1", &f, 1, 3);
        let msg = denied(&decide(&input("Edit", "s1", json!({"file_path": f, "old_string": "    x", "new_string": "    y"})), &l, policy()))
            .expect("a partial read must be refused");
        assert!(msg.contains(&f) && msg.contains("line 4 of 5") && msg.contains("ROW-TEXT"), "{msg}");
    }
    #[test]
    fn read_code_first_hook_negative_no_read_and_a_shell_read_are_refused() {
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        // a shell read is a Bash call: it records nothing
        record_result(&input("Bash", "s1", json!({"command": format!("cat {f}")})), &l);
        assert!(denied(&decide(&input("Write", "s1", json!({"file_path": f, "content": SRC})), &l, policy())).is_some());
        // another session's full read does not count for this one
        l.record_read("s2", &f, 1, 5);
        assert!(denied(&decide(&input("Edit", "s1", json!({"file_path": f, "old_string": "x", "new_string": "y"})), &l, policy())).is_some());
    }
    #[test]
    fn read_code_first_hook_a_token_capped_read_counts_only_what_it_returned() {
        // measured 11-24: a Read of main.rs came back 1153 of 1816 lines, truncatedByTokenCap
        let resp = json!({"type": "text", "file": {"numLines": 1153, "startLine": 1, "totalLines": 1816, "truncatedByTokenCap": true}});
        assert_eq!(read_range(&json!({"file_path": "x"}), Some(&resp)), Some((1, 1153)));
        assert_eq!(read_range(&json!({"AbsolutePath": "x", "StartLine": 10, "EndLine": 40}), None), Some((10, 40)));
        assert_eq!(read_range(&json!({"file_path": "x", "offset": 100, "limit": 50}), None), Some((100, 149)));
        assert_eq!(first_unread(&[(1, 1153)], 1815), Some(1154));
        // the boundary: every line but the last is still unread at the last line
        assert_eq!(first_unread(&[(1, 1814)], 1815), Some(1815));
        assert_eq!(first_unread(&[(1, 1815)], 1815), None);
        // Silas 11-24: after a hooks restart a re-read can come back "file unchanged" with no
        // line count; the range asked for is what the session holds, so it counts
        let unchanged = json!({"type": "file_unchanged", "file": {"filePath": "x"}});
        assert_eq!(read_range(&json!({"file_path": "x"}), Some(&unchanged)), Some((1, 2000)));
        assert_eq!(read_range(&json!({"file_path": "x", "offset": 40, "limit": 10}), Some(&unchanged)), Some((40, 49)));
        assert_eq!(first_unread(&[(1, 1153), (1154, 1816)], 1815), None);
    }
    #[test]
    fn read_code_first_hook_new_files_written_files_and_paths_outside_werk_pass() {
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        let new = f.replace("lib.rs", "new.rs");
        assert!(denied(&decide(&input("Write", "s1", json!({"file_path": new, "content": "x"})), &l, policy())).is_none());
        l.record_authored("s1", &f);
        assert!(denied(&decide(&input("Edit", "s1", json!({"file_path": f, "old_string": "x", "new_string": "y"})), &l, policy())).is_none());
        assert!(denied(&decide(&input("Edit", "s9", json!({"file_path": "/tmp/scratch.rs", "old_string": "x", "new_string": "y"})), &l, policy())).is_none());
    }
    #[test]
    fn read_code_first_hook_a_row_is_asked_for_only_when_the_rule_governs_the_edit() {
        let l = Ledger::default();
        let asked = std::cell::Cell::new(false);
        let r = decide(&input("Edit", "s1", json!({"file_path": "/tmp/x.rs", "old_string": "a", "new_string": "b"})), &l,
            || { asked.set(true); Err("down".into()) });
        assert!(denied(&r).is_none() && !asked.get(), "a path outside the werk never reads the row");
    }
    #[test]
    fn read_code_first_hook_a_canonical_read_counts_for_the_werk_twin() {
        let root = crate::shared::state_paths::chorus_root();
        assert_eq!(key(&format!("{root}/platform/x.rs")), "platform/x.rs");
        assert_eq!(key("/a/chorus-werk/kade-5027/platform/x.rs"), "platform/x.rs");
    }

    // ---- AC3: the rule comes from the row ----
    #[test]
    fn read_code_first_reads_policy_the_refusal_carries_the_rows_words() {
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        let other = || Ok(Policy { label: "L2".into(), comment: "A DIFFERENT ROW SENTENCE".into() });
        let msg = denied(&decide(&input("Edit", "s1", json!({"file_path": f, "old_string": "x", "new_string": "y"})), &l, other)).unwrap();
        assert!(msg.contains("A DIFFERENT ROW SENTENCE") && msg.contains("L2") && !msg.contains("ROW-TEXT"), "{msg}");
    }
    #[test]
    fn read_code_first_reads_policy_negative_an_absent_row_refuses_and_says_so() {
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        l.record_read("s1", &f, 1, 5); // even a fully read file: no row, no pass
        let msg = denied(&decide(&input("Edit", "s1", json!({"file_path": f, "old_string": "x", "new_string": "y"})), &l, || Err("404".into())))
            .expect("an unreadable policy must refuse, never pass");
        assert!(msg.contains("could not be read") && msg.contains("404"), "{msg}");
        assert!(policy_from_body(&json!({"data": {"label": "x", "comment": ""}})).is_err());
        assert!(fetch_policy("http://127.0.0.1:1").is_err(), "an unreachable door is an error, never a policy");
    }
    #[test]
    fn read_code_first_reads_policy_body_shape() {
        let p = policy_from_body(&json!({"data": {"label": "Read the code first", "comment": "rule"}})).unwrap();
        assert_eq!(p, Policy { label: "Read the code first".into(), comment: "rule".into() });
    }

    // ---- AC4: a changed definition needs a lookup ----
    #[test]
    fn read_code_first_lsp_ast_body_edits_are_not_checked() {
        assert!(changed_symbols("fn a(x: u8) -> u8 {\n    x\n}", "fn a(x: u8) -> u8 {\n    x + 1\n}", "rs").is_empty());
        assert!(changed_symbols("    x\n", "    x + 1\n", "rs").is_empty());
        // reflowing a signature is no change; adding a new definition is an addition
        assert!(changed_symbols("fn a(x: u8, y: u8)", "fn a(\n    x: u8,\n    y: u8,\n)", "rs").is_empty());
        assert!(changed_symbols("fn a() {}\n", "fn a() {}\nfn c() {}\n", "rs").is_empty());
    }
    #[test]
    fn read_code_first_lsp_ast_renames_and_parameter_changes_are_found() {
        assert_eq!(changed_symbols("pub fn load(p: &str) {", "pub fn load(p: &str, strict: bool) {", "rs"), ["load"]);
        assert_eq!(changed_symbols("fn old_name() {}", "fn new_name() {}", "rs"), ["old_name"]);
        assert_eq!(changed_symbols("pub struct Row {", "pub struct Line {", "rs"), ["Row"]);
        assert_eq!(changed_symbols("export function go(a) {", "export function go(a, b) {", "ts"), ["go"]);
        assert_eq!(changed_symbols("def run(x):\n", "def run(x, y):\n", "py"), ["run"]);
        assert_eq!(changed_symbols("handler() {\n", "on_exit() {\n", "sh"), ["handler"]);
    }
    #[test]
    fn read_code_first_lsp_ast_negative_a_signature_change_without_a_lookup_is_refused() {
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        l.record_read("s1", &f, 1, 5);
        let edit = json!({"file_path": f, "old_string": "fn a(x: u8) -> u8 {", "new_string": "fn a(x: u8, y: u8) -> u8 {"});
        let msg = denied(&decide(&input("Edit", "s1", edit.clone()), &l, policy())).expect("an unasked signature change must be refused");
        assert!(msg.contains("definition of a ") && msg.contains("find_references"), "{msg}");
        // a lookup naming another symbol does not count
        record_lookup(&input("mcp__cclsp__find_references", "s1", json!({"symbol_name": "ab"})), &l);
        assert!(denied(&decide(&input("Edit", "s1", edit.clone()), &l, policy())).is_some());
        // the lookup of `a` (here ast-grep on the shell) lets it through
        record_lookup(&input("Bash", "s1", json!({"command": "ast-grep -p 'a($$$)' src"})), &l);
        assert!(denied(&decide(&input("Edit", "s1", edit), &l, policy())).is_none());
    }
    #[test]
    fn read_code_first_lsp_ast_a_whole_file_write_that_renames_is_checked() {
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        l.record_read("s1", &f, 1, 5);
        let msg = denied(&decide(&input("Write", "s1", json!({"file_path": f, "content": SRC.replace("fn b()", "fn c()")})), &l, policy())).unwrap();
        assert!(msg.contains("definition of b "), "{msg}");
        record_lookup(&input("mcp__cclsp__find_references", "s1", json!({"symbol_name": "b"})), &l);
        assert!(denied(&decide(&input("Write", "s1", json!({"file_path": f, "content": SRC.replace("fn b()", "fn c()")})), &l, policy())).is_none());
    }
}
