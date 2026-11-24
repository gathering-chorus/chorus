// @domain: policies — the read-the-code-first rule's enforcement (#5027)
//! Read the code first (#5027).
//!
//! Jeff, 2026-11-21 17:37: "its unacceptable for all of u to not read the code … a stop
//! hook as the very first step of werk-code". Borg Queen measured 351 sessions (11-22):
//! 22% of first edits to a file came after reading the whole file; the rest followed a
//! partial read, a shell peek, or nothing. cclsp was called 0 times in 30 days.
//!
//! Two rules, both at PreToolUse on an Edit, MultiEdit or Write to an existing file in a
//! werk (Claude Code, and agy's view_file / replace_file_content / write_to_file, which
//! its AgentRuntime row maps to the same names):
//! 1. The session has read the whole file as it is now: the line ranges its reads
//!    returned, together, cover 1..N, and the file has not changed since (length and
//!    mtime). A shell read (cat, sed) is not a Read and does not count. A file the
//!    session wrote itself counts as read.
//! 2. An edit that changes a definition (a function's name or parameters, a type's name
//!    or header) needs a lookup of that symbol first: a cclsp or ast-grep call whose
//!    symbol or pattern names it. The edit is applied to the whole file and the
//!    definitions before and after are compared, so a narrow edit inside a parameter
//!    list is seen.
//!
//! The rule's words come from the policy row `read-the-code-first` (the policies
//! domain), never a copy here. A missing or empty row refuses the edit and says so; a
//! door that does not answer uses the last row it served (a blip must not lock the team
//! out), and with none ever served, refuses.
//!
//! Reads and lookups are recorded live by the daemon (PostToolUse Read, PreToolUse
//! lookup), as tdd_gate's test edits are (#3278): the transcript flushes a turn late.
//! They are also appended to one file per session under the daemon's run dir, so a
//! daemon restart does not forget them. Claude Code answers a repeated read "file
//! unchanged" with no lines, which counts for nothing; a read with a new range returns
//! the lines (Silas measured 11-24), and the refusal says so.
//!
//! Kept limits, named (Wren's read, 11-24): edits made through the shell (sed -i, a
//! heredoc, git apply) are not gated; a session whose context was compacted still counts
//! its earlier reads; a return-type change alone is not a changed definition (AC4 names
//! name and parameters); an edit to a file the session wrote itself is not re-checked
//! against later outside changes; a MultiEdit is gated by this rule only, not by the
//! Edit arm's other gates (that predates #5027; Wren 11-24: name it, do not merge).

use crate::types::{permission_deny_json, HookInput, HookResponse};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const POLICY: &str = "read-the-code-first";
/// agy's view_file shows at most this many lines when no range is asked for (BQ, 11-22).
/// Only a read with no result file (not Claude Code) falls back to it.
const AGY_VIEW_DEFAULT: usize = 800;
const POLICY_TTL: Duration = Duration::from_secs(60);
const MAX_LOOKUPS: usize = 500;
const MAX_LOOKUP_CHARS: usize = 2000;
/// The last row the door served, kept beside the session files.
const POLICY_FILE: &str = "_policy.json";

/// The policy row's words, as the door serves them.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Policy {
    pub label: String,
    pub comment: String,
}

/// A file's identity for staleness: its length and mtime (ns) when it was read.
type Stamp = (u64, i64);

fn stamp(path: &str) -> Option<Stamp> {
    let m = std::fs::metadata(path).ok()?;
    let ns = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos() as i64;
    Some((m.len(), ns))
}

/// What a session has read of one file, and the file as it was when it read it.
#[derive(Clone, Default)]
struct FileReads { stamp: Option<Stamp>, ranges: Vec<(usize, usize)> }

/// What each session has read, written and looked up. One per daemon (AppState).
/// With a dir, every record is also appended to `<dir>/<session>.jsonl` and a session
/// the daemon has not seen since it started is loaded from there first.
#[derive(Default)]
pub struct Ledger {
    reads: Mutex<HashMap<(String, String), FileReads>>,
    authored: Mutex<HashSet<(String, String)>>,
    lookups: Mutex<HashMap<String, Vec<String>>>,
    dir: Option<PathBuf>,
    loaded: Mutex<HashSet<String>>,
}

/// One line of a session's ledger file.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "k", rename_all = "lowercase")]
enum Entry {
    Read { path: String, a: usize, b: usize, #[serde(default)] stamp: Option<Stamp> },
    Wrote { path: String },
    Lookup { text: String },
}

/// A session id that is safe as a file name.
fn safe_sid(sid: &str) -> bool {
    !sid.is_empty() && sid.len() <= 128 && sid.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl Ledger {
    /// A ledger that survives a daemon restart. Files older than KEEP are dropped.
    pub fn persistent(dir: PathBuf) -> Self {
        const KEEP: Duration = Duration::from_secs(3 * 24 * 3600);
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let old = e.metadata().and_then(|m| m.modified()).ok()
                    .and_then(|t| t.elapsed().ok()).is_some_and(|age| age > KEEP);
                if old { let _ = std::fs::remove_file(e.path()); }
            }
        }
        Self { dir: Some(dir), ..Self::default() }
    }

    fn file(&self, sid: &str) -> Option<PathBuf> {
        self.dir.as_ref().filter(|_| safe_sid(sid)).map(|d| d.join(format!("{sid}.jsonl")))
    }

    fn persist(&self, sid: &str, e: &Entry) {
        use std::io::Write;
        let Some(f) = self.file(sid) else { return };
        let Ok(line) = serde_json::to_string(e) else { return };
        if let Ok(mut fh) = std::fs::OpenOptions::new().create(true).append(true).open(&f) {
            let _ = fh.write_all(format!("{line}\n").as_bytes());
        }
    }

    /// Load a session's file once, the first time this daemon is asked about it.
    fn load(&self, sid: &str) {
        // the replay happens under the lock, so a concurrent call waits for the whole
        // ledger instead of reading half of it (code review, run 4)
        let Ok(mut loaded) = self.loaded.lock() else { return };
        if !loaded.insert(sid.to_string()) { return; }
        let Some(f) = self.file(sid) else { return };
        let Ok(text) = std::fs::read_to_string(&f) else { return };
        for e in text.lines().filter_map(|l| serde_json::from_str::<Entry>(l).ok()) {
            match e {
                Entry::Read { path, a, b, stamp } => self.add_read(sid, &path, a, b, stamp),
                Entry::Wrote { path } => self.add_authored(sid, &path),
                Entry::Lookup { text } => self.add_lookup(sid, &text),
            }
        }
        drop(loaded);
    }

    /// A read of lines first..=last of `path`, stamped with the file as it is now.
    pub fn record_read(&self, sid: &str, path: &str, first: usize, last: usize) {
        if sid.is_empty() || last < first { return; }
        let s = stamp(path);
        self.load(sid);
        self.add_read(sid, path, first, last, s);
        self.persist(sid, &Entry::Read { path: path.to_string(), a: first, b: last, stamp: s });
    }
    pub fn record_authored(&self, sid: &str, path: &str) {
        if sid.is_empty() { return; }
        self.load(sid);
        self.add_authored(sid, path);
        self.persist(sid, &Entry::Wrote { path: path.to_string() });
    }
    pub fn record_lookup(&self, sid: &str, text: &str) {
        if sid.is_empty() || text.trim().is_empty() { return; }
        // bounded: the daemon lives for days; the newest lookups are the ones an edit needs
        let text: String = text.chars().take(MAX_LOOKUP_CHARS).collect();
        self.load(sid);
        self.add_lookup(sid, &text);
        self.persist(sid, &Entry::Lookup { text });
    }

    fn add_read(&self, sid: &str, path: &str, first: usize, last: usize, s: Option<Stamp>) {
        if let Ok(mut m) = self.reads.lock() {
            let r = m.entry((sid.to_string(), path.to_string())).or_default();
            // a read of a changed file starts the count again: the old lines are not these
            if r.stamp != s { *r = FileReads { stamp: s, ranges: Vec::new() }; }
            // kept merged, so a long session holds a few ranges per file, not every read
            r.ranges.push((first.max(1), last));
            r.ranges = merged(&r.ranges);
        }
    }
    fn add_authored(&self, sid: &str, path: &str) {
        if let Ok(mut s) = self.authored.lock() { s.insert((sid.to_string(), path.to_string())); }
    }
    fn add_lookup(&self, sid: &str, text: &str) {
        if let Ok(mut m) = self.lookups.lock() {
            let v = m.entry(sid.to_string()).or_default();
            v.push(text.to_string());
            if v.len() > MAX_LOOKUPS { v.remove(0); }
        }
    }

    /// The ranges read of `path` while it was the file it is now; none if it changed since.
    fn ranges(&self, sid: &str, path: &str) -> Vec<(usize, usize)> {
        self.load(sid);
        let now = stamp(path);
        self.reads.lock().ok()
            .and_then(|m| m.get(&(sid.to_string(), path.to_string())).filter(|r| r.stamp == now).map(|r| r.ranges.clone()))
            .unwrap_or_default()
    }
    fn wrote(&self, sid: &str, path: &str) -> bool {
        self.load(sid);
        self.authored.lock().map(|s| s.contains(&(sid.to_string(), path.to_string()))).unwrap_or(false)
    }
    fn looked_up(&self, sid: &str, symbol: &str) -> bool {
        self.load(sid);
        let Ok(re) = regex::Regex::new(&format!(r"(?:^|[^\w$-]){}(?:$|[^\w$-])", regex::escape(symbol))) else { return false };
        self.lookups.lock().map(|m| m.get(sid).is_some_and(|v| v.iter().any(|t| re.is_match(t)))).unwrap_or(false)
    }
}

/// A file in a card's werk: `…/chorus-werk/<role>-<card>/<path>`. Canonical, /tmp and
/// everything else are not werk-code.
fn in_werk(path: &str) -> bool {
    path.split_once("/chorus-werk/").is_some_and(|(_, rest)| rest.split_once('/').is_some_and(|(slot, rel)| !slot.is_empty() && !rel.is_empty()))
}

/// The lines a Read returned. A Claude Code result says how far it got (a token-capped
/// read stops short); one with no line count (a re-read answered "file unchanged", an
/// image) returned no lines and counts for nothing. With no result at all (agy sends
/// none), the range asked for: StartLine/EndLine, or offset/limit, else agy's default.
pub fn read_range(input: &Value, response: Option<&Value>) -> Option<(usize, usize)> {
    let num = |v: Option<&Value>| v.and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).map(|n| n as usize);
    if let Some(r) = response.filter(|r| !r.is_null()) {
        let file = r.get("file")?;
        let start = num(file.get("startLine")).unwrap_or(1).max(1);
        let n = num(file.get("numLines"))?;
        return Some((start, start + n.max(1) - 1));
    }
    if let (Some(s), Some(e)) = (num(input.get("StartLine")), num(input.get("EndLine"))) {
        return Some((s.max(1), e));
    }
    let start = num(input.get("offset")).unwrap_or(1).max(1);
    let limit = num(input.get("limit")).unwrap_or(AGY_VIEW_DEFAULT);
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

/// The language of a file: its extension, or for a script with none, its `#!` line.
fn lang(path: &str, content: &str) -> String {
    let base = path.rsplit('/').next().unwrap_or(path);
    if let Some((_, e)) = base.rsplit_once('.') { return e.to_string(); }
    let first = content.lines().next().unwrap_or("");
    if !first.starts_with("#!") { return String::new(); }
    if first.contains("python") { "py" } else if first.contains("node") { "js" }
    else if first.ends_with("/sh") || first.ends_with(" sh") || first.contains("bash") || first.contains("zsh") { "sh" } else { "" }.to_string()
}

/// Every definition in a piece of source, as (name, signature): name and parameters for
/// a function, the header up to its body for a type, whitespace-normalized. A name may
/// appear more than once (`fn new` in two impls).
pub fn definitions(src: &str, ext: &str) -> Vec<(String, String)> {
    static RS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static TS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static PY: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static SH: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = match ext {
        "rs" => RS.get_or_init(|| regex::Regex::new(r#"(?m)^[ \t]*(?:pub(?:\([^)]*\))?\s+)?(?:(?:async|const|unsafe)\s+)*(?:extern\s+"[^"]*"\s+)?(?:fn|struct|enum|trait|type|union)\s+([A-Za-z_]\w*)"#).unwrap()),
        "ts" | "tsx" | "js" | "mjs" | "cjs" => TS.get_or_init(|| regex::Regex::new(r"(?m)^[ \t]*(?:export\s+)?(?:default\s+)?(?:async\s+)?(?:function\s*\*?\s*([A-Za-z_$][\w$]*)|(?:class|interface|enum|type)\s+([A-Za-z_$][\w$]*)|(?:const|let)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?\([^)]*\)\s*(?::[^=]*)?=>)").unwrap()),
        "py" => PY.get_or_init(|| regex::Regex::new(r"(?m)^[ \t]*(?:async\s+)?(?:def|class)\s+([A-Za-z_]\w*)").unwrap()),
        "sh" | "bash" | "bats" => SH.get_or_init(|| regex::Regex::new(r"(?m)^[ \t]*(?:function\s+)?([A-Za-z_][\w-]*)\s*\(\)").unwrap()),
        _ => return Vec::new(),
    };
    re.captures_iter(src).filter_map(|caps| {
        let name = (1..caps.len()).find_map(|i| caps.get(i))?;
        let start = caps.get(0)?.start();
        Some((name.as_str().to_string(), signature(&src[start..])))
    }).collect()
}

/// From a definition's start: through its balanced parameter list (a function, an arrow
/// function), or up to its body or terminator (a type). Whitespace and a trailing comma
/// are layout, not signature.
fn signature(from: &str) -> String {
    let mut depth = 0i32;
    let mut end = from.len();
    let mut seen_paren = false;
    for (i, c) in from.char_indices() {
        match c {
            '(' => { depth += 1; seen_paren = true; }
            ')' => { depth -= 1; if depth == 0 && seen_paren { end = i + 1; break; } }
            // `const f = (a) =>`: the `=` leads to the parameters, it does not end them
            '=' if depth == 0 && !seen_paren && {
                let rest = from[i + 1..].trim_start();
                rest.strip_prefix("async").unwrap_or(rest).trim_start().starts_with('(')
            } => {}
            '{' | ';' | '=' if depth == 0 && !seen_paren => { end = i; break; }
            '\n' if depth == 0 && !seen_paren && from[..i].trim_end().ends_with(':') => { end = i; break; }
            _ => {}
        }
    }
    from[..end].split_whitespace().collect::<String>().replace(",)", ")")
}

/// The symbols a change of the whole file changes: a definition in the old text whose
/// signature the new text no longer has (gone, renamed, or different parameters). A
/// definition only the new text has is an addition. Compared as multisets, so two
/// same-named definitions are both checked.
pub fn changed_symbols(old: &str, new: &str, ext: &str) -> Vec<String> {
    let mut after: HashMap<(String, String), usize> = HashMap::new();
    for d in definitions(new, ext) { *after.entry(d).or_default() += 1; }
    let mut out: Vec<String> = Vec::new();
    for d in definitions(old, ext) {
        match after.get_mut(&d) {
            Some(n) if *n > 0 => *n -= 1,
            _ => out.push(d.0),
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The file as it will be after this call: Write's content, or the Edit / MultiEdit
/// replacements applied to the text on disk.
fn after_edit(tool: &str, input: &Value, current: &str) -> String {
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let apply = |text: String, e: &Value| {
        let (old, new) = (s(e, "old_string"), s(e, "new_string"));
        if old.is_empty() { return text; }
        if e.get("replace_all").and_then(Value::as_bool) == Some(true) { text.replace(&old, &new) } else { text.replacen(&old, &new, 1) }
    };
    match tool {
        // a patch (codex / opencode apply_patch) arrives as a Write carrying only its hunk:
        // its old and new lines are the edit, not the whole file (code review, run 4)
        "Write" if input.get("operation").and_then(Value::as_str) == Some("update") => apply(current.to_string(), input),
        "Write" => s(input, "content"),
        "MultiEdit" => input.get("edits").and_then(Value::as_array).map_or(current.to_string(),
            |edits| edits.iter().fold(current.to_string(), apply)),
        _ => apply(current.to_string(), input),
    }
}

fn deny(policy: &Policy, what: String) -> HookResponse {
    HookResponse::deny(&permission_deny_json(&format!(
        "{POLICY} ({}): {what} The rule: {}", policy.label, policy.comment)))
}

/// The decision. `policy` is asked for the row only when the edit is one the rule governs.
pub fn decide(input: &HookInput, ledger: &Ledger, policy: impl FnOnce() -> Result<Policy, String>) -> HookResponse {
    let tool = input.tool_name_str();
    if !matches!(tool, "Edit" | "Write" | "MultiEdit") { return HookResponse::allow(); }
    let path = input.get_tool_input_str("file_path");
    if !in_werk(&path) || path.contains("/target/") || path.contains("/node_modules/") {
        return HookResponse::allow();
    }
    let Ok(current) = std::fs::read_to_string(&path) else {
        return HookResponse::allow(); // a new file (or not text): nothing to have read
    };
    let Some(sid) = input.session_id.as_deref().filter(|s| !s.is_empty()) else {
        // nothing is ever recorded without a session, so nothing could pass: say so, allow
        tracing::warn!(gate = POLICY, file = %path, "no session id: reads cannot be counted, edit not checked");
        return HookResponse::allow();
    };
    let policy = match policy() {
        Ok(p) => p,
        Err(e) => return HookResponse::deny(&permission_deny_json(&format!(
            "{POLICY}: the policy could not be read ({e}), so no edit to a werk file can be checked \
             and none passes. The row is policies/{POLICY} at the door ({}).", door()))),
    };
    if !ledger.wrote(sid, &path) {
        let lines = current.lines().count();
        if let Some(line) = first_unread(&ledger.ranges(sid, &path), lines) {
            return deny(&policy, format!(
                "{path} has not been read in full in this session, as it is now: line {line} of {lines} \
                 was never read (or the file changed since). Read the whole file with the Read tool first \
                 (no offset/limit, or ranges that together cover 1..{lines}); a shell read (cat, sed, head) \
                 does not count. If the Read answers that the file is unchanged, read it again with \
                 offset 1 and a limit past line {lines}: that returns the lines, and they count."));
        }
    }
    let after = after_edit(tool, input.tool_input.as_ref().unwrap_or(&Value::Null), &current);
    let unasked: Vec<String> = changed_symbols(&current, &after, &lang(&path, &current)).into_iter()
        .filter(|s| !ledger.looked_up(sid, s)).collect();
    if !unasked.is_empty() {
        return deny(&policy, format!(
            "this edit changes the definition of {} in {path}, and this session never asked for its \
             callers. Look them up first: cclsp find_references / find_definition, or an ast-grep search \
             naming it. Then edit.", unasked.join(", ")));
    }
    HookResponse::allow()
}

/// The part of a lookup that names what was looked up: an MCP call's symbol or pattern
/// fields, or the arguments of a shell `ast-grep` command (paths and flags left out).
/// Empty when the call is not a lookup.
pub fn lookup_text(tool: &str, input: &Value) -> String {
    const FIELDS: &[&str] = &["symbol_name", "symbolName", "symbol", "name", "query", "pattern", "rule", "yaml"];
    if tool.starts_with("mcp__cclsp__") || tool.starts_with("mcp__ast-grep__") {
        return FIELDS.iter().filter_map(|k| input.get(*k).and_then(Value::as_str)).collect::<Vec<_>>().join(" ");
    }
    if tool != "Bash" { return String::new(); }
    static SG: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    // ast-grep as a command (line start, or after ; & | ( ), to the end of its line: a
    // quoted pattern may hold ; or |. Flags are kept (--pattern=x names x); paths are not
    // (Wren I: a path must not count as a lookup of the words in it).
    let re = SG.get_or_init(|| regex::Regex::new(r"(?m)(?:^|[;&|(])\s*ast-grep\s+(.*)$").unwrap());
    let cmd = input.get("command").and_then(Value::as_str).unwrap_or("");
    // a word that is a path (a slash, and nothing a pattern holds) is not the lookup
    let path_like = |t: &str| t.contains('/') && !t.contains(['(', '$', '\'', '"']);
    re.captures_iter(cmd).filter_map(|c| c.get(1))
        .flat_map(|m| m.as_str().split_whitespace()).filter(|t| !path_like(t))
        .collect::<Vec<_>>().join(" ")
}

/// Record a lookup at PreToolUse.
pub fn record_lookup(input: &HookInput, ledger: &Ledger) {
    let sid = input.session_id.as_deref().unwrap_or("");
    let text = lookup_text(input.tool_name_str(), input.tool_input.as_ref().unwrap_or(&Value::Null));
    ledger.record_lookup(sid, &text);
}

/// Record what a finished call taught the session (PostToolUse): a Read's lines, a write's file.
pub fn record_result(input: &HookInput, ledger: &Ledger) {
    if input.tool_output_is_error == Some(true) { return; }
    let sid = input.session_id.as_deref().unwrap_or("");
    let path = input.get_tool_input_str("file_path");
    if path.is_empty() { return; }
    match input.tool_name_str() {
        "Read" => {
            if let Some((a, b)) = read_range(input.tool_input.as_ref().unwrap_or(&Value::Null), input.tool_response.as_ref()) {
                ledger.record_read(sid, &path, a, b);
            }
        }
        "Edit" | "Write" | "MultiEdit" => ledger.record_authored(sid, &path),
        _ => {}
    }
}

static POLICY_CACHE: Mutex<Option<(String, Policy, Instant)>> = Mutex::new(None);
/// When the door last failed to answer, per door: no new GET for DOOR_DOWN_TTL.
static DOOR_DOWN: Mutex<Option<(String, Instant)>> = Mutex::new(None);
const DOOR_DOWN_TTL: Duration = Duration::from_secs(30);

/// The door the gate reads the row from.
pub fn door() -> String {
    std::env::var("ATHENA_MAKE_URL").unwrap_or_else(|_| "http://localhost:3360".into())
}

/// Test seam: the row as a door at `base` would serve it, without a door.
#[cfg(test)]
pub fn prime_policy(base: &str, p: Policy) {
    if let Ok(mut c) = POLICY_CACHE.lock() { *c = Some((base.to_string(), p, Instant::now())); }
}

/// The row from the door, cached for a minute; the last good row is also kept in
/// `last_good` (a file). A 404 or an empty row is the row's own answer and refuses. A
/// door that does not answer at all uses the last good row, so a blip does not lock
/// every role out; with none ever served it refuses.
pub fn fetch_policy(base: &str, last_good: Option<&Path>) -> Result<Policy, String> {
    if let Ok(c) = POLICY_CACHE.lock() {
        if let Some((b, p, at)) = c.as_ref() { if b == base && at.elapsed() < POLICY_TTL { return Ok(p.clone()); } }
    }
    let url = format!("{base}/v1/policies/policies/{POLICY}");
    let remembered = || POLICY_CACHE.lock().ok().and_then(|c| c.as_ref().filter(|(b, _, _)| b == base).map(|(_, p, _)| p.clone()))
        .or_else(|| last_good.and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| serde_json::from_str(&t).ok()));
    // a door that just failed is not asked again for a while: each edit would wait 1.5 s
    let down = DOOR_DOWN.lock().ok().and_then(|d| d.as_ref().filter(|(b, at)| b == base && at.elapsed() < DOOR_DOWN_TTL).map(|_| ()));
    if down.is_some() {
        return remembered().ok_or_else(|| format!("GET {url}: the door did not answer, and no row was ever read"));
    }
    let mark_down = || { if let Ok(mut d) = DOOR_DOWN.lock() { *d = Some((base.to_string(), Instant::now())); } };
    match ureq::get(&url).timeout(Duration::from_millis(1500)).call() {
        Ok(resp) => {
            let body: Value = resp.into_json().map_err(|e| format!("GET {url}: {e}"))?;
            let p = policy_from_body(&body)?;
            if let Ok(mut c) = POLICY_CACHE.lock() { *c = Some((base.to_string(), p.clone(), Instant::now())); }
            if let (Some(f), Ok(text)) = (last_good, serde_json::to_string(&p)) { let _ = std::fs::write(f, text); }
            Ok(p)
        }
        // the row is gone (or the request is wrong): the door's own answer, it refuses
        Err(ureq::Error::Status(code, _)) if code < 500 => Err(format!("GET {url}: the door answered {code}")),
        // a 5xx or no answer is the door failing, not the row: the last good row stands
        Err(e) => {
            mark_down();
            remembered().ok_or_else(|| format!("GET {url}: {e}, and no row was ever read"))
        }
    }
}

pub fn policy_from_body(body: &Value) -> Result<Policy, String> {
    let data = body.get("data").unwrap_or(body);
    let field = |k: &str| data.get(k).and_then(|v| v.as_str()).map(str::trim).unwrap_or("").to_string();
    let (label, comment) = (field("label"), field("comment"));
    if comment.is_empty() { return Err(format!("the row {POLICY} has no rule text")); }
    Ok(Policy { label, comment })
}

/// The gate the PreToolUse dispatch calls: the row comes from the door.
pub fn check(input: &HookInput, ledger: &Ledger) -> HookResponse {
    let last_good = ledger.dir.as_ref().map(|d| d.join(POLICY_FILE));
    decide(input, ledger, || fetch_policy(&door(), last_good.as_deref()))
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
    fn input(tool: &str, sid: &str, ti: Value) -> HookInput {
        HookInput { tool_name: Some(tool.into()), tool_input: Some(ti), session_id: Some(sid.into()), ..Default::default() }
    }
    fn policy() -> impl FnOnce() -> Result<Policy, String> {
        || Ok(Policy { label: "Read the code first".into(), comment: "ROW-TEXT: read the whole file before you change it.".into() })
    }
    fn denied(r: &HookResponse) -> Option<String> { r.stdout.clone().filter(|s| s.contains("deny")) }
    const SRC: &str = "fn a(x: u8) -> u8 {\n    x\n}\n\nfn b() {}\n";
    /// a body edit: it changes no definition
    fn body_edit(f: &str) -> Value { json!({"file_path": f, "old_string": "    x\n", "new_string": "    x + 1\n"}) }

    // ---- AC2: whole-file read ----
    #[test]
    fn read_code_first_hook_a_file_read_in_full_passes() {
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        l.record_read("s1", &f, 1, 5);
        let r = decide(&input("Edit", "s1", body_edit(&f)), &l, policy());
        assert!(denied(&r).is_none(), "{r:?}");
    }
    #[test]
    fn read_code_first_hook_ranges_that_together_cover_the_file_pass() {
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        l.record_read("s1", &f, 3, 5);
        l.record_read("s1", &f, 1, 2);
        assert!(denied(&decide(&input("Edit", "s1", body_edit(&f)), &l, policy())).is_none());
    }
    #[test]
    fn read_code_first_hook_negative_a_partial_read_is_refused_naming_file_and_rule() {
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        l.record_read("s1", &f, 1, 3);
        let msg = denied(&decide(&input("Edit", "s1", body_edit(&f)), &l, policy()))
            .expect("a partial read must be refused");
        assert!(msg.contains(&f) && msg.contains("line 4 of 5") && msg.contains("ROW-TEXT"), "{msg}");
        // the refusal names the way out of a "file unchanged" answer (Silas, 11-24)
        assert!(msg.contains("offset 1 and a limit past line 5"), "{msg}");
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
        assert!(denied(&decide(&input("Edit", "s1", body_edit(&f)), &l, policy())).is_some());
    }
    #[test]
    fn read_code_first_hook_negative_a_file_changed_since_the_read_is_unread() {
        // Wren F: git, sed -i or another role changed the file after the read
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        l.record_read("s1", &f, 1, 5);
        std::fs::write(&f, format!("{SRC}// a line someone else added\n")).unwrap();
        assert!(denied(&decide(&input("Edit", "s1", body_edit(&f)), &l, policy())).is_some(), "a stale read must not count");
        l.record_read("s1", &f, 1, 6);
        assert!(denied(&decide(&input("Edit", "s1", body_edit(&f)), &l, policy())).is_none(), "a fresh read counts");
    }
    #[test]
    fn read_code_first_hook_a_token_capped_read_counts_only_what_it_returned() {
        // measured 11-24: a Read of main.rs came back 1153 of 1816 lines, truncatedByTokenCap
        let resp = json!({"type": "text", "file": {"numLines": 1153, "startLine": 1, "totalLines": 1816, "truncatedByTokenCap": true}});
        assert_eq!(read_range(&json!({"file_path": "x"}), Some(&resp)), Some((1, 1153)));
        assert_eq!(first_unread(&[(1, 1153)], 1815), Some(1154));
        assert_eq!(first_unread(&[(1, 1153), (1154, 1816)], 1815), None);
        // the boundary: every line but the last is still unread at the last line
        assert_eq!(first_unread(&[(1, 1814)], 1815), Some(1815));
        assert_eq!(first_unread(&[(1, 1815)], 1815), None);
    }
    #[test]
    fn read_code_first_hook_negative_an_unchanged_reread_counts_for_nothing() {
        // a re-read answered "file unchanged" returned no lines, so a capped read followed
        // by one cannot pass for a whole read (code review, run 2); an image neither
        let unchanged = json!({"type": "file_unchanged", "file": {"filePath": "x"}});
        assert_eq!(read_range(&json!({"file_path": "x"}), Some(&unchanged)), None);
        assert_eq!(read_range(&json!({"file_path": "x"}), Some(&json!({"type": "image"}))), None);
    }
    #[test]
    fn read_code_first_hook_a_read_with_no_result_counts_the_range_asked_for() {
        // agy sends no result: StartLine/EndLine, or offset/limit, or its 800-line default
        assert_eq!(read_range(&json!({"file_path": "x", "StartLine": 10, "EndLine": 40}), None), Some((10, 40)));
        assert_eq!(read_range(&json!({"file_path": "x", "offset": 100, "limit": 50}), None), Some((100, 149)));
        assert_eq!(read_range(&json!({"file_path": "x"}), None), Some((1, AGY_VIEW_DEFAULT)));
    }
    #[test]
    fn read_code_first_hook_new_files_written_files_and_paths_outside_werk_pass() {
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        let new = f.replace("lib.rs", "new.rs");
        assert!(denied(&decide(&input("Write", "s1", json!({"file_path": new, "content": "x"})), &l, policy())).is_none());
        l.record_authored("s1", &f);
        assert!(denied(&decide(&input("Edit", "s1", body_edit(&f)), &l, policy())).is_none());
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
    fn read_code_first_hook_multiedit_is_gated_like_edit() {
        // Wren D: MultiEdit was recorded as authored but never checked
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        let me = json!({"file_path": f, "edits": [{"old_string": "    x\n", "new_string": "    x + 1\n"}]});
        assert!(denied(&decide(&input("MultiEdit", "s1", me.clone()), &l, policy())).is_some(), "an unread MultiEdit must be refused");
        l.record_read("s1", &f, 1, 5);
        assert!(denied(&decide(&input("MultiEdit", "s1", me), &l, policy())).is_none());
    }
    #[test]
    fn read_code_first_lsp_ast_negative_a_multiedit_that_renames_is_checked() {
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        l.record_read("s1", &f, 1, 5);
        let me = json!({"file_path": f, "edits": [{"old_string": "    x\n", "new_string": "    x + 1\n"}, {"old_string": "fn b()", "new_string": "fn c()"}]});
        let msg = denied(&decide(&input("MultiEdit", "s1", me), &l, policy())).expect("a renaming MultiEdit must be checked");
        assert!(msg.contains("definition of b "), "{msg}");
    }
    #[test]
    fn read_code_first_lsp_ast_a_patch_write_is_applied_as_an_edit() {
        // codex / opencode: apply_patch becomes a Write with operation=update and only the hunk
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        l.record_read("s1", &f, 1, 5);
        let w = json!({"file_path": f, "operation": "update", "content": "    x + 1\n", "old_string": "    x\n", "new_string": "    x + 1\n"});
        assert!(denied(&decide(&input("Write", "s1", w), &l, policy())).is_none(), "a body hunk is not a rename of everything");
    }
    #[test]
    fn read_code_first_hook_no_session_id_is_not_checked() {
        let (_d, f) = werk_file(SRC);
        let mut i = input("Edit", "", body_edit(&f));
        assert!(denied(&decide(&i, &Ledger::default(), policy())).is_none());
        i.session_id = None;
        assert!(denied(&decide(&i, &Ledger::default(), policy())).is_none());
    }
    #[test]
    fn read_code_first_hook_reads_survive_a_daemon_restart() {
        // Silas 11-24: the land restarts chorus-hooks; a session must keep the reads it made
        let (_d, f) = werk_file(SRC);
        let dir = tempfile::tempdir().unwrap();
        let before = Ledger::persistent(dir.path().to_path_buf());
        before.record_read("sess-a", &f, 1, 5);
        before.record_lookup("sess-a", "a");
        let after = Ledger::persistent(dir.path().to_path_buf()); // the restarted daemon
        let edit = json!({"file_path": f, "old_string": "fn a(x: u8) -> u8 {", "new_string": "fn a(x: u16) -> u8 {"});
        let r = decide(&input("Edit", "sess-a", edit.clone()), &after, policy());
        assert!(denied(&r).is_none(), "{r:?} {:?}", std::fs::read_to_string(dir.path().join("sess-a.jsonl")));
        // NEGATIVE: a ledger with no file (the old in-memory one) forgets on restart
        let forgot = Ledger::default();
        assert!(denied(&decide(&input("Edit", "sess-a", edit), &forgot, policy())).is_some());
        // a session id that is not a safe file name is never written to disk
        before.record_read("../x", &f, 1, 5);
        assert!(!dir.path().join("../x.jsonl").exists());
    }

    /// Wren C and K: Abby's and Borg Queen's real agy payloads (tests/fixtures/agy-4444),
    /// read through the AgentRuntime row and normalized the way the runtime hook does,
    /// are recorded and gated like Claude Code's.
    #[test]
    fn read_code_first_hook_agy_payloads_record_reads_and_are_gated() {
        let (_d, f) = werk_file(SRC);
        let table = chorus_hooks::runtime_rows::table();
        let row = chorus_hooks::runtime_rows::row_in(&table, "agy").expect("agy row");
        let through = |fixture: &str, moment| -> HookInput {
            let mut raw: Value = serde_json::from_str(fixture).unwrap();
            let args = &mut raw["toolCall"]["args"];
            for k in ["AbsolutePath", "TargetFile"] { if args.get(k).is_some() { args[k] = json!(f); } }
            if args.get("StartLine").is_some() { args["StartLine"] = json!(1); args["EndLine"] = json!(5); }
            let canon = chorus_hooks::runtime_rows::canonical(row, moment, &raw).unwrap();
            let op = chorus_hooks::runtime_tools::normalize_tools("external", canon["tool_name"].as_str().unwrap(), &canon["tool_input"], "/tmp").unwrap().remove(0);
            serde_json::from_value(json!({"tool_name": op.tool, "tool_input": op.input, "session_id": "agy-sess"})).unwrap()
        };
        let edit = through(include_str!("../../tests/fixtures/agy-4444/pre-tool-use-edit.json"), chorus_hooks::runtime_rows::Moment::BeforeTool);
        let l = Ledger::default();
        assert_eq!(edit.tool_name_str(), "Edit");
        assert!(denied(&decide(&edit, &l, policy())).is_some(), "an agy edit of an unread werk file must be refused");
        let view = through(include_str!("../../tests/fixtures/agy-4444/pre-tool-use-view.json"), chorus_hooks::runtime_rows::Moment::AfterTool);
        assert_eq!(view.tool_name_str(), "Read");
        record_result(&view, &l);
        let r = decide(&edit, &l, policy());
        // read in full now; what is left is the rule's other half or nothing
        assert!(denied(&r).is_none_or(|m| !m.contains("not been read")), "{r:?}");
    }

    // ---- AC3: the rule comes from the row ----
    #[test]
    fn read_code_first_reads_policy_the_refusal_carries_the_rows_words() {
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        let other = || Ok(Policy { label: "L2".into(), comment: "A DIFFERENT ROW SENTENCE".into() });
        let msg = denied(&decide(&input("Edit", "s1", body_edit(&f)), &l, other)).unwrap();
        assert!(msg.contains("A DIFFERENT ROW SENTENCE") && msg.contains("L2") && !msg.contains("ROW-TEXT"), "{msg}");
    }
    #[test]
    fn read_code_first_reads_policy_negative_an_absent_row_refuses_and_says_so() {
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        l.record_read("s1", &f, 1, 5); // even a fully read file: no row, no pass
        let msg = denied(&decide(&input("Edit", "s1", body_edit(&f)), &l, || Err("404".into())))
            .expect("an unreadable policy must refuse, never pass");
        assert!(msg.contains("could not be read") && msg.contains("404"), "{msg}");
        assert!(policy_from_body(&json!({"data": {"label": "x", "comment": ""}})).is_err());
    }
    /// A one-shot HTTP server that answers every request with `status` and `body`.
    fn door_answering(status: &str, body: &str) -> String {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let reply = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            for s in l.incoming().take(4).flatten() {
                let mut s = s;
                let mut buf = [0u8; 2048];
                let _ = s.read(&mut buf);
                let _ = s.write_all(reply.as_bytes());
            }
        });
        format!("http://{addr}")
    }
    #[test]
    fn read_code_first_reads_policy_a_door_blip_uses_the_last_good_row_and_a_404_refuses() {
        // Wren E: a door that does not answer must not lock every role out
        let dir = tempfile::tempdir().unwrap();
        let keep = dir.path().join(POLICY_FILE);
        let good = door_answering("200 OK", r#"{"data":{"label":"Read the code first","comment":"rule text"}}"#);
        assert_eq!(fetch_policy(&good, Some(&keep)).unwrap().comment, "rule text");
        // the door goes away: an unreachable address, a fresh process (no memory cache for it)
        let p = fetch_policy("http://127.0.0.1:1", Some(&keep)).expect("a blip uses the last good row");
        assert_eq!(p.comment, "rule text");
        // NEGATIVE: never read, never served: refuse
        let empty = tempfile::tempdir().unwrap();
        assert!(fetch_policy("http://127.0.0.1:1", Some(&empty.path().join(POLICY_FILE))).is_err());
        // NEGATIVE: the door answers that the row is gone: refuse, the last good row does not apply
        let gone = door_answering("404 Not Found", r#"{"error":"not found"}"#);
        assert!(fetch_policy(&gone, Some(&keep)).unwrap_err().contains("404"));
        // a door that errors (5xx) is failing, not answering: the last good row stands
        let sick = door_answering("503 Service Unavailable", "{}");
        assert_eq!(fetch_policy(&sick, Some(&keep)).unwrap().comment, "rule text");
    }
    #[test]
    fn read_code_first_reads_policy_a_down_door_is_not_asked_again_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let keep = dir.path().join(POLICY_FILE);
        std::fs::write(&keep, r#"{"label":"L","comment":"kept"}"#).unwrap();
        let dead = "http://127.0.0.1:9"; // discard port: nothing answers
        assert_eq!(fetch_policy(dead, Some(&keep)).unwrap().comment, "kept");
        let t = Instant::now();
        assert_eq!(fetch_policy(dead, Some(&keep)).unwrap().comment, "kept");
        assert!(t.elapsed() < Duration::from_millis(200), "the second call must not wait on the door");
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
        // a file with no extension and no #! line has no definitions to compare
        assert_eq!(lang("/w/Makefile", "all:\n"), "");
        assert_eq!(lang("/w/run-finished", "#!/usr/bin/env bash\n"), "sh");
        assert_eq!(lang("/w/land-5027-rows", "#!/usr/bin/env python3\n"), "py");
        assert_eq!(lang("/w/x", "#!/bin/sh\n"), "sh");
        assert_eq!(lang("/w/x", "#!/usr/bin/env fish-shell-thing\n"), "");
        // a const holding a parenthesised value is not a function (code review, run 4)
        assert!(changed_symbols("const total = (a + b);\n", "const total = (a + b + c);\n", "ts").is_empty());
    }
    #[test]
    fn read_code_first_lsp_ast_renames_and_parameter_changes_are_found() {
        assert_eq!(changed_symbols("pub fn load(p: &str) {", "pub fn load(p: &str, strict: bool) {", "rs"), ["load"]);
        assert_eq!(changed_symbols("fn old_name() {}", "fn new_name() {}", "rs"), ["old_name"]);
        assert_eq!(changed_symbols("pub struct Row {", "pub struct Line {", "rs"), ["Row"]);
        assert_eq!(changed_symbols("export function go(a) {", "export function go(a, b) {", "ts"), ["go"]);
        assert_eq!(changed_symbols("def run(x):\n", "def run(x, y):\n", "py"), ["run"]);
        assert_eq!(changed_symbols("handler() {\n", "on_exit() {\n", "sh"), ["handler"]);
        // an arrow function's parameters (review note, run 1)
        assert_eq!(changed_symbols("const go = (a) => {", "const go = (a, b) => {", "ts"), ["go"]);
        assert_eq!(changed_symbols("export const go = async (a) => {", "export const go = async (a, b) => {", "ts"), ["go"]);
        // two same-named definitions: the second one's change is not hidden by the first
        let old = "impl A {\n    fn new() {}\n}\nimpl B {\n    fn new(x: u8) {}\n}\n";
        assert_eq!(changed_symbols(old, &old.replace("fn new(x: u8)", "fn new(x: u16)"), "rs"), ["new"]);
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
        record_lookup(&input("mcp__cclsp__find_references", "s1", json!({"symbol_name": "ab", "file_path": "/w/a/x.rs"})), &l);
        assert!(denied(&decide(&input("Edit", "s1", edit.clone()), &l, policy())).is_some());
        // the lookup of `a` (here ast-grep on the shell) lets it through
        record_lookup(&input("Bash", "s1", json!({"command": "ast-grep -p 'a($$$)' src"})), &l);
        assert!(denied(&decide(&input("Edit", "s1", edit), &l, policy())).is_none());
    }
    #[test]
    fn read_code_first_lsp_ast_negative_a_narrow_edit_inside_the_parameters_is_seen() {
        // Wren A: "x: u8" -> "x: u16" holds no `fn`; applied to the file, it changes a()
        let (_d, f) = werk_file(SRC);
        let l = Ledger::default();
        l.record_read("s1", &f, 1, 5);
        let edit = json!({"file_path": f, "old_string": "x: u8)", "new_string": "x: u16)"});
        let msg = denied(&decide(&input("Edit", "s1", edit), &l, policy())).expect("a narrow parameter edit must be seen");
        assert!(msg.contains("definition of a "), "{msg}");
    }
    #[test]
    fn read_code_first_lsp_ast_negative_a_lookup_counts_only_by_its_symbol_or_pattern() {
        // Wren I: a path or an unrelated word in the call does not name the symbol
        assert_eq!(lookup_text("mcp__cclsp__find_references", &json!({"file_path": "/run/x.rs", "symbol_name": "load"})), "load");
        assert_eq!(lookup_text("Bash", &json!({"command": "echo ast-grep load"})), "");
        assert_eq!(lookup_text("Bash", &json!({"command": "cd src && ast-grep -p 'load($$$)' platform/x"})), "-p 'load($$$)'");
        assert_eq!(lookup_text("Read", &json!({"file_path": "/a/load.rs"})), "");
        // an ast-grep MCP call counts by its pattern; a shell one keeps --pattern= values
        assert_eq!(lookup_text("mcp__ast-grep__find_code", &json!({"pattern": "load($$$)", "project_folder": "/w"})), "load($$$)");
        assert!(lookup_text("Bash", &json!({"command": "ast-grep run --pattern=load src"})).contains("--pattern=load"));
        assert!(lookup_text("Bash", &json!({"command": "ls\nast-grep -p 'load($A); x' ."})).contains("load($A); x"));
        let l = Ledger::default();
        record_lookup(&input("mcp__cclsp__find_references", "s1", json!({"file_path": "/run/x.rs", "symbol_name": "load"})), &l);
        assert!(!l.looked_up("s1", "run") && l.looked_up("s1", "load"));
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
