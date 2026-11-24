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

/// A restore a session owes: (werk root, path in the werk, the commit its call started from).
type Debt = (String, String, String);

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
    /// #5033 AC4 — each running Bash call's werks as they were when it started, by call id.
    snaps: Mutex<HashMap<String, Vec<WerkSnap>>>,
    /// #5033 AC4 — files a session changed through the shell without reading them whole
    /// first: (werk root, path in the werk, the commit the call started from). Cleared only
    /// by restoring the file to its content at that commit.
    debts: Mutex<HashMap<String, Vec<Debt>>>,
}

/// One line of a session's ledger file.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "k", rename_all = "lowercase")]
enum Entry {
    Read { path: String, a: usize, b: usize, #[serde(default)] stamp: Option<Stamp> },
    Wrote { path: String },
    Lookup { text: String },
    Debt { root: String, rel: String, base: String },
    Cleared { root: String, rel: String },
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
                Entry::Debt { root, rel, base } => self.add_debt(sid, &root, &rel, &base),
                Entry::Cleared { root, rel } => self.drop_debt(sid, &root, &rel),
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
    fn add_debt(&self, sid: &str, root: &str, rel: &str, base: &str) {
        if let Ok(mut m) = self.debts.lock() {
            let v = m.entry(sid.to_string()).or_default();
            // the first base stands: a later blind change of the same file owes the same restore
            if !v.iter().any(|(r, f, _)| r == root && f == rel) { v.push((root.to_string(), rel.to_string(), base.to_string())); }
        }
    }
    fn drop_debt(&self, sid: &str, root: &str, rel: &str) {
        if let Ok(mut m) = self.debts.lock() {
            if let Some(v) = m.get_mut(sid) { v.retain(|(r, f, _)| !(r == root && f == rel)); }
        }
    }
    fn record_debt(&self, sid: &str, root: &str, rel: &str, base: &str) {
        self.load(sid);
        self.add_debt(sid, root, rel, base);
        self.persist(sid, &Entry::Debt { root: root.to_string(), rel: rel.to_string(), base: base.to_string() });
    }
    fn clear_debt(&self, sid: &str, root: &str, rel: &str) {
        self.drop_debt(sid, root, rel);
        self.persist(sid, &Entry::Cleared { root: root.to_string(), rel: rel.to_string() });
    }
    fn session_debts(&self, sid: &str) -> Vec<Debt> {
        self.load(sid);
        self.debts.lock().ok().and_then(|m| m.get(sid).cloned()).unwrap_or_default()
    }
    /// Files under `root` this session has read whole as they are now, or written itself.
    fn known_under(&self, sid: &str, root: &str) -> HashSet<String> {
        self.load(sid);
        let prefix = format!("{root}/");
        let mut out: HashSet<String> = self.authored.lock().map(|s| s.iter()
            .filter(|(id, p)| id == sid && p.starts_with(&prefix)).map(|(_, p)| p.clone()).collect()).unwrap_or_default();
        let read: Vec<String> = self.reads.lock().map(|m| m.keys()
            .filter(|(id, p)| id == sid && p.starts_with(&prefix)).map(|(_, p)| p.clone()).collect()).unwrap_or_default();
        for p in read {
            let lines = std::fs::read_to_string(&p).map(|t| t.lines().count()).unwrap_or(usize::MAX);
            if first_unread(&self.ranges(sid, &p), lines).is_none() { out.insert(p); }
        }
        out
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

// ---- #5033 AC4: the rule through the shell ----
//
// Jeff 11-24 09:45: a rule a role can walk around through the shell is not finished. The
// daemon looks at the werks a Bash call can reach before it runs and after it ends. A
// tracked file whose content the call changed, that the session had not read whole (or
// written itself) before the call started, is a debt: every next Edit, Write, shell
// command or commit (werk-commit, werk-push, chorus-werk) of that session is refused,
// naming the file and its restore command, until the file is restored to its content at
// the commit the call started from. Restoring is the only way out (Wren 09:52): reading
// afterwards would let every blind change stand, and committing it does not clear it
// either (Wren 10:15 A: the debt keeps its base commit). Never a trap: Read, Grep, the
// reply script, read-only git and the restore commands themselves always pass.
//
// The daemon runs as jeffbridwell. A werk's root and .git are his and its files are group
// chorus, so `git -c safe.directory=<root>` reads any role's werk from here, and the ledger
// is already here (Wren 10:15 H; objection 1 had said the shim, before this was measured).
//
// Not changes of the session: commits that came in during the call (a pull, a rebase, a
// merge: their committer time is before the call, or HEAD left the line it was on); a file
// whose header at the base commit says GENERATED (its generator owns it); lock files a
// build rewrites (Cargo.lock, package-lock.json, yarn.lock, go.sum: Wren 10:15 E); untracked
// new files. Commits the call itself made count, with the files they touched (Wren D).
// Kept limits: writes by a background job, a subagent or a pipeline step land on whichever
// call of the session is running when they happen; a werk reached only through a variable
// path ($W/x) is not scanned; a chmod is a change (git sees the mode); each werk-naming call
// pays two git status calls (0.11 s each, measured); Abby's and Borg Queen's restore path
// through their runtime is checked after the land (After the land, Wren).

/// One werk as a Bash call found it: its HEAD, its dirty tracked files with their stamps,
/// the files the session knew (read whole, or wrote) at that moment, and when.
#[derive(Clone, Debug)]
pub struct WerkSnap { root: String, head: String, dirty: HashMap<String, Option<Stamp>>, known: HashSet<String>, at: u64 }

fn git(root: &str, args: &[&str]) -> Option<std::process::Output> {
    std::process::Command::new("git").arg("-c").arg(format!("safe.directory={root}")).arg("-C").arg(root)
        .args(args).stdin(std::process::Stdio::null()).output().ok()
}
fn git_out(root: &str, args: &[&str]) -> Option<String> {
    git(root, args).filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
}

/// `…/chorus-werk/<slot>` for a path inside a werk.
fn werk_root(path: &str) -> Option<String> {
    let (base, rest) = path.split_once("/chorus-werk/")?;
    let slot = rest.split('/').next().filter(|s| !s.is_empty())?;
    Some(format!("{base}/chorus-werk/{slot}"))
}

/// Where card werks live (CHORUS_WERK_BASE, as canonical_write_guard reads it).
fn werk_base() -> String {
    std::env::var("CHORUS_WERK_BASE").ok().filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/Users/jeffbridwell/CascadeProjects/chorus-werk".to_string())
}

/// The werks a shell command can write: the one its cwd is in, every werk its text names
/// however it is spelled (an absolute path, `~/…/chorus-werk/<slot>`, `$HOME/…`, a relative
/// `chorus-werk/<slot>` or `../chorus-werk/<slot>`: each is resolved to the werk base), and,
/// from inside a werk, a sibling slot named `../<role>-<card>` (Wren 10:15 W1). Only real
/// worktrees (a .git entry at the root).
pub fn werks_reached(cwd: &str, command: &str) -> Vec<String> {
    werks_reached_in(cwd, command, &werk_base())
}

pub fn werks_reached_in(cwd: &str, command: &str, base: &str) -> Vec<String> {
    static NAMED: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static SIBLING: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let named = NAMED.get_or_init(|| regex::Regex::new(r"((?:/[^\s'\x22;&|()<>]*)?)chorus-werk/([A-Za-z0-9_.-]+)").unwrap());
    let sibling = SIBLING.get_or_init(|| regex::Regex::new(r"(?:^|[\s'\x22=:(])\.\./([a-z][a-z0-9-]*-[0-9]+)(?:/|\s|$|['\x22])").unwrap());
    let here = werk_root(cwd);
    let mut out: Vec<String> = here.iter().cloned().collect();
    for c in named.captures_iter(command) {
        let (prefix, slot) = (c.get(1).map_or("", |m| m.as_str()), &c[2]);
        if prefix.starts_with('/') { out.push(format!("{prefix}chorus-werk/{slot}")); }
        out.push(format!("{base}/{slot}"));
    }
    if let Some(h) = &here {
        let parent = h.rsplit_once('/').map_or("", |(p, _)| p);
        out.extend(sibling.captures_iter(command).map(|c| format!("{parent}/{}", &c[1])));
    }
    out.sort();
    out.dedup();
    out.retain(|r| Path::new(r).join(".git").exists());
    out
}

/// Dirty tracked files of a werk (path in the werk -> stamp; None when deleted), or None
/// when git cannot answer.
fn dirty(root: &str) -> Option<HashMap<String, Option<Stamp>>> {
    let text = git_out(root, &["status", "--porcelain=v1", "-z", "--untracked-files=no"])?;
    let mut out = HashMap::new();
    let mut parts = text.split('\0').filter(|p| !p.is_empty());
    while let Some(entry) = parts.next() {
        if entry.len() < 4 { continue; }
        let (code, rel) = (&entry[..2], &entry[3..]);
        if code.contains('R') || code.contains('C') { let _ = parts.next(); } // the rename's source
        out.insert(rel.to_string(), stamp(&format!("{root}/{rel}")));
    }
    Some(out)
}

fn head(root: &str) -> String {
    git_out(root, &["rev-parse", "HEAD"]).map(|s| s.trim().to_string()).unwrap_or_default()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The files the call's own commits touched: commits between the old and the new HEAD made
/// at or after the call started. None when HEAD left the line it was on (a rebase, a reset,
/// a checkout): those rewrite the tree by design and are not a blind edit.
fn own_commit_files(root: &str, old: &str, new: &str, since: u64) -> Option<HashSet<String>> {
    if old == new { return Some(HashSet::new()); }
    git(root, &["merge-base", "--is-ancestor", old, new]).filter(|o| o.status.success())?;
    let log = git_out(root, &["log", "--format=%ct %H", &format!("{old}..{new}")])?;
    let mut files = HashSet::new();
    for (ct, sha) in log.lines().filter_map(|l| l.split_once(' ')) {
        if ct.parse::<u64>().unwrap_or(0) + 1 < since { continue; } // came in from elsewhere (a pull)
        for f in git_out(root, &["diff-tree", "--no-commit-id", "--name-only", "-r", "--root", sha]).unwrap_or_default().lines() {
            files.insert(f.to_string());
        }
    }
    Some(files)
}

/// A file its tools own: a GENERATED header at the base commit (read there, never from the
/// changed file: Wren 10:15 C), or a lock file a build rewrites.
fn tool_owned(root: &str, base: &str, rel: &str) -> bool {
    const LOCKS: &[&str] = &["Cargo.lock", "package-lock.json", "yarn.lock", "pnpm-lock.yaml", "go.sum", "poetry.lock"];
    if LOCKS.contains(&rel.rsplit('/').next().unwrap_or(rel)) { return true; }
    git_out(root, &["show", &format!("{base}:{rel}")])
        .is_some_and(|t| t.lines().take(10).any(|l| l.contains("GENERATED")))
}

/// The file is as it was at the base commit: content, or absence (a file the call deleted,
/// a file that did not exist there).
fn restored(root: &str, rel: &str, base: &str) -> bool {
    git(root, &["diff", "--quiet", base, "--", rel]).is_some_and(|o| o.status.success())
}

fn call_id(input: &HookInput) -> String {
    input.tool_use_id.clone().unwrap_or_else(|| input.session_id.clone().unwrap_or_default())
}

/// PreToolUse of an allowed Bash call: snapshot the werks it can reach.
pub fn shell_pre(input: &HookInput, ledger: &Ledger) {
    let sid = input.session_id.as_deref().unwrap_or("");
    if input.tool_name_str() != "Bash" || sid.is_empty() { return; }
    let roots = werks_reached(input.cwd.as_deref().unwrap_or(""), &input.get_tool_input_str("command"));
    let at = now_secs();
    let snaps: Vec<WerkSnap> = roots.into_iter().filter_map(|root| Some(WerkSnap {
        head: head(&root), dirty: dirty(&root)?, known: ledger.known_under(sid, &root), root, at,
    })).collect();
    if let Ok(mut m) = ledger.snaps.lock() {
        // a call that never reached PostToolUse leaves its snapshot: drop those after an hour
        m.retain(|_, v| v.first().is_none_or(|s| at.saturating_sub(s.at) < 3600));
        m.insert(call_id(input), snaps);
    }
}

/// PostToolUse of a Bash call: every tracked file it changed (in the tree, or in a commit
/// it made) that the session did not know becomes a debt against the commit the call
/// started from; a known file it changed counts as the session's own write.
pub fn shell_post(input: &HookInput, ledger: &Ledger) -> Vec<String> {
    let sid = input.session_id.as_deref().unwrap_or("");
    if input.tool_name_str() != "Bash" || sid.is_empty() { return Vec::new(); }
    let Some(snaps) = ledger.snaps.lock().ok().and_then(|mut m| m.remove(&call_id(input))) else { return Vec::new() };
    let mut owed = Vec::new();
    for snap in snaps {
        let Some(committed) = own_commit_files(&snap.root, &snap.head, &head(&snap.root), snap.at) else { continue };
        let Some(now) = dirty(&snap.root) else { continue };
        let mut changed: Vec<String> = now.iter()
            .filter(|(rel, st)| snap.dirty.get(*rel) != Some(*st)) // dirty before and untouched: not this call
            .map(|(rel, _)| rel.clone()).collect();
        // a file a commit of this call took: changed by the call unless it sat dirty, untouched
        changed.extend(committed.into_iter().filter(|rel| snap.dirty.get(rel) != Some(&stamp(&format!("{}/{rel}", snap.root)))));
        changed.sort();
        changed.dedup();
        for rel in changed {
            let full = format!("{}/{rel}", snap.root);
            if snap.known.contains(&full) || ledger.wrote(sid, &full) {
                ledger.record_authored(sid, &full);
            } else if !tool_owned(&snap.root, &snap.head, &rel) && !restored(&snap.root, &rel, &snap.head) {
                ledger.record_debt(sid, &snap.root, &rel, &snap.head);
                owed.push(full);
            }
        }
    }
    owed
}

/// Shell commands that never change a werk file, restore one, or are the reply script.
fn shell_passes(command: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(concat!(
        r"^\s*(?:cd\s+\S+\s*&&\s*)?(?:",
        r"git(?:\s+-C\s+\S+)?\s+(?:",
        r"restore\b[^;&|>]*|checkout\s+(?:\S+\s+)?--[^;&|>]*",                            // restore
        r"|show\s+\S+:\S+\s*>\s*\S+",                                                   // git show REV:f > f
        r"|(?:status|diff|log|show)\b[^;&|>]*)",                                         // read-only
        // the reply script as the command itself, its arguments quoted (Wren 10:15 F)
        r#"|\S*chorus-mcp-call\.sh\s+[a-z][a-z0-9-]*\s+clearing-nudge\s+'[^']*'"#,
        r")\s*$")).unwrap());
    re.is_match(command)
}

/// The tools a debt refuses: every way to change a werk file or to commit one.
fn debt_governs(tool: &str) -> bool {
    matches!(tool, "Edit" | "Write" | "MultiEdit" | "Bash"
        | "mcp__chorus-api__werk-commit" | "mcp__chorus-api__werk-push" | "mcp__chorus-api__chorus-werk")
}

/// The debt gate, ahead of the rest: a session that owes a restore is refused, naming
/// each file and its restore command.
pub fn shell_debt(input: &HookInput, ledger: &Ledger, policy: impl FnOnce() -> Result<Policy, String>) -> Option<HookResponse> {
    let tool = input.tool_name_str();
    if !debt_governs(tool) { return None; }
    let sid = input.session_id.as_deref().filter(|s| !s.is_empty())?;
    let mut owed = Vec::new();
    for (root, rel, base) in ledger.session_debts(sid) {
        // a werk that is gone (accepted, unpulled, removed) took the change with it: nothing
        // is left to restore, so the debt is settled, never a trap (Wren 10:15 T1)
        let gone = !Path::new(&root).join(".git").exists();
        if gone || restored(&root, &rel, &base) { ledger.clear_debt(sid, &root, &rel); } else { owed.push((root, rel, base)); }
    }
    if owed.is_empty() || (tool == "Bash" && shell_passes(&input.get_tool_input_str("command"))) { return None; }
    let p = policy().unwrap_or(Policy { label: "Read the code first".into(), comment: format!("(the row {POLICY} could not be read)") });
    let list = owed.iter().map(|(r, f, b)| format!("{r}/{f} (restore: git -C {r} restore --source={b} -- {f})"))
        .collect::<Vec<_>>().join("; ");
    Some(deny(&p, format!(
        "a shell command changed {list} without this session reading it whole first. Nothing else \
         runs, commits included, until each file is restored with its restore command; then read it \
         whole and make the change with Edit. Read, Grep and read-only git still work.")))
}

/// The gate the PreToolUse dispatch calls: the row comes from the door.
pub fn check(input: &HookInput, ledger: &Ledger) -> HookResponse {
    let last_good = ledger.dir.as_ref().map(|d| d.join(POLICY_FILE));
    let row = || fetch_policy(&door(), last_good.as_deref());
    if let Some(r) = shell_debt(input, ledger, row) { return r; }
    decide(input, ledger, row)
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

    // ---- #5033 AC4: the shell ----
    /// A real worktree-shaped werk: `<tmp>/chorus-werk/kade-1`, one commit of a.rs (5 lines)
    /// and gen.yaml (GENERATED header).
    fn werk_repo() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("chorus-werk/kade-1");
        std::fs::create_dir_all(&root).unwrap();
        let root = root.to_string_lossy().into_owned();
        std::fs::write(format!("{root}/a.rs"), SRC).unwrap();
        std::fs::write(format!("{root}/gen.yaml"), "# GENERATED by chorus-make\nk: v\n").unwrap();
        std::fs::write(format!("{root}/Cargo.lock"), "version = 3\n").unwrap();
        sh(&root, "git init -q && git -c user.email=t@t -c user.name=t add -A && git -c user.email=t@t -c user.name=t commit -qm base");
        (dir, root)
    }
    fn sh(root: &str, cmd: &str) {
        let o = std::process::Command::new("/bin/sh").arg("-c").arg(cmd).current_dir(root).output().unwrap();
        assert!(o.status.success(), "{cmd}: {}", String::from_utf8_lossy(&o.stderr));
    }
    fn bash(sid: &str, cwd: &str, cmd: &str, id: &str) -> HookInput {
        HookInput { tool_name: Some("Bash".into()), tool_input: Some(json!({"command": cmd})), session_id: Some(sid.into()),
            cwd: Some(cwd.into()), tool_use_id: Some(id.into()), ..Default::default() }
    }
    /// pre snapshot, the command, post scan: what the session now owes
    fn through_shell(l: &Ledger, root: &str, cmd: &str) -> Vec<String> {
        let i = bash("s1", root, cmd, &format!("call-{}", uuid::Uuid::now_v7()));
        shell_pre(&i, l);
        sh(root, cmd);
        shell_post(&i, l)
    }

    #[test]
    fn read_code_first_shell_negative_sed_on_an_unread_file_refuses_the_next_edit_and_shell_until_restored() {
        let (_d, root) = werk_repo();
        let l = Ledger::default();
        let owed = through_shell(&l, &root, "sed -i '' 's/x/y/' a.rs");
        assert_eq!(owed, [format!("{root}/a.rs")]);
        let edit = input("Edit", "s1", json!({"file_path": format!("{root}/gen.yaml"), "old_string": "v", "new_string": "w"}));
        let msg = shell_debt(&edit, &l, policy()).and_then(|r| denied(&r)).expect("the next edit must be refused");
        assert!(msg.contains(&format!("{root}/a.rs")) && msg.contains(&format!("git -C {root} restore --source=")) && msg.contains("ROW-TEXT"), "{msg}");
        assert!(shell_debt(&bash("s1", &root, "ls", "c2"), &l, policy()).is_some(), "the next shell command must be refused");
        // never a trap: the restore, read-only git and the reply path pass
        for ok in [format!("git -C {root} restore -- a.rs"), "git diff a.rs".into(), "git status".into(), "chorus-mcp-call.sh kade clearing-nudge '{}'".into()] {
            assert!(shell_debt(&bash("s1", &root, &ok, "c3"), &l, policy()).is_none(), "{ok} must pass");
        }
        // another session owes nothing
        assert!(shell_debt(&input("Edit", "s2", json!({"file_path": format!("{root}/a.rs")})), &l, policy()).is_none());
        sh(&root, "git restore -- a.rs");
        assert!(shell_debt(&edit, &l, policy()).is_none(), "after the restore the edit passes");
        assert!(shell_debt(&bash("s1", &root, "ls", "c4"), &l, policy()).is_none(), "and so does the shell");
    }
    #[test]
    fn read_code_first_shell_negative_every_way_of_writing_is_seen() {
        for cmd in [
            "cat > a.rs <<'EOF'\nfn z() {}\nEOF",
            "python3 -c \"open('a.rs','w').write('fn z() {}\\n')\"",
            "printf 'fn z() {}\\n' > ../new.rs; git diff --no-index --src-prefix=a/ --dst-prefix=b/ a.rs ../new.rs | sed 's|b/../new.rs|b/a.rs|' > ../p.diff; git apply ../p.diff",
            "perl -pi -e 's/x/q/' a.rs",
            "rm a.rs",
        ] {
            let (_d, root) = werk_repo();
            let l = Ledger::default();
            let owed = through_shell(&l, &root, cmd);
            assert_eq!(owed, [format!("{root}/a.rs")], "not seen: {cmd}");
        }
    }
    #[test]
    fn read_code_first_shell_a_file_read_whole_first_or_written_by_the_session_is_never_refused() {
        let (_d, root) = werk_repo();
        let l = Ledger::default();
        l.record_read("s1", &format!("{root}/a.rs"), 1, 5);
        assert!(through_shell(&l, &root, "sed -i '' 's/x/y/' a.rs").is_empty());
        assert!(shell_debt(&bash("s1", &root, "ls", "c5"), &l, policy()).is_none());
        // the shell's change of a known file is the session's own write: later edits pass the read rule
        assert!(l.wrote("s1", &format!("{root}/a.rs")));
        // NEGATIVE: a partial read is not whole
        let (_d2, root2) = werk_repo();
        let l2 = Ledger::default();
        l2.record_read("s1", &format!("{root2}/a.rs"), 1, 4);
        assert_eq!(through_shell(&l2, &root2, "sed -i '' 's/x/y/' a.rs").len(), 1);
    }
    #[test]
    fn read_code_first_shell_generated_files_moved_heads_new_files_and_already_dirty_files_pass() {
        let (_d, root) = werk_repo();
        let l = Ledger::default();
        assert!(through_shell(&l, &root, "printf 'k: w\\n' >> gen.yaml").is_empty(), "a GENERATED file is its generator's");
        assert!(through_shell(&l, &root, "printf 'new\\n' > brand-new.rs").is_empty(), "an untracked new file");
        assert!(through_shell(&l, &root, "printf 'x' >> Cargo.lock").is_empty(), "a lock file a build rewrites (Wren E)");
        // commits that came in from elsewhere (a pull): committed before the call started
        sh(&root, "git checkout -qb side && printf 'pulled\n' >> a.rs && GIT_COMMITTER_DATE='2020-01-01T00:00:00' git -c user.email=t@t -c user.name=t commit -qam pulled && git checkout -q -");
        assert!(through_shell(&l, &root, "git merge -q --ff-only side").is_empty(), "a fast-forward of older commits");
        // HEAD left its line (a checkout of another branch, a rebase): not a blind edit
        sh(&root, "git checkout -qb other HEAD~1 && printf 'o\n' >> a.rs && git -c user.email=t@t -c user.name=t commit -qam other && git checkout -q -");
        assert!(through_shell(&l, &root, "git checkout -q other").is_empty(), "HEAD left the line");
        sh(&root, "git checkout -q -");
        // dirty before the call and untouched by it: not this call's change
        sh(&root, "printf 'more\\n' >> a.rs");
        assert!(through_shell(&l, &root, "ls").is_empty());
    }
    #[test]
    fn read_code_first_shell_negative_a_blind_change_committed_in_the_same_call_is_seen() {
        // Wren 10:15 D: "sed -i ... && git commit -am x" moved HEAD and was exempted
        let (_d, root) = werk_repo();
        let l = Ledger::default();
        let owed = through_shell(&l, &root, "sed -i '' 's/x/y/' a.rs && git -c user.email=t@t -c user.name=t commit -qam blind");
        assert_eq!(owed, [format!("{root}/a.rs")]);
    }
    #[test]
    fn read_code_first_shell_negative_committing_the_change_does_not_clear_the_debt() {
        // Wren 10:15 A: a commit (werk-commit, /cw) makes the file equal HEAD; the debt keeps
        // its base commit, so it is still owed until the file is back to that content
        let (_d, root) = werk_repo();
        let l = Ledger::default();
        assert_eq!(through_shell(&l, &root, "sed -i '' 's/x/y/' a.rs").len(), 1);
        let base = head(&root);
        sh(&root, "git -c user.email=t@t -c user.name=t commit -qam 'committed by the pipeline'");
        let edit = input("Edit", "s1", json!({"file_path": format!("{root}/gen.yaml")}));
        let msg = shell_debt(&edit, &l, policy()).and_then(|r| denied(&r)).expect("a committed blind change is still owed");
        assert!(msg.contains(&format!("restore --source={base} -- a.rs")), "{msg}");
        let commit = input("mcp__chorus-api__werk-commit", "s1", json!({"card": 1}));
        assert!(shell_debt(&commit, &l, policy()).is_some(), "the commit tools are refused too");
        sh(&root, &format!("git restore --source={base} -- a.rs"));
        assert!(shell_debt(&edit, &l, policy()).is_none(), "back to the base content: cleared");
    }
    #[test]
    fn read_code_first_shell_a_debt_whose_werk_is_gone_is_settled_not_a_trap() {
        // Wren 10:15 T1: the werk was accepted or unpulled while the debt stood
        let (dir, root) = werk_repo();
        let l = Ledger::default();
        assert_eq!(through_shell(&l, &root, "sed -i '' 's/x/y/' a.rs").len(), 1);
        let ls = bash("s1", "/tmp", "ls", "c-gone");
        assert!(shell_debt(&ls, &l, policy()).is_some(), "owed while the werk stands");
        std::fs::remove_dir_all(dir.path().join("chorus-werk")).unwrap();
        assert!(shell_debt(&ls, &l, policy()).is_none(), "the werk is gone: settled");
        assert!(l.session_debts("s1").is_empty());
    }
    #[test]
    fn read_code_first_shell_negative_a_header_written_by_the_change_does_not_exempt_it() {
        // Wren 10:15 C: GENERATED is read from the base commit, not from the changed file
        let (_d, root) = werk_repo();
        let l = Ledger::default();
        let owed = through_shell(&l, &root, "printf '// GENERATED\nfn z() {}\n' > a.rs");
        assert_eq!(owed, [format!("{root}/a.rs")]);
    }
    #[test]
    fn read_code_first_shell_the_werk_is_found_from_the_command_and_the_debt_survives_a_restart() {
        let (_d, root) = werk_repo();
        assert_eq!(werks_reached("/tmp", &format!("cd {root} && sed -i '' x a.rs")), std::slice::from_ref(&root));
        assert_eq!(werks_reached(&format!("{root}/src"), "ls"), std::slice::from_ref(&root));
        assert!(werks_reached("/tmp", "ls /nowhere/chorus-werk/kade-9/x").is_empty(), "only real worktrees");
        // Wren 10:15 W1: every spelling of the same werk is found
        let base = root.rsplit_once('/').unwrap().0.to_string();
        for cmd in ["sed -i '' x ~/CascadeProjects/chorus-werk/kade-1/a.rs", "sed -i '' x $HOME/CascadeProjects/chorus-werk/kade-1/a.rs",
                    "cd chorus-werk/kade-1 && sed -i '' x a.rs", "sed -i '' x ../../chorus-werk/kade-1/a.rs"] {
            assert_eq!(werks_reached_in("/tmp", cmd, &base), std::slice::from_ref(&root), "not found: {cmd}");
        }
        let sib = format!("{base}/kade-2");
        std::fs::create_dir_all(&sib).unwrap();
        sh(&sib, "git init -q");
        assert!(werks_reached_in(&root, "sed -i '' x ../kade-2/a.rs", &base).contains(&sib), "a sibling slot from inside a werk");
        let dir = tempfile::tempdir().unwrap();
        let before = Ledger::persistent(dir.path().to_path_buf());
        let i = bash("sess-r", "/tmp", &format!("cd {root} && sed -i '' 's/x/y/' a.rs"), "c-r");
        shell_pre(&i, &before);
        sh(&root, "sed -i '' 's/x/y/' a.rs");
        assert_eq!(shell_post(&i, &before).len(), 1);
        let after = Ledger::persistent(dir.path().to_path_buf());
        assert!(shell_debt(&bash("sess-r", "/tmp", "ls", "c-r2"), &after, policy()).is_some(), "the debt survives a restart");
    }
    #[test]
    fn read_code_first_shell_only_restores_and_read_only_git_pass_the_gate() {
        for ok in ["git restore -- a.rs", "git -C /w restore a.rs", "git checkout -- a.rs", "git checkout HEAD -- a.rs",
                   "git show origin/main:a.rs > a.rs", "cd /w && git diff", "git log -1"] {
            assert!(shell_passes(ok), "{ok}");
        }
        // the reply script only as the command itself (Wren 10:15 F)
        for ok in ["chorus-mcp-call.sh kade clearing-nudge '{\"to\":\"wren\"}'", "/x/bin/chorus-mcp-call.sh wren clearing-nudge '{}'"] {
            assert!(shell_passes(ok), "{ok}");
        }
        for no in ["git restore -- a.rs; echo x > b.rs", "git diff > a.rs", "echo x > a.rs", "git commit -am x", "ls && sed -i '' x a.rs",
                   "sed -i '' x a.rs # chorus-mcp-call.sh", "chorus-mcp-call.sh kade clearing-nudge '{}'; rm a.rs"] {
            assert!(!shell_passes(no), "{no}");
        }
    }
}
