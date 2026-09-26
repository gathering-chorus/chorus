//! chorus-crawl — the one herald (#4173).
//!
//! Walks the repo, keeps a row per tracked file in the graph through the
//! generated door, and says what it did. Zero external crates: `git` and
//! `curl` as subprocesses, std for the rest.
//!
//! Every decision this binary makes lives in lib.rs as a pure function. main
//! is I/O and reporting only — so the rules are testable without a repo, a
//! server, or a clock.

use chorus_crawl::cases::{self, CaseAction, CaseInGraph, CaseRow};
use chorus_crawl::pages;
use chorus_crawl::domain;
use chorus_crawl::*;
use std::collections::HashMap;
use std::process::Command;

fn sh(cmd: &str, args: &[&str], cwd: &str) -> Result<String, String> {
    let out = Command::new(cmd)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("{cmd}: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "{cmd} {:?} exited {}: {}",
            args,
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Every tracked file, from git — not a directory walk. git already knows what
/// is in the repo and what is ignored; re-deriving it is how a walker ends up
/// indexing node_modules.
fn tracked_files(root: &str) -> Result<Vec<String>, String> {
    Ok(sh("git", &["ls-files"], root)?
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| l.to_string())
        .collect())
}

fn head_commit(root: &str) -> Result<String, String> {
    Ok(sh("git", &["rev-parse", "HEAD"], root)?.trim().to_string())
}

/// Is this commit in this clone? A watermark naming a commit we do not have
/// (rebase, force-push, shallow clone) must force a full walk, not a crash and
/// not a silent empty delta.
fn commit_is_reachable(root: &str, sha: &str) -> bool {
    sh(
        "git",
        &["cat-file", "-e", &format!("{sha}^{{commit}}")],
        root,
    )
    .is_ok()
}

fn changes_since(root: &str, from: &str, to: &str) -> Result<Vec<Change>, String> {
    let raw = sh(
        "git",
        &["diff", "--name-status", "-M", &format!("{from}..{to}")],
        root,
    )?;
    let mut out = Vec::new();
    for line in raw.lines().filter(|l| !l.is_empty()) {
        match parse_name_status(line) {
            Some(c) => out.push(c),
            // An unreadable status is not skipped quietly: the caller widens to
            // a full walk rather than lose a file nobody knows about.
            None => return Err(format!("unreadable git status line: {line}")),
        }
    }
    Ok(out)
}

/// Content hashes for every tracked file, in ONE git call.
///
/// The first cut shelled to `shasum` per file: 6,175 processes, 2m02s wall and
/// 78% of a core to compute something git already stores. `git ls-files -s`
/// prints `<mode> <blob-sha> <stage>\t<path>` for the whole index at once.
/// The blob sha IS the content hash, and using git's own is both faster and
/// more honest — it is the hash the rest of the system already agrees on.
fn tree_hashes(root: &str) -> Result<HashMap<String, String>, String> {
    let raw = sh("git", &["ls-files", "-s"], root)?;
    let mut out = HashMap::new();
    for line in raw.lines().filter(|l| !l.is_empty()) {
        let (meta, path) = match line.split_once('\t') {
            Some(p) => p,
            None => continue,
        };
        if let Some(sha) = meta.split_whitespace().nth(1) {
            out.insert(path.to_string(), sha.to_string());
        }
    }
    Ok(out)
}

fn rust_declares_tests(abs: &str) -> bool {
    std::fs::read_to_string(abs)
        .map(|s| s.contains("#[test]") || s.contains("#[cfg(test)]"))
        .unwrap_or(false)
}

/// The file list the tree reports, classified. Returns the reading quality
/// alongside: if any file could not be read, deletes are refused for this run.
/// First bytes of a file, for the extension-less rule (#4199). None when unreadable.
fn head_of(abs: &str) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(abs).ok()?;
    let mut buf = [0u8; 160];
    let n = f.read(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf[..n]).into_owned())
}

/// The verdict for one tracked path, opening the file only when the rule needs
/// its content: a .rs (does it declare tests) or an extension-less name (shebang).
fn verdict_for(root: &str, rel: &str) -> Verdict {
    let is_rs = rel.ends_with(".rs");
    let no_ext = {
        let base = rel.rsplit('/').next().unwrap_or(rel);
        !base[1.min(base.len())..].contains('.')
    };
    let head = if no_ext {
        head_of(&format!("{root}/{rel}"))
    } else {
        None
    };
    classify_with_head(
        rel,
        is_rs && rust_declares_tests(&format!("{root}/{rel}")),
        head.as_deref(),
    )
}

/// The file list the tree reports, classified. Returns the reading quality
/// alongside: if any file could not be read, deletes are refused for this run.
fn read_tree(
    root: &str,
    paths: &[String],
    hashes: &HashMap<String, String>,
    scoped: bool,
) -> (Vec<OnDisk>, TreeRead) {
    let mut out = Vec::new();
    let mut complete = if scoped {
        TreeRead::Scoped
    } else {
        TreeRead::Complete
    };
    for rel in paths {
        let verdict = verdict_for(root, rel);
        let classified = matches!(verdict, Verdict::Classified(..));
        let sha = match hashes.get(rel) {
            Some(s) => s.clone(),
            None => {
                complete = TreeRead::Partial;
                continue;
            }
        };
        out.push(OnDisk {
            path: rel.clone(),
            sha,
            classified,
        });
    }
    (out, complete)
}

// ─────────────────────────── the door ───────────────────────────

/// The server is the authority on its own routes. #4158 renamed every
/// collection (/codefiles → /code/files); a hardcoded path here would 404 the
/// night that lands and have to be flipped by hand in every caller — the drift
/// class this card removes. Ask once per run.
fn collection_for(api: &str, kind: &str) -> Result<String, String> {
    let doc = curl(api, "GET", "/", None, None)?;
    // discovery advertises /v1/<domain>/<segment>; the server answers the bare
    // path too, and the bare one is what every other caller uses.
    let needle = format!("\"kind\": \"{kind}\"");
    let at = doc
        .find(&needle)
        .ok_or_else(|| format!("discovery does not serve {kind} — refusing to guess a route"))?;
    let tail = &doc[at..];
    let key = "\"collection\": \"";
    let cs = tail.find(key).ok_or("discovery row has no collection")? + key.len();
    let ce = tail[cs..].find('"').ok_or("unterminated collection")? + cs;
    Ok(tail[cs..ce].trim_start_matches("/v1").to_string())
}

/// This run's identity. The crawler holds its own credential (principal-crawler,
/// #4154) — it never reaches for shared admin, which is the thing that makes a
/// write attributable at all.
///
/// #4192 — the identity lives 600 s and a full pass over the registry does not.
/// The token is re-minted inside the run: before any write that would land
/// within a minute of `exp`, and once more on a 401. A run that cannot re-mint
/// goes RED — it never keeps writing with a token it knows is dead.
struct Identity {
    root: String,
    role: String,
    token: String,
    exp: Option<u64>,
    mints: usize,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn mint_token(root: &str, role: &str) -> Result<String, String> {
    let script = std::env::var("CHORUS_IDENTITY_MINT")
        .unwrap_or_else(|_| format!("{root}/platform/scripts/chorus-identity-token"));
    let t = sh(&script, &[role], root)?.trim().to_string();
    if t.is_empty() {
        return Err(format!("no identity token for {role}"));
    }
    Ok(t)
}

impl Identity {
    /// The first token: the environment's if one is set (a test's fixture, a
    /// caller's hand), else a fresh mint.
    fn open(root: &str, role: &str) -> Result<Identity, String> {
        let token = match std::env::var("CHORUS_IDENTITY_TOKEN") {
            Ok(t) if !t.trim().is_empty() => t,
            _ => mint_token(root, role)?,
        };
        let exp = token_exp(&token);
        Ok(Identity {
            root: root.to_string(),
            role: role.to_string(),
            token,
            exp,
            mints: 0,
        })
    }
    /// A token good for at least the next minute, minting if not.
    fn bearer(&mut self) -> Result<String, String> {
        if token_needs_mint(self.exp, now_secs(), 60) {
            self.remint()?;
        }
        Ok(self.token.clone())
    }
    fn remint(&mut self) -> Result<(), String> {
        let t = mint_token(&self.root, &self.role).map_err(|e| {
            format!("identity re-mint failed ({e}) — refusing to keep writing with a dead token")
        })?;
        self.exp = token_exp(&t);
        self.token = t;
        self.mints += 1;
        Ok(())
    }
}

/// One write through the door under a live identity: mints when the token is
/// about to expire, and retries exactly once on a 401 after a fresh mint.
/// #4201 — one queued full-replace, written after the planning loops.
struct PendingPut {
    kind: &'static str,
    label: String,
    path: String,
    fields: Vec<(String, String)>,
}

/// CHORUS_CRAWL_WRITERS, clamped to 1..=8; unset or unparsable is 4.
fn writers_from_env(v: Option<&str>) -> usize {
    v.and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(4)
        .clamp(1, 8)
}

/// Write the queued PUTs `writers` at a time. The FIRST goes alone through
/// `write()` so the door's prefix is learned and the token minted; the rest go
/// in chunks of 200 under one token (a chunk is well inside the 600 s TTL), and
/// any 401 in a chunk is retried once, alone, after a re-mint. Order within a
/// chunk does not matter: every PUT is a full replace of its own row.
fn flush_puts(
    puts: &mut Vec<PendingPut>,
    ident: &std::cell::RefCell<Identity>,
    api: &str,
    prefixes: &mut PrefixMemory,
    writers: usize,
    failed: &mut Vec<String>,
    wrote: &mut usize,
) {
    if puts.is_empty() {
        return;
    }
    let mut queue: std::collections::VecDeque<PendingPut> = puts.drain(..).collect();
    // the first one, alone: learn the prefix, mint the token
    if let Some(mut first) = queue.pop_front() {
        let mut attempt = 0;
        loop {
            let body = fields_json(&first.fields);
            match write(ident, api, "PUT", &first.path, Some(&body)) {
                Ok(_) => {
                    *wrote += 1;
                    break;
                }
                Err(e) if attempt == 0 && prefixes.learn(&e) && prefixes.apply(&mut first.fields) => {
                    attempt += 1
                }
                Err(e) => {
                    failed.push(format!("update {} {}: {e}", first.kind, first.label));
                    break;
                }
            }
        }
    }
    for p in queue.iter_mut() {
        prefixes.apply(&mut p.fields);
    }
    while !queue.is_empty() {
        let chunk: Vec<PendingPut> = queue.drain(..queue.len().min(200)).collect();
        let token = match ident.borrow_mut().bearer() {
            Ok(t) => t,
            Err(e) => {
                for p in chunk {
                    failed.push(format!("update {} {}: {e}", p.kind, p.label));
                }
                continue;
            }
        };
        let work = std::sync::Mutex::new(std::collections::VecDeque::from(chunk));
        let results: std::sync::Mutex<Vec<(PendingPut, Result<(), String>)>> =
            std::sync::Mutex::new(Vec::new());
        std::thread::scope(|s| {
            for _ in 0..writers {
                s.spawn(|| loop {
                    let next = work.lock().ok().and_then(|mut q| q.pop_front());
                    let Some(p) = next else { break };
                    let body = fields_json(&p.fields);
                    let r = curl(api, "PUT", &p.path, Some(&body), Some(&token)).map(|_| ());
                    if let Ok(mut v) = results.lock() {
                        v.push((p, r));
                    }
                });
            }
        });
        let results = results.into_inner().unwrap_or_default();
        for (p, r) in results {
            match r {
                Ok(()) => *wrote += 1,
                Err(e) if e.contains("HTTP 401") => {
                    // token died mid-chunk: once more, alone, after a re-mint
                    let body = fields_json(&p.fields);
                    match write(ident, api, "PUT", &p.path, Some(&body)) {
                        Ok(_) => *wrote += 1,
                        Err(e2) => failed.push(format!("update {} {}: {e2}", p.kind, p.label)),
                    }
                }
                Err(e) => failed.push(format!("update {} {}: {e}", p.kind, p.label)),
            }
        }
    }
}

#[cfg(test)]
mod writers_4201 {
    use super::writers_from_env;
    #[test]
    fn writers_default_four_and_clamp() {
        assert_eq!(writers_from_env(None), 4);
        assert_eq!(writers_from_env(Some("x")), 4);
        assert_eq!(writers_from_env(Some("1")), 1);
        assert_eq!(writers_from_env(Some("0")), 1);
        assert_eq!(writers_from_env(Some("64")), 8);
    }
}

fn write(
    ident: &std::cell::RefCell<Identity>,
    api: &str,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> Result<String, String> {
    let token = ident.borrow_mut().bearer()?;
    match curl(api, method, path, body, Some(&token)) {
        Err(e) if e.contains("HTTP 401") => {
            ident.borrow_mut().remint()?;
            let token = ident.borrow().token.clone();
            curl(api, method, path, body, Some(&token))
        }
        r => r,
    }
}

fn curl(
    api: &str,
    method: &str,
    path: &str,
    body: Option<&str>,
    token: Option<&str>,
) -> Result<String, String> {
    let url = format!("{api}{path}");
    let mut args: Vec<String> = vec![
        "-s".into(),
        "--max-time".into(),
        "180".into(),
        "-X".into(),
        method.into(),
        "-w".into(),
        "\n%{http_code}".into(),
    ];
    if let Some(t) = token {
        args.push("-H".into());
        args.push(format!("Authorization: Bearer {t}"));
    }
    if let Some(b) = body {
        args.push("-H".into());
        args.push("Content-Type: application/json".into());
        args.push("--data-binary".into());
        args.push(b.into());
    }
    args.push(url.clone());
    let out = Command::new("curl")
        .args(&args)
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (payload, code) = text.rsplit_once('\n').unwrap_or(("", "000"));
    if !code.trim().starts_with('2') {
        return Err(format!(
            "{method} {path} -> HTTP {} {}",
            code.trim(),
            payload.chars().take(200).collect::<String>()
        ));
    }
    Ok(payload.to_string())
}


/// #4214 — the gathering checkout, whose absence makes two page sources a
/// reported SKIP rather than an error (#3097).
const GATHERING_ROOT: &str = "../jeff-bridwell-personal-site";

/// A Page row as the door expects it: the parent CodeFile floor plus the two
/// fields a page adds. `hasDomain` is present only when the rules placed it —
/// an unplaced page is listed by name, never filed under a default.
fn page_fields(row: &pages::PageRow, domain: Option<&str>) -> Vec<(String, String)> {
    let mut f = vec![
        ("name".to_string(), pages::page_row_name(&row.route)),
        ("filePath".to_string(), row.path.clone()),
        ("hasKind".to_string(), "code".to_string()),
        ("route".to_string(), row.route.clone()),
        ("pageType".to_string(), row.page_type.clone()),
    ];
    if let Some(d) = domain {
        f.push(("hasDomain".to_string(), d.to_string()));
    }
    f
}

/// An Endpoint row: the same floor, keyed on method AND path together.
fn endpoint_fields(row: &pages::EndpointRow, domain: Option<&str>) -> Vec<(String, String)> {
    let mut f = vec![
        (
            "name".to_string(),
            pages::endpoint_row_name(&row.http_method, &row.route_path),
        ),
        ("filePath".to_string(), row.path.clone()),
        ("hasKind".to_string(), "code".to_string()),
        ("routePath".to_string(), row.route_path.clone()),
        ("httpMethod".to_string(), row.http_method.clone()),
    ];
    if let Some(d) = domain {
        f.push(("hasDomain".to_string(), d.to_string()));
    }
    f
}

/// Every served row of one class as the planner wants it: the door name, the
/// key the plan matches on, and the domain currently stored.
fn rows_as_in_graph(
    api: &str,
    token: &str,
    kind: &str,
    key_field: &str,
) -> Result<Vec<pages::InGraph>, String> {
    let mut out = Vec::new();
    for fields in fetch_rows(api, token, kind)? {
        let get = |k: &str| -> String {
            fields
                .iter()
                .find(|(f, _)| f == k)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let name = get("name");
        if name.is_empty() {
            continue;
        }
        // An endpoint's key is method AND path: GET /x and POST /x are two rows.
        let key = if kind == "Endpoint" {
            format!("{} {}", get("httpMethod"), get(key_field))
        } else {
            get(key_field)
        };
        let domain = Some(get("hasDomain")).filter(|d| !d.is_empty());
        out.push(pages::InGraph { name, key, domain, fields });
    }
    Ok(out)
}

/// One batch POST, shared by the page and endpoint legs.
fn flush_batch(
    batch: &mut Vec<String>,
    ident: &std::cell::RefCell<Identity>,
    api: &str,
    coll: &str,
    label: &str,
    failed: &mut Vec<String>,
    wrote: &mut usize,
) {
    if batch.is_empty() {
        return;
    }
    let body = format!("[{}]", batch.join(","));
    match write(ident, api, "POST", &format!("{coll}/batch"), Some(&body)) {
        Ok(_) => *wrote += batch.len(),
        Err(e) => failed.push(format!("{label} batch of {}: {e}", batch.len())),
    }
    batch.clear();
}


/// #4214 — the domain a page or endpoint belongs to, READ FROM ITS FILE by the
/// same rules that place a CodeFile. Never the folder, never the name.
fn place_row(
    root: &str,
    path: &str,
    unit_rows: &[(String, String)],
    dir_rows: &[(String, String)],
    valid_domains: &[String],
    card_domain: &dyn Fn(u32) -> Option<String>,
) -> Option<String> {
    let read = |q: &str| std::fs::read_to_string(std::path::Path::new(root).join(q)).ok();
    let content = read(path)?;
    let unit = domain::declared_unit(path, &read);
    domain::place_in_file(
                    &content,
                    path,
                    unit.as_deref(),
                    unit_rows,
                    dir_rows, valid_domains, card_domain, &read)
        .domain()
        .map(str::to_string)
}

/// #4214 — perform the undo. Each name is deleted from whichever fold serves it;
/// a name in neither is REPORTED, never silently counted as done.
fn run_undo(file: &str) -> i32 {
    let body = match std::fs::read_to_string(file) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("chorus-crawl: cannot read the undo list {file} ({e})");
            return 2;
        }
    };
    let names: Vec<&str> = body.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let api = std::env::var("CHORUS_OWL_API").unwrap_or_else(|_| "http://localhost:3360".to_string());
    let root = std::env::var("CHORUS_ROOT").unwrap_or_else(|_| ".".to_string());
    let role = match declared_role(std::env::var("CHORUS_ROLE").ok()) {
        Ok(r) => r,
        Err(why) => {
            eprintln!("chorus-crawl: {why}");
            return 2;
        }
    };
    let ident = match Identity::open(&root, &role) {
        Ok(i) => std::cell::RefCell::new(i),
        Err(e) => {
            eprintln!("chorus-crawl: {e} — an undo is a write and needs an identity");
            return 2;
        }
    };
    let (mut gone, mut failed) = (0usize, Vec::new());
    for line in &names {
        // "<kind> <name>" — the kind is recorded because a bare name cannot say
        // which fold it belongs to.
        let (kind_word, name) = match line.split_once(' ') {
            Some((k, n)) => (k, n),
            None => {
                failed.push(format!("{line}: malformed undo line, expected '<kind> <name>'"));
                continue;
            }
        };
        let coll = if kind_word == "endpoint" { "Endpoint" } else { "Page" };
        let path = match collection_for(&api, coll) {
            Ok(c) => format!("{c}/{name}"),
            Err(e) => {
                failed.push(format!("{name}: {e}"));
                continue;
            }
        };
        match write(&ident, &api, "DELETE", &path, None) {
            Ok(_) => gone += 1,
            Err(e) => failed.push(format!("{name}: {e}")),
        }
    }
    println!("chorus-crawl: undo — {} of {} row(s) deleted", gone, names.len());
    for f in &failed {
        println!("chorus-crawl: undo could NOT delete {f}");
    }
    if failed.is_empty() { 0 } else { 1 }
}

fn json_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

/// #4290 — the name the door serves for the row at `path`, if it serves one.
fn served_name_for(graph: &[InGraph], path: &str) -> Option<String> {
    graph
        .iter()
        .find(|g| g.path == path)?
        .other
        .iter()
        .find(|(k, v)| k == "name" && !v.is_empty())
        .map(|(_, v)| v.clone())
}

#[cfg(test)]
mod served_name_4290 {
    use super::*;

    fn row(path: &str, name: Option<&str>) -> InGraph {
        InGraph {
            path: path.into(),
            sha: String::new(),
            other: name.map(|n| vec![("name".to_string(), n.to_string())]).unwrap_or_default(),
        }
    }

    /// NEGATIVE PROOF: the 2026-09-23 probe row — its served name is not the
    /// name this crawler mints from its path. Deleting by stable_name was the
    /// nightly 404; the served name is the one the door addresses.
    #[test]
    fn a_row_another_writer_minted_is_deleted_by_its_served_name() {
        let path = "zz-4267-zz-probe-20260923T193502Z-codefile-filePath";
        let g = [row(path, Some("zz-probe-20260923t193502z-codefile"))];
        assert_eq!(served_name_for(&g, path).as_deref(), Some("zz-probe-20260923t193502z-codefile"));
        assert_ne!(stable_name(path), "zz-probe-20260923t193502z-codefile", "the minted name is the one that 404'd");
    }

    #[test]
    fn no_served_name_falls_back_to_none() {
        assert_eq!(served_name_for(&[row("a.rs", None)], "a.rs"), None);
        assert_eq!(served_name_for(&[row("a.rs", Some(""))], "a.rs"), None);
        assert_eq!(served_name_for(&[], "a.rs"), None);
    }
}

/// A CodeFile row as the generated door expects it. `name` is a stable key
/// derived from the path, so a re-walk addresses the same row rather than
/// minting a second one.
fn row_json(f: &OnDisk, kind: &str, lang: Option<&str>) -> String {
    let mut s = format!(
        "{{\"name\":\"{}\",\"filePath\":\"{}\",\"hasKind\":\"{}\",\"fileSha\":\"{}\"",
        json_escape(&stable_name(&f.path)),
        json_escape(&f.path),
        kind,
        json_escape(&f.sha)
    );
    if let Some(l) = lang {
        s.push_str(&format!(",\"hasLanguage\":\"{l}\""));
    }
    s.push('}');
    s
}

/// Deterministic row key: a readable slug plus a digest of the EXACT path.
///
/// The slug alone is not injective. The first live run against 6,172 files
/// found it in one batch: `designing/docs/LOG_RELATEDNESS.html` and
/// `designing/docs/log-relatedness.html` are different files that lowercase to
/// the same slug, so the door refused the batch with a duplicate-name conflict.
/// Two files must never share a row. The suffix is FNV-1a over the raw bytes —
/// case, punctuation and all — so distinct paths stay distinct while the name
/// remains something a human can read in a query result.
pub fn stable_name(rel: &str) -> String {
    let mut out = String::with_capacity(rel.len() + 8);
    let mut last_dash = false;
    for c in rel.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in rel.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    format!(
        "file-{}-{:08x}",
        out.trim_matches('-'),
        (h & 0xffff_ffff) as u32
    )
}

#[cfg(test)]
mod stable_name_4173 {
    use super::stable_name;

    // NEGATIVE PROOF (#3734): the two real paths that collided on the first live
    // run. A slug-only key maps both to the same row; the door refused the batch
    // with "duplicate entity name". Distinct paths must stay distinct.
    #[test]
    fn negative_proof_two_paths_differing_only_in_case_do_not_share_a_row() {
        let a = stable_name("designing/docs/LOG_RELATEDNESS.html");
        let b = stable_name("designing/docs/log-relatedness.html");
        assert_ne!(a, b, "these are two different files and must be two rows");
    }

    // The control: the same path must answer the same name every run, or the
    // walk would mint a fresh row each pass instead of addressing the old one.
    #[test]
    fn the_same_path_answers_the_same_name_every_time() {
        assert_eq!(
            stable_name("platform/api/src/server.ts"),
            stable_name("platform/api/src/server.ts")
        );
        assert!(stable_name("platform/api/src/server.ts")
            .starts_with("file-platform-api-src-server-ts-"));
    }
}

/// Every CodeFile the door already serves, as path → sha.
///
/// Pages by the CURSOR the door hands back, not by an offset we invent. The
/// first cut incremented `offset`, which this API ignores: every page returned
/// the same first rows, the loop never saw a short page, and the run spent ten
/// minutes going nowhere. The response carries `links.next` — following what
/// the server says is next is the only paging that cannot silently loop.
///
/// A row past the end would look absent, and absent means delete, so this walks
/// to exhaustion rather than stopping at a ceiling.
fn existing_rows(api: &str, token: &str) -> Result<Vec<InGraph>, String> {
    let mut out = Vec::new();
    for fields in fetch_rows(api, token, "CodeFile")? {
        let get = |k: &str| fields.iter().find(|(f, _)| f == k).map(|(_, v)| v.clone());
        let Some(path) = get("filePath").filter(|p| !p.is_empty()) else {
            continue;
        };
        let sha = get("fileSha").unwrap_or_default();
        let other = fields
            .iter()
            .filter(|(k, _)| k != "filePath" && k != "fileSha")
            .cloned()
            .collect();
        out.push(InGraph { path, sha, other });
    }
    Ok(out)
}

/// #4185 — every Test (case) row the door serves. Identity is filePath +
/// testName (the model's word, and the runner's join); `name` is whatever the
/// row was minted as, legacy or ours, and is only used to address it.
fn existing_case_rows(api: &str, token: &str) -> Result<Vec<CaseInGraph>, String> {
    let mut out = Vec::new();
    for fields in fetch_rows(api, token, "Test")? {
        let get = |k: &str| {
            fields
                .iter()
                .find(|(f, _)| f == k)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let (name, file, case) = (get("name"), get("filePath"), get("testName"));
        if name.is_empty() || file.is_empty() {
            continue;
        }
        out.push(CaseInGraph {
            name,
            file,
            case,
            fields,
        });
    }
    Ok(out)
}

/// #4199 — every log file this box writes: the StandardOut/ErrPath of every
/// plist under platform/launchd (with the job's Label), plus every *.log / *.err
/// in the directories launchd, the app and the spine write into.
fn box_log_files(root: &str) -> Vec<LogFile> {
    let mut out: Vec<LogFile> = Vec::new();
    let mut push = |path: String, label: Option<String>| {
        let Ok(md) = std::fs::metadata(&path) else { return };
        if !md.is_file() {
            return;
        }
        if let Some(e) = out.iter_mut().find(|f| f.path == path) {
            if e.launchd_label.is_none() {
                e.launchd_label = label;
            }
            return;
        }
        let written = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        out.push(LogFile {
            path,
            launchd_label: label,
            size: md.len(),
            written_secs: written,
        });
    };
    if let Ok(rd) = std::fs::read_dir(format!("{root}/platform/launchd")) {
        for e in rd.flatten() {
            let path = e.path();
            if path.extension().map(|x| x == "plist").unwrap_or(false) {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    let label = plist_label(&text);
                    for l in log_paths_in_plist(&text) {
                        push(l, label.clone());
                    }
                }
            }
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        for dir in [
            format!("{home}/Library/Logs/Chorus"),
            format!("{home}/Library/Logs/Gathering"),
            format!("{home}/.chorus"),
            format!("{home}/.chorus/logs"),
        ] {
            if let Ok(rd) = std::fs::read_dir(&dir) {
                for e in rd.flatten() {
                    let p = e.path();
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name.ends_with(".log") || name.ends_with(".err") {
                        push(p.to_string_lossy().to_string(), None);
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// #4199 — every LogSource row the door serves, whole, addressed by its name.
fn existing_log_rows(api: &str, token: &str) -> Result<Vec<LogInGraph>, String> {
    let mut out = Vec::new();
    for fields in fetch_rows(api, token, "LogSource")? {
        let get = |k: &str| {
            fields
                .iter()
                .find(|(f, _)| f == k)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let (name, path) = (get("name"), get("logPath"));
        if name.is_empty() {
            continue;
        }
        out.push(LogInGraph { name, path, fields });
    }
    Ok(out)
}

/// #4310 — the TestResult names whose `ofTest` is this case. A read against the
/// store (reads are anonymous); the deletes still go through the door.
fn results_of_case(case_name: &str) -> Result<Vec<String>, String> {
    let url = std::env::var("FUSEKI_QUERY")
        .unwrap_or_else(|_| "http://localhost:3030/pods/query".to_string());
    let q = cases::results_of_case_query(case_name);
    let out = Command::new("curl")
        .args(["-s", "-f", "--max-time", "60", "-H", "Accept: text/csv", "--data-urlencode"])
        .arg(format!("query={q}"))
        .arg(&url)
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if !out.status.success() {
        return Err(format!("results query failed ({})", out.status));
    }
    Ok(cases::result_names_from_csv(&String::from_utf8_lossy(&out.stdout)))
}

/// Every row of one served class, as flat field maps — paged by the door's own
/// `links.next`, to exhaustion (see the note on existing_rows' first cut).
fn fetch_rows(api: &str, token: &str, kind: &str) -> Result<Vec<Vec<(String, String)>>, String> {
    let coll = collection_for(api, kind)?;
    let mut out = Vec::new();
    let mut next = format!("{coll}?limit=1000");
    let mut pages = 0usize;
    loop {
        let page = curl(api, "GET", &next, None, Some(token))?;
        // #4178 — read the WHOLE row, not two strings. An update has to put the
        // complete entity back (the DAL is full-replace, #3345), so anything
        // dropped here is deleted from the graph on the next content change.
        for obj in row_objects(&page) {
            out.push(row_fields(obj));
        }
        pages += 1;
        // The door's own "next", with the /v1 prefix stripped the way every
        // other caller addresses it. No next link means this was the last page.
        let link = page
            .find("\"next\"")
            .and_then(|i| {
                let t = page[i..]
                    .trim_start_matches("\"next\"")
                    .trim_start()
                    .trim_start_matches(':')
                    .trim()
                    .trim_start_matches('"');
                t.find('"').map(|j| t[..j].to_string())
            })
            .filter(|l| !l.is_empty());
        match link {
            Some(l) => next = l.trim_start_matches("/v1").to_string(),
            None => break,
        }
        if pages > 10_000 {
            return Err(
                "paging did not terminate after 10,000 pages — the door is not advancing".into(),
            );
        }
    }
    Ok(out)
}

/// A Test row as the create batch expects it.
fn case_row_json(name: &str, row: &CaseRow) -> String {
    let mut fields = vec![("name".to_string(), name.to_string())];
    fields.extend(row.owned_fields());
    fields_json(&fields)
}

/// #4131 — platform/services/shared/ is a SOURCE directory other crates include,
/// not a crate; its #[test] fns run under the including crate's names, so a row
/// registered here is a name no lane can ever emit.
fn registers_cases(path: &str) -> bool {
    // #4201 — a file under tests/fixtures/ is DATA a test reads, not a suite.
    // My two placement fixtures landed in the registry the run after I wrote
    // them and moved the counts they exist to prove (read 1045 -> 1048).
    !path.contains("platform/services/shared/") && !path.contains("/tests/fixtures/")
}

#[cfg(test)]
mod registers_cases_4131 {
    use super::registers_cases;

    // #4131 — a source dir other crates include is not a crate; its #[test] fns
    // run under the including crate's names. Registering them minted three rows
    // no lane could ever emit (LANE SILENT every night).
    #[test]
    fn negative_proof_the_shared_source_dir_registers_no_cases_and_a_crate_does() {
        assert!(!registers_cases("platform/services/shared/scope_units.rs"));
        // #4201 — fixture data, read by a proof, never run as a suite
        assert!(!registers_cases(
            "platform/services/chorus-crawl/tests/fixtures/route-cards-header-search.test.ts"
        ));
        assert!(
            registers_cases("platform/services/chorus-crawl/tests/ac_fixtures_4201.rs"),
            "control: the proof itself is a real suite and does register"
        );
        assert!(
            registers_cases("platform/services/werk-test/src/lib.rs"),
            "control: a real crate's file does register"
        );
    }
}

/// #4185 — the case pass over one set of test files: parse each, decide the
/// rows. Returns (desired rows, files parsed, no-case files, declared, inferred,
/// whether every file could be read).
struct Parsed {
    desired: Vec<CaseRow>,
    parsed_files: Vec<String>,
    no_case: Vec<String>,
    /// #4199 — why each no-case file has none (cases::no_case_bucket), parallel to no_case.
    no_case_buckets: Vec<&'static str>,
    declared: usize,
    inferred: usize,
    complete: bool,
    /// #4201 — how the five rules placed the files that have cases
    tags: domain::TagCounts,
    /// one line per conflicted or unplaced file
    tag_lines: Vec<String>,
}

/// The authored unit → domain rows, relative to the tree root (#4084).
const UNIT_DOMAIN_TTL: &str = "roles/silas/ontology/unit-domain-4084.ttl";
/// #4222 — the authored route → domain and log-file → domain rows. Beside the
/// unit rows, for the same reason: a mapping only a build can change is not a
/// model. An unmapped surface is reported unplaced, never defaulted.
const SURFACE_DOMAIN_TTL: &str = "roles/kade/ontology/surface-domain-4222.ttl";

fn parse_cases(
    root: &str,
    test_files: &[&str],
    valid_domains: &[String],
    card_domain: &dyn Fn(u32) -> Option<String>,
) -> Parsed {
    // #4084's authored unit → domain rows, read once per run.
    let unit_rows = domain::unit_domain_rows(
        &std::fs::read_to_string(format!("{root}/{UNIT_DOMAIN_TTL}")).unwrap_or_default(),
    );
    let dir_rows = domain::surface_domain_rows(
        &std::fs::read_to_string(format!("{root}/{SURFACE_DOMAIN_TTL}")).unwrap_or_default(),
        "chorus:pathPrefix",
    );
    let unit_rows = &unit_rows[..];
    let mut p = Parsed {
        desired: Vec::new(),
        parsed_files: Vec::new(),
        no_case: Vec::new(),
        no_case_buckets: Vec::new(),
        declared: 0,
        inferred: 0,
        complete: true,
        tags: domain::TagCounts::default(),
        tag_lines: Vec::new(),
    };
    for path in test_files {
        if !registers_cases(path) {
            continue;
        }
        let content = match std::fs::read_to_string(format!("{root}/{path}")) {
            Ok(c) => c,
            Err(_) => {
                // A file we could not read is a fact about this run, not about the
                // repo: its rows are left alone and deletes are refused (#4022).
                p.complete = false;
                continue;
            }
        };
        p.parsed_files.push(path.to_string());
        let names = cases::case_names(path, &content);
        if names.is_empty() {
            p.no_case.push(path.to_string());
            p.no_case_buckets
                .push(cases::no_case_bucket(path, &content));
            continue;
        }
        let fc = cases::file_class(path, &content);
        if fc.declared {
            p.declared += 1
        } else {
            p.inferred += 1
        }
        // #4201 — the domain comes from the file, never the folder
        // #4201 — the unit the file declares (nearest manifest), for the rule
        // that reads a crate source file's own identity.
        let unit = domain::declared_unit(path, &|p: &str| {
            std::fs::read_to_string(std::path::Path::new(&root).join(p)).ok()
        });
        let placement = domain::place_in_file(
                    &content,
                    path,
                    unit.as_deref(),
                    unit_rows,
                    &dir_rows,
            valid_domains,
            card_domain,
            &|p: &str| std::fs::read_to_string(std::path::Path::new(&root).join(p)).ok(),
        );
        p.tags.read += 1;
        match &placement {
            domain::Placement::Tagged { .. } => p.tags.placed += 1,
            domain::Placement::Conflict { .. } => p.tags.conflicts += 1,
            domain::Placement::Unplaced => p.tags.unplaced += 1,
        }
        if let Some(line) = domain::listing(path, &placement) {
            p.tag_lines.push(line);
        }
        let covers = cases::covers_from(placement.domain(), fc.concern);
        let in_file = stable_name(path);
        for case in names {
            p.desired.push(CaseRow {
                file: path.to_string(),
                case,
                covers: covers.clone(),
                layer: fc.layer.to_string(),
                hermeticity: fc.hermeticity.to_string(),
                concern: fc.concern.map(|c| c.to_string()),
                in_file: in_file.clone(),
            });
        }
    }
    p
}

/// The share gate's cap and floor, from env (MAX_DOMAIN_SHARE, MIN_CORPUS_FOR_SHARES)
/// with testfiles.py's defaults.
fn share_limits() -> (f64, usize) {
    let cap = std::env::var("MAX_DOMAIN_SHARE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.30);
    let floor = std::env::var("MIN_CORPUS_FOR_SHARES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    (cap, floor)
}

/// The hermetic seams the bats suites drive (#4022 #4106 #4111 #3924 #3996):
/// one file in, the answer out, no store, no network. Returns true if one ran.
fn seam(args: &[String]) -> bool {
    let arg = |i: usize| args.get(i).cloned().unwrap_or_default();
    match arg(1).as_str() {
        "--names-of" => {
            let path = arg(2);
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            for n in cases::case_names(&path, &content) {
                println!("{n}");
            }
        }
        "--covers-of" => {
            // #4201 — hermetic: the valid Domain list comes from CHORUS_VALID_DOMAINS
            // (comma-separated) since no store is in reach; prints the domain, or
            // "conflict: ..." / "unplaced".
            let path = arg(2);
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let valid: Vec<String> = std::env::var("CHORUS_VALID_DOMAINS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            // #4201 — the seam must answer with the SAME rules the crawl runs,
            // unit rule included, or it reports a file unplaced that the pass
            // places (seen 2026-09-17 on chorus-hooks).
            let unit = domain::declared_unit(&path, &|p: &str| std::fs::read_to_string(p).ok());
            let unit_rows = domain::unit_domain_rows(
                &std::fs::read_to_string(UNIT_DOMAIN_TTL).unwrap_or_default(),
            );
            let dir_rows = domain::surface_domain_rows(
                &std::fs::read_to_string(SURFACE_DOMAIN_TTL).unwrap_or_default(),
                "chorus:pathPrefix",
            );
            let placement = domain::place_in_file(
                &content,
                &path,
                unit.as_deref(),
                &unit_rows,
                &dir_rows,
                &valid,
                &|_| None,
                &|p: &str| std::fs::read_to_string(p).ok(),
            );
            let concern = cases::file_class(&path, &content).concern;
            let covers = cases::covers_from(placement.domain(), concern);
            // #4201 — the answer on stdout, the reason on stderr. Printing
            // both on one line made the seam unreadable by anything that
            // compares it: `security (unplaced …)` never equals `security`.
            println!("{covers}");
            if let Some(l) = domain::listing(&path, &placement) {
                eprintln!("{l}");
            }
        }
        "--classify" => {
            let path = arg(2);
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let fc = cases::file_class(&path, &content);
            println!(
                "{} {} {} {}",
                fc.layer,
                fc.hermeticity,
                fc.concern.unwrap_or("-"),
                if fc.declared { "declared" } else { "inferred" }
            );
        }
        "--check-shares" => {
            let json = std::fs::read_to_string(arg(2)).unwrap_or_default();
            let counts = match cases::parse_count_object(&json) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("chorus-crawl: {e}");
                    std::process::exit(2);
                }
            };
            let (cap, floor) = share_limits();
            match cases::check_shares(&counts, cap, floor) {
                Ok(()) => println!("shares ok"),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        _ => return false,
    }
    true
}

/// #4201 — a run that cannot write must say so ON the count line, not in a
/// footnote. `--reconcile` is read-only, but its `posted/replaced/deleted`
/// line was byte-identical to a write pass's, and the disclaimer sat eleven
/// lines below. On 2026-09-17 that cost an afternoon: two reconciles were read
/// as data changes, and the store never moved. The counts a run did not make
/// carry the reason on the same line.
fn wrote_nothing(dry_run: bool, reconciling: bool) -> &'static str {
    match (dry_run, reconciling) {
        (true, _) => "  (dry-run — NOTHING WRITTEN)",
        (_, true) => "  (--reconcile is read-only — NOTHING WRITTEN, this is what a write pass would do)",
        _ => "",
    }
}

#[cfg(test)]
mod wrote_nothing_4201 {
    use super::wrote_nothing;

    /// Negative proof: the state that cost the afternoon — a reconcile whose
    /// counts read as writes. The line must name itself read-only.
    #[test]
    fn a_reconcile_line_says_nothing_was_written() {
        let s = wrote_nothing(false, true);
        assert!(s.contains("NOTHING WRITTEN"), "{s}");
        assert!(s.contains("read-only"), "{s}");
    }

    #[test]
    fn a_dry_run_line_says_nothing_was_written() {
        assert!(wrote_nothing(true, false).contains("NOTHING WRITTEN"));
    }

    /// Control: a real write pass carries no disclaimer, so the marker never
    /// becomes noise that stops being read.
    #[test]
    fn a_write_pass_carries_no_disclaimer() {
        assert_eq!(wrote_nothing(false, false), "");
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    if seam(&argv) {
        return;
    }
    let root = std::env::var("CHORUS_ROOT").unwrap_or_else(|_| ".".to_string());
    // #4214 — `--undo <file>`: delete exactly the rows a previous run created,
    // by the names that run recorded. Runs alone, writes nothing else, and says
    // what it could not delete rather than reporting a clean sweep.
    if let Some(i) = std::env::args().position(|a| a == "--undo") {
        let file = match std::env::args().nth(i + 1) {
            Some(f) => f,
            None => {
                eprintln!("chorus-crawl: --undo needs the undo list a run wrote");
                std::process::exit(2);
            }
        };
        std::process::exit(run_undo(&file));
    }
    let dry_run = std::env::args().any(|a| a == "--dry-run");
    // #4292 — `--validate` is the crawler-validate control's name for the
    // same read-only pass (#4290); the nightly lane calls it by that name.
    let reconciling = std::env::args().any(|a| a == "--reconcile" || a == "--validate");

    let head = match head_commit(&root) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("chorus-crawl: cannot read HEAD — {e}");
            std::process::exit(2);
        }
    };

    // The watermark lives on the graph; until the door carries it, a full walk
    // is the honest answer and it says so.
    // --reconcile ALWAYS walks the whole tree. It ran once in delta scope and
    // reported "clean" having compared ZERO files against 5,533 rows — a check
    // that passes because it looked at nothing is the hollow gate #3734 exists
    // to forbid, and this one was in the reconcile itself.
    let watermark: Option<String> = if reconciling {
        None
    } else {
        std::env::var("CHORUS_CRAWL_WATERMARK")
            .ok()
            .or_else(|| std::fs::read_to_string(format!("{root}/.chorus-crawl-watermark")).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let reachable = watermark
        .as_deref()
        .map(|w| commit_is_reachable(&root, w))
        .unwrap_or(false);
    // scope_for cannot know WHY the watermark is absent; on a reconcile we
    // cleared it ourselves, and reporting that as "first run" would have the
    // run narrate a state it is not in.
    let scope = match scope_for(watermark.as_deref(), &head, reachable) {
        Scope::Full { .. } if reconciling => Scope::Full {
            why: "--reconcile: forced full walk",
        },
        s => s,
    };

    // On a delta, the ONLY rows eligible for deletion are the ones git says
    // were removed or renamed away. The walk never sees the rest.
    let mut removed_by_git: Vec<String> = Vec::new();
    let paths = match (&scope, tracked_files(&root)) {
        (_, Err(e)) => {
            eprintln!("chorus-crawl: cannot list tracked files — {e}");
            std::process::exit(2);
        }
        (Scope::Full { .. }, Ok(all)) => all,
        (Scope::Delta { from, to }, Ok(all)) => match changes_since(&root, from, to) {
            Ok(changes) => {
                let mut touched: Vec<String> = Vec::new();
                for c in changes {
                    match c {
                        Change::Touched(p) => touched.push(p),
                        // A rename is a move: the new path is walked, the old
                        // one is removed. Never a delete-plus-add.
                        Change::Renamed { from, to } => {
                            touched.push(to);
                            removed_by_git.push(from);
                        }
                        Change::Removed(p) => removed_by_git.push(p),
                    }
                }
                touched.retain(|p| all.contains(p));
                touched
            }
            Err(e) => {
                eprintln!("chorus-crawl: delta unreadable ({e}) — widening to a full walk");
                all
            }
        },
    };

    let hashes = match tree_hashes(&root) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("chorus-crawl: cannot read git's content hashes — {e}");
            std::process::exit(2);
        }
    };
    // A delta walked only the changed files. Absence from that view says
    // nothing about the rest of the graph — see TreeRead::Scoped.
    let walked_subset = matches!(scope, Scope::Delta { .. });
    let (disk, read) = read_tree(&root, &paths, &hashes, walked_subset);

    let api =
        std::env::var("CHORUS_OWL_API").unwrap_or_else(|_| "http://localhost:3360".to_string());
    // #4210 — a default here INVENTS a principal. Every row this pass wrote
    // landed owned by `crawler`, and Silas hand-moved them three times on
    // 2026-09-18 alone. The caller says who it is or the pass refuses.
    let role = match declared_role(std::env::var("CHORUS_ROLE").ok()) {
        Ok(r) => r,
        Err(why) => {
            eprintln!("chorus-crawl: {why}");
            std::process::exit(2);
        }
    };

    // What the graph already holds. Read BEFORE deciding anything — the walk is
    // idempotent by diff, not by luck.
    let started = std::time::Instant::now();
    let ident: Option<std::cell::RefCell<Identity>> = if dry_run && !reconciling {
        None
    } else {
        match Identity::open(&root, &role) {
            Ok(i) => Some(std::cell::RefCell::new(i)),
            Err(e) => {
                eprintln!("chorus-crawl: {e} — refusing to write without an identity");
                std::process::exit(2);
            }
        }
    };
    let token: String = match &ident {
        Some(i) => match i.borrow_mut().bearer() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("chorus-crawl: {e}");
                std::process::exit(2);
            }
        },
        None => String::new(),
    };
    let graph = if token.is_empty() {
        Vec::new()
    } else {
        match existing_rows(&api, &token) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("chorus-crawl: cannot read existing rows ({e}) — refusing to plan against an unknown graph");
                std::process::exit(2);
            }
        }
    };

    // #4185 — the case registry, read under the same identity and the same
    // "read before deciding" rule. Empty in a token-less dry run, like `graph`.
    let case_graph: Vec<CaseInGraph> = if token.is_empty() {
        Vec::new()
    } else {
        match existing_case_rows(&api, &token) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("chorus-crawl: cannot read existing Test rows ({e}) — refusing to plan against an unknown registry");
                std::process::exit(2);
            }
        }
    };

    // The store can be rebuilt under a surviving watermark. A delta planned
    // against a graph that no longer holds the rows the watermark promises is
    // not a small run — it is a run that leaves the hole in place and then
    // advances the watermark over it. Re-scope to a full walk and say so.
    let (scope, paths, disk, read) = if !delta_is_trustworthy(graph.len(), hashes.len())
        && matches!(scope, Scope::Delta { .. })
    {
        eprintln!(
            "chorus-crawl: the graph holds 0 rows but a watermark claims {} tracked files are already there — the store was reset; widening to a full walk",
            hashes.len()
        );
        let all = match tracked_files(&root) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("chorus-crawl: cannot list tracked files — {e}");
                std::process::exit(2);
            }
        };
        let (d, r) = read_tree(&root, &all, &hashes, false);
        (
            Scope::Full {
                why: "graph was reset under a live watermark",
            },
            all,
            d,
            r,
        )
    } else {
        (scope, paths, disk, read)
    };
    let _ = &paths;

    let mut actions = plan(&disk, &graph, read);
    for path in &removed_by_git {
        if graph.iter().any(|g| &g.path == path) {
            actions.push(Action::Delete { path: path.clone() });
        }
    }
    // #4185 — a plan that would delete most of the graph is not a plan, it is a
    // wrong CHORUS_ROOT. Refuse every delete, say so, hold the watermark.
    let file_deletes = actions
        .iter()
        .filter(|a| matches!(a, Action::Delete { .. }))
        .count();
    let mut mass_delete = mass_delete_refused(file_deletes, graph.len());
    if mass_delete {
        eprintln!(
            "chorus-crawl: MASS DELETE REFUSED — the plan would delete {} of {} file rows. A full pass never removes half the graph; this tree is not the tree the graph describes (wrong CHORUS_ROOT?). No row is deleted this run.",
            file_deletes,
            graph.len()
        );
        actions.retain(|a| !matches!(a, Action::Delete { .. }));
    }
    // #4201 — tallied AFTER the hasDomain restate below, or the line reports
    // 30 replaces on a run that restates 1,563 rows.

    // #4185 — the case pass: every kind=test file this run walked, parsed.
    let test_files: Vec<&str> = disk
        .iter()
        .filter(|f| f.classified)
        .filter(|f| {
            matches!(
                verdict_for(&root, &f.path),
                Verdict::Classified(Kind::Test, _)
            )
        })
        .map(|f| f.path.as_str())
        .collect();
    // #4201 — only a real Domain row can be a tag; read them from the API.
    let valid_domains: Vec<String> = match fetch_rows(&api, &token, "Domain") {
        Ok(rows) => rows
            .into_iter()
            .filter_map(|f| f.into_iter().find(|(k, _)| k == "name").map(|(_, v)| v))
            .filter(|v| !v.is_empty())
            .collect(),
        Err(e) => {
            eprintln!("chorus-crawl: cannot read Domain rows ({e}) — refusing to tag tests against an unknown domain list");
            std::process::exit(2);
        }
    };
    // rule 5: a card's labels name a real Domain. Today the board carries
    // sequence/subproduct labels (athena, werk, borg), not Domain rows, so this
    // answers None for every card; the rule is wired and inert until a label does.
    let card_domain = |_card: u32| -> Option<String> { None };

    // #4201 — a row whose stored hasDomain is not what the rules now say is
    // stale even though its sha has not moved. plan() ran before the Domain
    // list was readable, so the eligibility is settled here: Unchanged becomes
    // Replace for exactly those rows, and for nothing else.
    {
        let unit_rows = domain::unit_domain_rows(
            &std::fs::read_to_string(format!("{root}/{UNIT_DOMAIN_TTL}")).unwrap_or_default(),
        );
    let dir_rows = domain::surface_domain_rows(
        &std::fs::read_to_string(format!("{root}/{SURFACE_DOMAIN_TTL}")).unwrap_or_default(),
        "chorus:pathPrefix",
    );
        let read_file = |q: &str| std::fs::read_to_string(std::path::Path::new(&root).join(q)).ok();
        let in_graph: std::collections::HashMap<&str, &InGraph> =
            graph.iter().map(|g| (g.path.as_str(), g)).collect();
        let mut restated = 0usize;
        for a in actions.iter_mut() {
            let Action::Unchanged { path } = a else { continue };
            let Some(g) = in_graph.get(path.as_str()) else {
                continue;
            };
            // #4222 — an unreadable file (image, binary) never reached the
            // restate pass, so 362 PNGs whose sha never moves could never pick
            // up a rule: they are Unchanged forever. Same path chain here.
            let Some(content) = read_file(path) else {
                let want_path: Vec<String> = domain::place_by_file_name(path, &valid_domains)
                    .or_else(|| domain::place_by_tree(path, &valid_domains))
                    .or_else(|| domain::place_by_dir(path, &valid_domains, &dir_rows))
                    .map(|s| vec![s.domain])
                    .unwrap_or_default();
                let mut held_path: Vec<String> = g
                    .other
                    .iter()
                    .filter(|(k, _)| k == "hasDomain")
                    .map(|(_, v)| v.clone())
                    .collect();
                held_path.sort();
                held_path.dedup();
                if want_path != held_path {
                    restated += 1;
                    *a = Action::Replace { path: path.clone() };
                }
                continue;
            };
            let unit = domain::declared_unit(path, &read_file);
            let want = domain::place_in_file(
                        &content,
                        path,
                        unit.as_deref(),
                        &unit_rows,
                        &dir_rows,
                &valid_domains,
                &card_domain,
                &read_file,
            );
            // #4222 — the stored value may now be a set. Compare sets, not one
            // string, or every multi-domain row restates on every run.
            let want_set = want.domains();
            let mut held_set: Vec<String> = g
                .other
                .iter()
                .filter(|(k, _)| k == "hasDomain")
                .map(|(_, v)| v.clone())
                .collect();
            held_set.sort();
            held_set.dedup();
            if want_set != held_set {
                restated += 1;
                *a = Action::Replace { path: path.clone() };
            }
        }
        if restated > 0 {
            println!("chorus-crawl: {restated} code row(s) restated — hasDomain differs from the rules");
        }
    }

    let c = counts(&actions);
    let parsed = parse_cases(&root, &test_files, &valid_domains, &card_domain);
    // A test file we could not read outranks a clean tree read: no deletes.
    let case_read = if parsed.complete {
        read
    } else {
        TreeRead::Partial
    };
    let case_actions = cases::plan_cases(
        &parsed.desired,
        &parsed.parsed_files,
        &removed_by_git,
        &case_graph,
        case_read,
    );
    let case_deletes = case_actions
        .iter()
        .filter(|a| matches!(a, CaseAction::Delete { .. }))
        .count();
    let mut case_actions = case_actions;
    if mass_delete_refused(case_deletes, case_graph.len()) {
        eprintln!(
            "chorus-crawl: MASS DELETE REFUSED — the plan would delete {} of {} case rows. A full pass never removes half the registry; this tree is not the tree the graph describes (wrong CHORUS_ROOT?). No case row is deleted this run.",
            case_deletes,
            case_graph.len()
        );
        case_actions.retain(|a| !matches!(a, CaseAction::Delete { .. }));
        mass_delete = true;
    }
    let cc = cases::case_counts(&case_actions);

    // #4222 — the authored surface rows, read once. Two keys, one file:
    // `chorus:routePath` for an endpoint, `chorus:logFileName` for a log file.
    // A missing file is not a default — it is zero rows, and every surface that
    // needed one is reported unplaced by name.
    let surface_ttl =
        std::fs::read_to_string(format!("{root}/{SURFACE_DOMAIN_TTL}")).unwrap_or_default();
    let route_rows = domain::surface_domain_rows(&surface_ttl, "chorus:routePath");
    let log_file_rows = domain::surface_domain_rows(&surface_ttl, "chorus:logFileName");

    // #4199 — the log leg: every log file the box writes has a LogSource row.
    // A full pass refreshes the rows it owns; a delta adds and retires only.
    let log_files = box_log_files(&root);
    let log_graph = match existing_log_rows(&api, &token) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("chorus-crawl: cannot read existing LogSource rows ({e}) — refusing to plan the log leg against an unknown registry");
            std::process::exit(2);
        }
    };
    let on_box = |p: &str| std::path::Path::new(p).is_file();
    let mut log_actions = plan_logs(&log_files, &log_graph, scope_was_full_walk(&scope), &on_box);
    let log_deletes = log_actions
        .iter()
        .filter(|a| matches!(a, LogAction::Delete { .. }))
        .count();
    if mass_delete_refused(log_deletes, log_graph.len()) {
        eprintln!(
            "chorus-crawl: MASS DELETE REFUSED — the plan would delete {} of {} log rows. No log row is deleted this run.",
            log_deletes,
            log_graph.len()
        );
        log_actions.retain(|a| !matches!(a, LogAction::Delete { .. }));
        mass_delete = true;
    }
    let lc = log_counts(&log_actions);

    println!(
        "chorus-crawl: {} · tracked={} read={:?}",
        scope.label(),
        paths.len(),
        read
    );
    println!(
        "chorus-crawl: posted={} replaced={} unchanged={} deleted={} skipped={}{}",
        c.posted,
        c.replaced,
        c.unchanged,
        c.deleted,
        c.skipped,
        wrote_nothing(dry_run, reconciling)
    );
    println!(
        "chorus-crawl: cases posted={} replaced={} unchanged={} deleted={} · test files parsed={} declared={} inferred={} no-case={}{}",
        cc.posted, cc.replaced, cc.unchanged, cc.deleted,
        parsed.parsed_files.len(), parsed.declared, parsed.inferred, parsed.no_case.len(),
        wrote_nothing(dry_run, reconciling)
    );
    if !parsed.no_case.is_empty() {
        println!("chorus-crawl: {}", cases::no_case_report(&parsed.no_case));
    }
    println!("chorus-crawl: tags: {}", parsed.tags.render());
    for l in &parsed.tag_lines {
        println!("chorus-crawl: {l}");
    }
    println!(
        "chorus-crawl: logs posted={} replaced={} unchanged={} deleted={} · log files={} rows={}{}",
        lc.posted, lc.replaced, lc.unchanged, lc.deleted, log_files.len(), log_graph.len(),
        if dry_run { "  (dry-run — nothing written)" } else { "" }
    );
    if read == TreeRead::Partial {
        println!("chorus-crawl: tree read was PARTIAL — deletes refused this run (#4022: absent must not mean delete)");
    } else if case_read == TreeRead::Partial {
        println!("chorus-crawl: a test file could not be read — case read was PARTIAL, case deletes refused this run (#4022: absent must not mean delete)");
    }
    // `--reconcile`: the nightly pass. Compares the graph to the tree BOTH ways
    // and names the paths. Drift is not a number to log — "6,140 vs 6,175"
    // tells nobody which thirty-five.
    if reconciling {
        // Refuse to grade an empty walk. "Clean" over zero files is not a pass.
        if disk.is_empty() || read != TreeRead::Complete {
            eprintln!(
                "chorus-crawl: reconcile REFUSED — walked {} file(s), read={:?}. A reconcile that looked at nothing cannot say clean.",
                disk.len(),
                read
            );
            std::process::exit(2);
        }
        // #4180 — the counts line above reads exactly like a run's outcome, and
        // --reconcile writes NOTHING: it exited here with "posted=1" on the line
        // and the row still absent, and I read that as work done. Say what the
        // numbers are.
        if c.posted + c.replaced + c.deleted + lc.posted + lc.replaced + lc.deleted > 0 {
            println!(
                "chorus-crawl: reconcile is read-only — the {} post / {} replace / {} delete above (logs {} / {} / {}) are what a write pass WOULD do, not what happened",
                c.posted, c.replaced, c.deleted, lc.posted, lc.replaced, lc.deleted
            );
        }
        let all_test_files: Vec<String> = test_files
            .iter()
            .map(|s| s.to_string())
            .filter(|p| registers_cases(p))
            .collect();
        let wm_file = std::fs::read_to_string(format!("{root}/.chorus-crawl-watermark"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let clean = print_graph_vs_project(
            &root,
            &api,
            &token,
            &disk,
            &graph,
            &parsed.desired,
            &all_test_files,
            &case_graph,
            &head,
            wm_file.as_deref(),
            &parsed.no_case_buckets,
            &parsed.no_case,
            &parsed.tags,
        );
        std::process::exit(if clean { 0 } else { 1 });
    }

    // ── #4214: plan the UI-Pages and API-Contract folds. Deliberately ABOVE the
    // dry-run return: the code, case and log legs all report their plan on a dry
    // run, and a leg that can only be seen by writing cannot be previewed at all.
    // Mine could not, which the first dry run against the variant showed.
    let (want_pages, want_endpoints, page_plan, endpoint_plan, page_graph, endpoint_graph) = {
        // #4201's authored unit rows, read here because planning now happens
        // before the write section that used to own them.
        let unit_rows = domain::unit_domain_rows(
            &std::fs::read_to_string(format!("{root}/{UNIT_DOMAIN_TTL}")).unwrap_or_default(),
        );
    let dir_rows = domain::surface_domain_rows(
        &std::fs::read_to_string(format!("{root}/{SURFACE_DOMAIN_TTL}")).unwrap_or_default(),
        "chorus:pathPrefix",
    );
        let gathering_present = std::path::Path::new(GATHERING_ROOT).is_dir();
        let read_file = |q: &str| std::fs::read_to_string(std::path::Path::new(&root).join(q)).ok();
        let (want_pages, want_endpoints, skipped) =
            pages::desired_rows(&paths, &read_file, gathering_present);
        for dir in &skipped {
            println!("chorus-crawl: pages SKIPPED {dir} — that checkout is not present on this box");
        }
        let page_graph = rows_as_in_graph(&api, &token, "Page", "route").unwrap_or_default();
        let endpoint_graph = rows_as_in_graph(&api, &token, "Endpoint", "routePath").unwrap_or_default();
        let full = scope_was_full_walk(&scope) && read == TreeRead::Complete;
        let mut page_plan = pages::plan_rows(
            &want_pages,
            &page_graph,
            &|r: &pages::PageRow| r.route.clone(),
            &|r: &pages::PageRow| pages::page_domain(&r.route, &valid_domains)
                .or_else(|| place_row(&root, &r.path, &unit_rows, &dir_rows, &valid_domains, &card_domain)),
            full,
        );
        let mut endpoint_plan = pages::plan_rows(
            &want_endpoints,
            &endpoint_graph,
            &|r: &pages::EndpointRow| format!("{} {}", r.http_method, r.route_path),
            &|r: &pages::EndpointRow| pages::endpoint_domain(&r.route_path, &valid_domains, &route_rows)
                .or_else(|| place_row(&root, &r.path, &unit_rows, &dir_rows, &valid_domains, &card_domain)),
            full,
        );
        // #4022 + #4214 — absent must not mean delete, and the shared guard has a
        // 100-row floor that a 63-row collection never reaches.
        for (label, dels, have) in [
            ("page", page_plan.iter().filter(|a| matches!(a, pages::RowAction::Delete { .. })).count(), page_graph.len()),
            ("endpoint", endpoint_plan.iter().filter(|a| matches!(a, pages::RowAction::Delete { .. })).count(), endpoint_graph.len()),
        ] {
            if dels > 0 && (read != TreeRead::Complete || pages::leg_mass_delete_refused(dels, have)) {
                eprintln!("chorus-crawl: {label} deletes REFUSED this run ({dels} of {have})");
                if label == "page" {
                    page_plan.retain(|a| !matches!(a, pages::RowAction::Delete { .. }));
                } else {
                    endpoint_plan.retain(|a| !matches!(a, pages::RowAction::Delete { .. }));
                }
            }
        }
        let pc = pages::counts(&page_plan);
        let ec = pages::counts(&endpoint_plan);
        println!(
            "chorus-crawl: pages posted={} replaced={} unchanged={} deleted={} · endpoints posted={} replaced={} unchanged={} deleted={} · page files={} endpoint routes={} rows={}/{}{}",
            pc.posted, pc.replaced, pc.unchanged, pc.deleted,
            ec.posted, ec.replaced, ec.unchanged, ec.deleted,
            want_pages.len(), want_endpoints.len(), page_graph.len(), endpoint_graph.len(),
            wrote_nothing(dry_run, reconciling)
        );
        // #4214 — the undo list, written BEFORE the run performs anything. Wren's
        // condition: a plan that can only go forward is not a plan. This file is
        // what `--undo <file>` deletes, and it names ONLY rows this run creates.
        let undo = pages::created_names(&page_plan, &endpoint_plan);
        if !undo.is_empty() {
            let dir = format!("{}/.chorus/ops", std::env::var("HOME").unwrap_or_default());
            let _ = std::fs::create_dir_all(&dir);
            let path = format!("{dir}/crawl-folds-undo-{}.txt", now_secs());
            match std::fs::write(&path, undo.join("\n") + "\n") {
                Ok(()) => println!("chorus-crawl: undo list ({} row(s)) → {path}", undo.len()),
                Err(e) => eprintln!("chorus-crawl: could not write the undo list ({e}) — refusing to write rows I cannot take back"),
            }
        }
        (want_pages, want_endpoints, page_plan, endpoint_plan, page_graph, endpoint_graph)
    };

    if dry_run {
        return;
    }

    // Writes. One failed write fails the run: a crawler that reports success
    // while rows are missing is the silence this card exists to end.
    let coll = match collection_for(&api, "CodeFile") {
        Ok(c) => c,
        Err(e) => {
            eprintln!("chorus-crawl: {e}");
            std::process::exit(2);
        }
    };
    let by_path: HashMap<&str, &OnDisk> = disk.iter().map(|f| (f.path.as_str(), f)).collect();
    // #4201 — the authored unit rows, for tagging each code row's domain.
    let unit_rows = domain::unit_domain_rows(
        &std::fs::read_to_string(format!("{root}/{UNIT_DOMAIN_TTL}")).unwrap_or_default(),
    );
    let dir_rows = domain::surface_domain_rows(
        &std::fs::read_to_string(format!("{root}/{SURFACE_DOMAIN_TTL}")).unwrap_or_default(),
        "chorus:pathPrefix",
    );
    let mut wrote = 0usize;
    let mut failed: Vec<String> = Vec::new();
    let mut batch: Vec<String> = Vec::new();

    let ident = ident.as_ref().expect("writes happen only with an identity");
    let mut prefixes = PrefixMemory::default();
    // #4201 — updates are queued and written `writers` at a time (CHORUS_CRAWL_WRITERS, default 4)
    let writers = writers_from_env(std::env::var("CHORUS_CRAWL_WRITERS").ok().as_deref());
    let mut puts: Vec<PendingPut> = Vec::new();
    let flush = |batch: &mut Vec<String>, failed: &mut Vec<String>, wrote: &mut usize| {
        if batch.is_empty() {
            return;
        }
        let body = format!("[{}]", batch.join(","));
        match write(ident, &api, "POST", &format!("{coll}/batch"), Some(&body)) {
            Ok(_) => *wrote += batch.len(),
            Err(e) => failed.push(format!("batch of {}: {e}", batch.len())),
        }
        batch.clear();
    };

    let in_graph: HashMap<&str, &InGraph> = graph.iter().map(|g| (g.path.as_str(), g)).collect();

    for a in &actions {
        match a {
            Action::Post { path } => {
                let f = match by_path.get(path.as_str()) {
                    Some(f) => *f,
                    None => continue,
                };
                if let Verdict::Classified(k, l) = verdict_for(&root, path) {
                    let row = row_json(f, k.as_str(), l);
                    if !batch_accepts(batch_bytes(&batch), row.len(), BATCH_BODY_BUDGET) {
                        flush(&mut batch, &mut failed, &mut wrote);
                    }
                    batch.push(row);
                    if batch.len() >= 200 {
                        flush(&mut batch, &mut failed, &mut wrote);
                    }
                }
            }
            // #4178 — an UPDATE cannot go through the create batch (409
            // already-exists) and cannot restate only the crawler's fields
            // (full-replace deletes the rest). It puts the complete entity:
            // the row as served, with our four fields written over it.
            Action::Replace { path } => {
                let f = match by_path.get(path.as_str()) {
                    Some(f) => *f,
                    None => continue,
                };
                let Verdict::Classified(k, l) = verdict_for(&root, path) else {
                    continue;
                };
                let existing: &[(String, String)] = match in_graph.get(path.as_str()) {
                    Some(g) => &g.other,
                    None => &[],
                };
                let mut owned = vec![
                    ("filePath".to_string(), path.clone()),
                    ("fileSha".to_string(), f.sha.clone()),
                    ("hasKind".to_string(), k.as_str().to_string()),
                ];
                if let Some(lang) = l {
                    owned.push(("hasLanguage".to_string(), lang.to_string()));
                }
                // #4201 — the same rules that place a test place the file it
                // tests. `hasDomain` was declared and empty on all 6,226 code
                // rows, which is why a test whose only signal is the module it
                // imports still cannot be placed: the module has no domain
                // either. Read from the file, never the folder; silence when
                // the rules cannot agree, exactly as for a test.
                // #4222 — a file we cannot read AS TEXT still has a path, and for
                // the non-source trees the path is the fact. 261 screenshots and
                // other binaries carried no domain for exactly this reason: the
                // read failed, so nothing below ever ran and the row was written
                // with no hasDomain at all. Silent, and invisible in the counts.
                let text = std::fs::read_to_string(std::path::Path::new(&root).join(path));
                if text.is_err() {
                    // #4222 — the SAME path chain a readable file gets: its own
                    // name, then the tree, then the authored directory rows.
                    // place_by_tree alone covers only three trees, so 362 images
                    // under platform/ and designing/ still carried nothing.
                    if let Some(s) = domain::place_by_file_name(path, &valid_domains)
                        .or_else(|| domain::place_by_tree(path, &valid_domains))
                        .or_else(|| domain::place_by_dir(path, &valid_domains, &dir_rows))
                    {
                        owned.push(("hasDomain".to_string(), s.domain));
                    }
                }
                if let Ok(content) = text
                {
                    let unit = domain::declared_unit(path, &|q: &str| {
                        std::fs::read_to_string(std::path::Path::new(&root).join(q)).ok()
                    });
                    let placement = domain::place_in_file(
                        &content,
                        path,
                        unit.as_deref(),
                        &unit_rows,
                        &dir_rows,
                        &valid_domains,
                        &card_domain,
                        &|q: &str| {
                            std::fs::read_to_string(std::path::Path::new(&root).join(q)).ok()
                        },
                    );
                    owned.extend(domain_fields(&placement));
                }
                let mut fields = merge_row(existing, &owned);
                let name = stable_name(path);
                // The door names the prefix its mint adds; a refusal that names
                // one is retried once with those values bared, never guessed at
                // from a table kept in step by hand — and once learned, the
                // prefix is stripped from every later row BEFORE its first PUT (#4192).
                prefixes.apply(&mut fields);
                // #4201 — queued; flushed writers at a time after the loop
                puts.push(PendingPut {
                    kind: "file",
                    label: path.clone(),
                    path: format!("{coll}/{name}"),
                    fields,
                });
            }
            Action::Delete { path } => {
                // #4290 — address the row by the name the door SERVES for it,
                // not the name this crawler would have minted. A row another
                // writer created (the 09-23 quartet probe) has a different
                // name, so stable_name(path) 404'd every night, the run went
                // red and the watermark held. The report named it first.
                let name = served_name_for(&graph, path).unwrap_or_else(|| stable_name(path));
                if let Err(e) = write(ident, &api, "DELETE", &format!("{coll}/{name}"), None) {
                    failed.push(format!("delete {path}: {e}"));
                }
            }
            _ => {}
        }
    }
    flush(&mut batch, &mut failed, &mut wrote);

    // ── #4185: the case rows. Files first, cases second: a case's inFile edge
    // names the CodeFile row the batch above just created.
    // The share gate (#3996) judges CORPUS shape, so it only has meaning on a
    // full walk; a delta's handful of files is below the floor by construction.
    if scope_was_full_walk(&scope) {
        let mut per_domain: Vec<(String, usize)> = Vec::new();
        let mut seen_files: Vec<&str> = Vec::new();
        for r in &parsed.desired {
            // #4201 — the tests domain is the explicit home of the unplaced, counted
            // on the line as unplaced/conflicts; it is not a corpus share to gate
            if r.covers == cases::UNPLACED_HOME || seen_files.contains(&r.file.as_str()) {
                continue;
            }
            seen_files.push(&r.file);
            match per_domain.iter_mut().find(|(d, _)| *d == r.covers) {
                Some(e) => e.1 += 1,
                None => per_domain.push((r.covers.clone(), 1)),
            }
        }
        let (cap, floor) = share_limits();
        if let Err(e) = cases::check_shares(&per_domain, cap, floor) {
            eprintln!("chorus-crawl: {e}");
            eprintln!("chorus-crawl: case rows NOT written this run — the file rows above stand");
            failed.push("covers-share gate refused the case pass".to_string());
        }
    }
    if !failed.iter().any(|f| f.contains("covers-share")) {
        let case_coll = match collection_for(&api, "Test") {
            Ok(c) => c,
            Err(e) => {
                eprintln!("chorus-crawl: {e}");
                std::process::exit(2);
            }
        };
        let case_in_graph: HashMap<&str, &CaseInGraph> =
            case_graph.iter().map(|g| (g.name.as_str(), g)).collect();
        let mut cbatch: Vec<String> = Vec::new();
        let cflush = |cbatch: &mut Vec<String>, failed: &mut Vec<String>, wrote: &mut usize| {
            if cbatch.is_empty() {
                return;
            }
            let body = format!("[{}]", cbatch.join(","));
            match write(
                ident,
                &api,
                "POST",
                &format!("{case_coll}/batch"),
                Some(&body),
            ) {
                Ok(_) => *wrote += cbatch.len(),
                Err(e) => failed.push(format!("case batch of {}: {e}", cbatch.len())),
            }
            cbatch.clear();
        };
        for a in &case_actions {
            match a {
                CaseAction::Post(row) => {
                    let body = case_row_json(&cases::case_row_name(&row.file, &row.case), row);
                    if !batch_accepts(batch_bytes(&cbatch), body.len(), BATCH_BODY_BUDGET) {
                        cflush(&mut cbatch, &mut failed, &mut wrote);
                    }
                    cbatch.push(body);
                    if cbatch.len() >= 200 {
                        cflush(&mut cbatch, &mut failed, &mut wrote);
                    }
                }
                CaseAction::Replace { name, row } => {
                    let existing: &[(String, String)] = case_in_graph
                        .get(name.as_str())
                        .map(|g| g.fields.as_slice())
                        .unwrap_or(&[]);
                    let mut fields = merge_row(existing, &row.owned_fields());
                    // #4162 — the retired fields never survive a rewrite (the migration)
                    fields.retain(|(k, _)| !cases::RETIRED_TEST_FIELDS.contains(&k.as_str()));
                    // #4201 — an untagged file must not keep a folder-era covers
                    if row.covers.is_empty() {
                        fields.retain(|(k, _)| k != "covers");
                    }
                    prefixes.apply(&mut fields);
                    puts.push(PendingPut {
                        kind: "case",
                        label: format!("{} :: {}", row.file, row.case),
                        path: format!("{case_coll}/{name}"),
                        fields,
                    });
                }
                CaseAction::Delete { name, file, case } => {
                    // #4310 — a case's results go first, through the same door.
                    // Deleting only the Test left 746 TestResults pointing at 103
                    // missing tests (2026-09-25). If the lookup or any result
                    // delete fails, the case stays, so nothing dangles.
                    // #4185 reworks case writes/deletes here: keep this cascade.
                    match results_of_case(&name) {
                        Ok(results) => {
                            let res_coll = collection_for(&api, "TestResult");
                            let mut all_gone = true;
                            for r in &results {
                                let del = res_coll.as_ref().map_err(|e| e.clone()).and_then(|c| {
                                    write(ident, &api, "DELETE", &format!("{c}/{r}"), None)
                                });
                                if let Err(e) = del {
                                    all_gone = false;
                                    failed.push(format!("delete result {r} of {file} :: {case}: {e}"));
                                }
                            }
                            if all_gone {
                                if let Err(e) =
                                    write(ident, &api, "DELETE", &format!("{case_coll}/{name}"), None)
                                {
                                    failed.push(format!("delete case {file} :: {case}: {e}"));
                                }
                            }
                        }
                        Err(e) => failed.push(format!(
                            "delete case {file} :: {case}: results lookup failed, case kept: {e}"
                        )),
                    }
                }
                CaseAction::Unchanged { .. } => {}
            }
        }
        cflush(&mut cbatch, &mut failed, &mut wrote);
    }

    // ── #4199: the log rows. Same identity, same refusals, same door.
    {
        let log_coll = match collection_for(&api, "LogSource") {
            Ok(c) => c,
            Err(e) => {
                eprintln!("chorus-crawl: {e}");
                std::process::exit(2);
            }
        };
        let log_in_graph: HashMap<&str, &LogInGraph> =
            log_graph.iter().map(|g| (g.name.as_str(), g)).collect();
        let observed = now_secs();
        let machine = std::env::var("CHORUS_MACHINE").unwrap_or_else(|_| "library".to_string());
        let mut lbatch: Vec<String> = Vec::new();
        let lflush = |lbatch: &mut Vec<String>, failed: &mut Vec<String>, wrote: &mut usize| {
            if lbatch.is_empty() {
                return;
            }
            let body = format!("[{}]", lbatch.join(","));
            match write(
                ident,
                &api,
                "POST",
                &format!("{log_coll}/batch"),
                Some(&body),
            ) {
                Ok(_) => *wrote += lbatch.len(),
                Err(e) => failed.push(format!("log batch of {}: {e}", lbatch.len())),
            }
            lbatch.clear();
        };
        for a in &log_actions {
            match a {
                LogAction::Post(row) => {
                    let mut fields = vec![("name".to_string(), log_row_name(&row.path))];
                    fields.extend(row.owned_fields(&machine, observed));
                    // #4222 — the domain the log belongs to, from the job that
                    // writes it and the file's own name. No tag when neither says.
                    if let Some(d) = log_domain(&row.launchd_label, &row.path, &valid_domains, &log_file_rows) {
                        fields.push(("hasDomain".to_string(), d));
                    }
                    let body = fields_json(&fields);
                    if !batch_accepts(batch_bytes(&lbatch), body.len(), BATCH_BODY_BUDGET) {
                        lflush(&mut lbatch, &mut failed, &mut wrote);
                    }
                    lbatch.push(body);
                    if lbatch.len() >= 200 {
                        lflush(&mut lbatch, &mut failed, &mut wrote);
                    }
                }
                LogAction::Replace { name, row } => {
                    let existing: &[(String, String)] = log_in_graph
                        .get(name.as_str())
                        .map(|g| g.fields.as_slice())
                        .unwrap_or(&[]);
                    let mut owned = row.owned_fields(&machine, observed);
                    if let Some(d) = log_domain(&row.launchd_label, &row.path, &valid_domains, &log_file_rows) {
                        owned.push(("hasDomain".to_string(), d));
                    }
                    let mut fields = merge_row(existing, &owned);
                    prefixes.apply(&mut fields);
                    puts.push(PendingPut {
                        kind: "log",
                        label: row.path.clone(),
                        path: format!("{log_coll}/{name}"),
                        fields,
                    });
                }
                LogAction::Delete { name, path } => {
                    if let Err(e) =
                        write(ident, &api, "DELETE", &format!("{log_coll}/{name}"), None)
                    {
                        failed.push(format!("delete log {path}: {e}"));
                    }
                }
                LogAction::Unchanged { .. } => {}
            }
        }
        lflush(&mut lbatch, &mut failed, &mut wrote);
    }

    // ── #4214: write the folds. The plan was made above; this only performs it.
    {
        let _ = (&want_pages, &want_endpoints);
        let page_coll = collection_for(&api, "Page");
        let endpoint_coll = collection_for(&api, "Endpoint");
        match (page_coll, endpoint_coll) {
            (Ok(page_coll), Ok(endpoint_coll)) => {
                let page_in_graph: HashMap<&str, &pages::InGraph> =
                    page_graph.iter().map(|g| (g.name.as_str(), g)).collect();
                let endpoint_in_graph: HashMap<&str, &pages::InGraph> =
                    endpoint_graph.iter().map(|g| (g.name.as_str(), g)).collect();
                let mut pbatch: Vec<String> = Vec::new();
                for a in &page_plan {
                    match a {
                        pages::RowAction::Post(row) => {
                            let d = pages::page_domain(&row.route, &valid_domains)
                                .or_else(|| place_row(&root, &row.path, &unit_rows, &dir_rows, &valid_domains, &card_domain));
                            let body = fields_json(&page_fields(row, d.as_deref()));
                            if !batch_accepts(batch_bytes(&pbatch), body.len(), BATCH_BODY_BUDGET) || pbatch.len() >= 200 {
                                flush_batch(&mut pbatch, ident, &api, &page_coll, "page", &mut failed, &mut wrote);
                            }
                            pbatch.push(body);
                        }
                        pages::RowAction::Replace { name, row } => {
                            let d = pages::page_domain(&row.route, &valid_domains)
                                .or_else(|| place_row(&root, &row.path, &unit_rows, &dir_rows, &valid_domains, &card_domain));
                            let existing: &[(String, String)] = page_in_graph
                                .get(name.as_str())
                                .map(|g| g.fields.as_slice())
                                .unwrap_or(&[]);
                            let mut fields = merge_row(existing, &page_fields(row, d.as_deref()));
                            if d.is_none() {
                                fields.retain(|(k, _)| k != "hasDomain");
                            }
                            prefixes.apply(&mut fields);
                            puts.push(PendingPut { kind: "page", label: row.route.clone(), path: format!("{page_coll}/{name}"), fields });
                        }
                        pages::RowAction::Delete { name, key } => {
                            if let Err(e) = write(ident, &api, "DELETE", &format!("{page_coll}/{name}"), None) {
                                failed.push(format!("delete page {key}: {e}"));
                            }
                        }
                        pages::RowAction::Unchanged { .. } => {}
                    }
                }
                flush_batch(&mut pbatch, ident, &api, &page_coll, "page", &mut failed, &mut wrote);

                let mut ebatch: Vec<String> = Vec::new();
                for a in &endpoint_plan {
                    match a {
                        pages::RowAction::Post(row) => {
                            // #4222 — an endpoint's own route is the better
                            // signal; the file it sits in is the fallback.
                            let d = pages::endpoint_domain(&row.route_path, &valid_domains, &route_rows)
                                .or_else(|| place_row(&root, &row.path, &unit_rows, &dir_rows, &valid_domains, &card_domain));
                            let body = fields_json(&endpoint_fields(row, d.as_deref()));
                            if !batch_accepts(batch_bytes(&ebatch), body.len(), BATCH_BODY_BUDGET) || ebatch.len() >= 200 {
                                flush_batch(&mut ebatch, ident, &api, &endpoint_coll, "endpoint", &mut failed, &mut wrote);
                            }
                            ebatch.push(body);
                        }
                        pages::RowAction::Replace { name, row } => {
                            let d = pages::endpoint_domain(&row.route_path, &valid_domains, &route_rows)
                                .or_else(|| place_row(&root, &row.path, &unit_rows, &dir_rows, &valid_domains, &card_domain));
                            let existing: &[(String, String)] = endpoint_in_graph
                                .get(name.as_str())
                                .map(|g| g.fields.as_slice())
                                .unwrap_or(&[]);
                            let mut fields = merge_row(existing, &endpoint_fields(row, d.as_deref()));
                            if d.is_none() {
                                fields.retain(|(k, _)| k != "hasDomain");
                            }
                            prefixes.apply(&mut fields);
                            puts.push(PendingPut { kind: "endpoint", label: format!("{} {}", row.http_method, row.route_path), path: format!("{endpoint_coll}/{name}"), fields });
                        }
                        pages::RowAction::Delete { name, key } => {
                            if let Err(e) = write(ident, &api, "DELETE", &format!("{endpoint_coll}/{name}"), None) {
                                failed.push(format!("delete endpoint {key}: {e}"));
                            }
                        }
                        pages::RowAction::Unchanged { .. } => {}
                    }
                }
                flush_batch(&mut ebatch, ident, &api, &endpoint_coll, "endpoint", &mut failed, &mut wrote);
            }
            (p, e) => {
                if let Err(err) = p { eprintln!("chorus-crawl: Page leg SKIPPED — {err}"); }
                if let Err(err) = e { eprintln!("chorus-crawl: Endpoint leg SKIPPED — {err}"); }
            }
        }
    }

    flush_puts(&mut puts, ident, &api, &mut prefixes, writers, &mut failed, &mut wrote);

    let elapsed = started.elapsed().as_secs_f64();
    let attempted = wrote + failed.len();
    println!(
        "chorus-crawl: wrote={} failed={} · elapsed={:.0}s rate={:.1}/s mints={}",
        wrote,
        failed.len(),
        elapsed,
        if elapsed > 0.0 {
            attempted as f64 / elapsed
        } else {
            0.0
        },
        ident.borrow().mints
    );

    // The watermark moves only when this run earned it.
    let scope_was_full = matches!(scope, Scope::Full { .. });
    // #4199 — a FULL pass that wrote clean re-reads the graph and prints the
    // morning line, so the nightly log carries complete / current / consistent /
    // lossless and the readout's TOTAL can quote it. A pass with failures says so
    // above and skips the grading — a red run does not also grade itself clean.
    if scope_was_full && failed.is_empty() && !mass_delete {
        let all_test_files: Vec<String> = test_files
            .iter()
            .map(|s| s.to_string())
            .filter(|p| registers_cases(p))
            .collect();
        match (
            existing_rows(&api, &token),
            existing_case_rows(&api, &token),
        ) {
            (Ok(g2), Ok(cg2)) => {
                print_graph_vs_project(
                    &root,
                    &api,
                    &token,
                    &disk,
                    &g2,
                    &parsed.desired,
                    &all_test_files,
                    &cg2,
                    &head,
                    Some(&head),
                    &parsed.no_case_buckets,
                    &parsed.no_case,
                    &parsed.tags,
                );
            }
            (Err(e), _) | (_, Err(e)) => println!(
                "chorus-crawl: graph vs project: UNMEASURED — re-read after the pass failed ({e})"
            ),
        }
    }

    // a refused mass delete means this graph does NOT match this commit
    let read_for_watermark = if mass_delete {
        TreeRead::Partial
    } else {
        case_read
    };
    match watermark_after(&head, read_for_watermark, failed.len(), scope_was_full) {
        Watermark::Advance(sha) => {
            println!("chorus-crawl: watermark -> {}", &sha[..sha.len().min(9)]);
            if let Err(e) = std::fs::write(format!("{root}/.chorus-crawl-watermark"), &sha) {
                eprintln!(
                    "chorus-crawl: could not record the watermark ({e}) — next run walks full"
                );
            }
        }
        Watermark::Hold(why) => {
            println!("chorus-crawl: watermark HELD — {why}");
        }
    }

    if !failed.is_empty() {
        for f in failed.iter().take(5) {
            eprintln!("chorus-crawl: FAILED {f}");
        }
        // #4201 — five named lines out of thousands says nothing about WHY.
        // Every failure is counted by its verb, route family and status, so
        // the run names its classes, not a sample of them.
        for (class, n) in failure_classes(&failed) {
            eprintln!("chorus-crawl: FAILED x{n} — {class}");
        }
        eprintln!(
            "chorus-crawl: {} write(s) failed — the run is RED, not partially green",
            failed.len()
        );
        std::process::exit(1);
    }
}

/// Each failure reduced to `VERB /route/family -> STATUS`, counted, most first.
/// A failure line the crawler prints reads like
/// `delete case path :: name: DELETE /tests/tests/<id> -> HTTP 403`.
fn failure_classes(failed: &[String]) -> Vec<(String, usize)> {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for f in failed {
        // The verb must be followed by an actual path. A case NAME can contain
        // the word DELETE ("a DELETE fires the guard") and did, inventing five
        // one-off classes in the first run that used this.
        let w: Vec<&str> = f.split_whitespace().collect();
        let verb_route = w
            .iter()
            .enumerate()
            .position(|(i, x)| {
                matches!(*x, "POST" | "PUT" | "DELETE" | "PATCH")
                    && w.get(i + 1).is_some_and(|r| r.starts_with('/'))
            })
            .map(|i| {
                let family: String = w[i + 1].split('/').take(3).collect::<Vec<_>>().join("/");
                format!("{} {}", w[i], family)
            })
            .unwrap_or_else(|| "(no route in line)".to_string());
        let status = f
            .split("HTTP ")
            .nth(1)
            .map(|t| t.chars().take_while(char::is_ascii_digit).collect::<String>())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "?".to_string());
        *counts.entry(format!("{verb_route} -> HTTP {status}")).or_default() += 1;
    }
    let mut out: Vec<(String, usize)> = counts.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}

/// The principal this pass writes as. #4210 — a default here INVENTS one: every
/// row the nightly wrote landed owned by `crawler`, and Silas hand-moved them
/// three times on 2026-09-18 alone. The caller says who it is or the pass
/// refuses; an unset variable is not a licence to pick a name.
fn declared_role(env: Option<String>) -> Result<String, String> {
    match env {
        Some(r) if !r.trim().is_empty() => Ok(r.trim().to_string()),
        _ => Err("CHORUS_ROLE is unset — refusing to write as an invented principal. \
                  Set it to the principal that OWNS these rows (the plist's \
                  EnvironmentVariables)."
            .to_string()),
    }
}

#[cfg(test)]
mod declared_role_4210 {
    use super::declared_role;

    /// NEGATIVE PROOF: the state that cost three hand-migrations in one day.
    #[test]
    fn an_unset_role_refuses_instead_of_inventing_one() {
        assert!(declared_role(None).is_err());
        assert!(declared_role(Some("   ".to_string())).is_err());
        let why = declared_role(None).unwrap_err();
        assert!(why.contains("CHORUS_ROLE"), "{why}");
        assert!(!why.contains("crawler"), "the refusal must not suggest a name: {why}");
    }

    #[test]
    fn a_declared_role_is_taken_as_given() {
        assert_eq!(declared_role(Some(" kade ".to_string())).unwrap(), "kade");
    }
}

/// The joined size of a batch body so far (rows plus the commas between them).
fn batch_bytes(batch: &[String]) -> usize {
    batch.iter().map(|b| b.len()).sum::<usize>() + batch.len().saturating_sub(1)
}

/// #4199 — the graph-vs-project verdicts, printed after a FULL read of tree and
/// graph. Read-only: nothing here writes. Returns whether everything was clean.
#[allow(clippy::too_many_arguments)]
fn print_graph_vs_project(
    root: &str,
    api: &str,
    token: &str,
    disk: &[OnDisk],
    graph: &[InGraph],
    desired: &[CaseRow],
    test_files: &[String],
    case_graph: &[CaseInGraph],
    head: &str,
    watermark: Option<&str>,
    no_case_buckets: &[&str],
    no_case: &[String],
    tags: &domain::TagCounts,
) -> bool {
    let drift = reconcile(disk, graph);
    println!("chorus-crawl: {}", drift.report());
    let case_drift = cases::reconcile_cases(desired, test_files, case_graph);
    println!("chorus-crawl: {}", case_drift.report());

    // logs: what the box writes vs what the logs domain holds (the same walk the log leg writes from)
    let log_files: Vec<String> = box_log_files(root).into_iter().map(|f| f.path).collect();
    let mut logs_measured = true;
    let log_rows: Vec<String> = match fetch_rows(api, token, "LogSource") {
        Ok(rows) => rows
            .into_iter()
            .filter_map(|f| f.into_iter().find(|(k, _)| k == "logPath").map(|(_, v)| v))
            .filter(|v| !v.is_empty())
            .collect(),
        Err(e) => {
            println!("chorus-crawl: reconcile logs: UNMEASURED — cannot read LogSource rows ({e})");
            logs_measured = false;
            Vec::new()
        }
    };
    let on_disk = |p: &str| std::path::Path::new(p).is_file();
    let log_drift = reconcile_logs(&log_files, &log_rows, &on_disk);
    println!("chorus-crawl: {}", log_drift.report());

    // #4290 — the fold rows of the control record, filled by the loop below.
    let mut fold_rows: Vec<chorus_crawl::ValidateRow> = Vec::new();
    // #4334 — every bats suite declares the domain it guards
    {
        let read = |q: &str| std::fs::read_to_string(std::path::Path::new(root).join(q)).ok();
        let (n, missing) = domain::undeclared_bats(test_files, &read);
        if missing.is_empty() {
            println!("chorus-crawl: reconcile declared domains: clean — {n} bats suites, each with an @domain header");
        } else {
            println!("chorus-crawl: reconcile declared domains: DRIFT — {} of {n} bats suites carry no @domain header", missing.len());
        }
        fold_rows.push(chorus_crawl::ValidateRow {
            domain: "tests".to_string(),
            class: "Test (declared domain)".to_string(),
            tree: n,
            graph: n - missing.len(),
            missing,
            stale: Vec::new(),
            excluded: Vec::new(),
            measured: true,
        });
    }
    // #4214 — the folds, both directions. The classes shipped without this, so
    // for one afternoon nothing proved that every page on disk had a row or that
    // every row still had a file. Jeff caught it by asking the obvious question.
    {
        let read_file = |q: &str| std::fs::read_to_string(std::path::Path::new(root).join(q)).ok();
        let tracked = tracked_files(root).unwrap_or_default();
        let (want_pages, want_endpoints, _) = pages::desired_rows(
            &tracked,
            &read_file,
            std::path::Path::new(GATHERING_ROOT).is_dir(),
        );
        for (kind, key_field, want) in [
            (
                "pages",
                "route",
                want_pages.iter().map(|r| r.route.clone()).collect::<Vec<_>>(),
            ),
            (
                "endpoints",
                "routePath",
                want_endpoints
                    .iter()
                    .map(|r| format!("{} {}", r.http_method, r.route_path))
                    .collect::<Vec<_>>(),
            ),
        ] {
            let class = if kind == "pages" { "Page" } else { "Endpoint" };
            match rows_as_in_graph(api, token, class, key_field) {
                Err(e) => {
                    println!(
                        "chorus-crawl: reconcile {kind}: UNMEASURED — cannot read {class} rows ({e})"
                    );
                    fold_rows.push(chorus_crawl::ValidateRow {
                        domain: kind.to_string(),
                        class: class.to_string(),
                        tree: want.len(),
                        graph: 0,
                        missing: Vec::new(),
                        stale: Vec::new(),
                        excluded: Vec::new(),
                        measured: false,
                    });
                }
                Ok(rows) => {
                    let have: Vec<String> = rows.iter().map(|r| r.key.clone()).collect();
                    let missing: Vec<&String> = want.iter().filter(|k| !have.contains(k)).collect();
                    let orphan: Vec<&String> = have.iter().filter(|k| !want.contains(k)).collect();
                    fold_rows.push(chorus_crawl::ValidateRow {
                        domain: kind.to_string(),
                        class: class.to_string(),
                        tree: want.len(),
                        graph: have.len(),
                        missing: missing.iter().map(|s| s.to_string()).collect(),
                        stale: orphan.iter().map(|s| s.to_string()).collect(),
                        excluded: Vec::new(),
                        measured: true,
                    });
                    if missing.is_empty() && orphan.is_empty() {
                        println!(
                            "chorus-crawl: reconcile {kind}: clean — {} in the tree, {} in the graph, same set",
                            want.len(),
                            have.len()
                        );
                    } else {
                        println!(
                            "chorus-crawl: reconcile {kind}: DRIFT — {} in the tree without a row, {} rows without a source",
                            missing.len(),
                            orphan.len()
                        );
                        for k in missing.iter().take(10) {
                            println!("chorus-crawl:   no row for {k}");
                        }
                        for k in orphan.iter().take(10) {
                            println!("chorus-crawl:   no source for {k}");
                        }
                    }
                }
            }
        }
    }

    // current: commits between the watermark and HEAD
    let lag = match watermark {
        Some(w) => sh(
            "git",
            &["rev-list", "--count", &format!("{w}..{head}")],
            root,
        )
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .unwrap_or(0),
        None => 0,
    };
    // lossless: every land since the first pass in the nightly log is covered by a pass
    let crawl_log = std::env::var("CRAWL_LOG").unwrap_or_else(|_| {
        format!(
            "{}/Library/Logs/Chorus/crawl-nightly.log",
            std::env::var("HOME").unwrap_or_default()
        )
    });
    let log_text = std::fs::read_to_string(&crawl_log).unwrap_or_default();
    let wms = passes_watermarks(&log_text);
    let (lands, uncovered) = match wms.first() {
        Some(first) => {
            let raw = sh(
                "git",
                &[
                    "log",
                    "--first-parent",
                    "--format=%h %s",
                    &format!("{first}..{head}"),
                ],
                root,
            )
            .unwrap_or_default();
            let lands: Vec<String> = raw
                .lines()
                .filter(|l| {
                    l.split_once(' ')
                        .map(|(_, s)| s.starts_with('#'))
                        .unwrap_or(false)
                })
                .map(|l| l.split_whitespace().next().unwrap_or("").to_string())
                .collect();
            let anc =
                |a: &str, b: &str| sh("git", &["merge-base", "--is-ancestor", a, b], root).is_ok();
            let unc = uncovered_lands(&lands, &wms, &anc);
            (lands.len(), unc)
        }
        None => (0, Vec::new()),
    };
    let skipped: Vec<String> = disk
        .iter()
        .filter(|f| !f.classified)
        .map(|f| f.path.clone())
        .collect();
    let (no_kind, top) = no_kind_summary(&skipped);
    let (no_case_missing, no_case_detail) = cases::no_case_summary(no_case_buckets);
    let line = ProjectLine {
        file_rows: graph.len(),
        tracked: disk.len(),
        no_kind,
        no_kind_top: top,
        case_rows: case_graph.len(),
        no_case: no_case_missing,
        no_case_detail: no_case_detail.clone(),
        log_rows: log_rows.len(),
        log_files: log_files.len(),
        lag_commits: lag,
        files_drift: drift.missing_from_graph.len()
            + drift.missing_from_tree.len()
            + drift.stale_sha.len(),
        cases_drift: case_drift.rows_without_file.len()
            + case_drift.files_without_rows.len()
            + case_drift.cases_without_rows.len()
            + case_drift.rows_without_case.len(),
        logs_drift: log_drift.files_without_rows.len() + log_drift.rows_without_files.len(),
        lands,
        uncovered,
        tag_placed: tags.placed,
        tag_conflicts: tags.conflicts,
        tag_unplaced: tags.unplaced,
    };
    println!("chorus-crawl: {}", line.render());

    // #4290 — the control record: one row per crawler-generated domain, both
    // directions, names on both sides, kept per run. The nightly lane reads
    // the VALIDATE| lines; the /crawler-validate page reads the kept JSON.
    let case_files_in_graph = {
        let mut v: Vec<&str> = case_graph.iter().map(|g| g.file.as_str()).collect();
        v.sort_unstable();
        v.dedup();
        v.len()
    };
    let mut rows = vec![
        chorus_crawl::ValidateRow {
            domain: "code".into(),
            class: "CodeFile".into(),
            tree: disk.iter().filter(|f| f.classified).count(),
            graph: graph.len(),
            missing: drift.missing_from_graph.clone(),
            stale: drift
                .missing_from_tree
                .iter()
                .cloned()
                .chain(drift.stale_sha.iter().map(|p| format!("{p} (stale sha)")))
                .collect(),
            excluded: Vec::new(),
            measured: true,
        },
        chorus_crawl::ValidateRow {
            domain: "tests".into(),
            class: "Test (files)".into(),
            tree: test_files.len(),
            graph: case_files_in_graph,
            // a test file in git with no row is a gap whether the crawler
            // could not extract its cases or simply never wrote them — the
            // bucket says which (#4199). The one exception is a file that
            // holds no test at all (bucket no-tests): listed, not counted.
            missing: case_drift
                .files_without_rows
                .iter()
                .cloned()
                .chain(
                    no_case
                        .iter()
                        .zip(no_case_buckets.iter())
                        .filter(|(_, b)| **b != "no-tests")
                        .map(|(f, b)| format!("{f} (no case extracted: {b})")),
                )
                .collect(),
            excluded: no_case
                .iter()
                .zip(no_case_buckets.iter())
                .filter(|(_, b)| **b == "no-tests")
                .map(|(f, _)| format!("{f} (contains no test)"))
                .collect(),
            stale: case_drift.rows_without_file.clone(),
            measured: true,
        },
        chorus_crawl::ValidateRow {
            domain: "tests".into(),
            class: "Test (cases)".into(),
            tree: desired.len(),
            graph: case_graph.len(),
            missing: case_drift.cases_without_rows.clone(),
            stale: case_drift.rows_without_case.clone(),
            excluded: Vec::new(),
            measured: true,
        },
        chorus_crawl::ValidateRow {
            domain: "logs".into(),
            class: "LogSource".into(),
            tree: log_files.len(),
            graph: log_rows.len(),
            missing: log_drift.files_without_rows.clone(),
            stale: log_drift.rows_without_files.clone(),
            excluded: Vec::new(),
            measured: logs_measured,
        },
    ];
    rows.extend(fold_rows);
    let rec = chorus_crawl::ValidateRecord {
        ts: sh("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"], root).unwrap_or_default().trim().to_string(),
        head: head.to_string(),
        head_time: sh("git", &["show", "-s", "--format=%cI", head], root).unwrap_or_default().trim().to_string(),
        watermark: watermark.unwrap_or("").to_string(),
        crawled_at: std::fs::metadata(&crawl_log)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .and_then(|d| {
                sh("date", &["-u", "-r", &d.as_secs().to_string(), "+%Y-%m-%dT%H:%M:%SZ"], root).ok()
            })
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
        rows,
    };
    for l in rec.lines() {
        println!("{l}");
    }
    let dir = chorus_crawl::validate_dir();
    match chorus_crawl::write_validate_record(&dir, &rec) {
        Ok(kept) => println!(
            "chorus-crawl: validate {} — {} gap(s) across {} row(s) → {kept}",
            if rec.is_clean() { "clean" } else { "DRIFT" },
            rec.gaps(),
            rec.rows.len()
        ),
        Err(e) => println!("chorus-crawl: validate record NOT kept ({e}) — the page will not see this run"),
    }
    drift.is_clean() && case_drift.is_clean() && log_drift.is_clean() && line.is_clean() && rec.is_clean()
}

fn scope_was_full_walk(scope: &Scope) -> bool {
    matches!(scope, Scope::Full { .. })
}

/// A flat field map as a JSON object body.
/// #4222 — the hasDomain fields a row carries: EVERY domain the rules found.
///
/// Pure, so the choice between "one answer" and "all of them" is testable. The
/// write site used `.domain()`, which returns None the moment two rules
/// disagree — 217 files that genuinely serve several domains were stored with
/// no domain at all rather than with all of theirs.
fn domain_fields(placement: &chorus_crawl::domain::Placement) -> Vec<(String, String)> {
    placement
        .domains()
        .into_iter()
        .map(|d| ("hasDomain".to_string(), d))
        .collect()
}

/// #4222 — a key repeated in `fields` is written as a JSON array, so a row can
/// carry every domain the rules found. Jeff: "its 1.n - period". The CodeFile
/// shape has minCount 1 and no maxCount, so 1..n was always legal in the store;
/// only this writer flattened it to one and dropped the rest.
fn fields_json(fields: &[(String, String)]) -> String {
    let mut order: Vec<&str> = Vec::new();
    for (k, _) in fields {
        if !order.contains(&k.as_str()) {
            order.push(k.as_str());
        }
    }
    let body: Vec<String> = order
        .iter()
        .map(|k| {
            let vals: Vec<&String> = fields
                .iter()
                .filter(|(fk, _)| fk == k)
                .map(|(_, v)| v)
                .collect();
            if vals.len() == 1 {
                format!("\"{}\":\"{}\"", json_escape(k), json_escape(vals[0]))
            } else {
                let items: Vec<String> = vals
                    .iter()
                    .map(|v| format!("\"{}\"", json_escape(v)))
                    .collect();
                format!("\"{}\":[{}]", json_escape(k), items.join(","))
            }
        })
        .collect();
    format!("{{{}}}", body.join(","))
}

#[cfg(test)]
mod failure_classes_4201 {
    use super::failure_classes;

    /// NEGATIVE PROOF: thousands of failures printed as five lines cannot tell
    /// one refused class from another. Same five names, two different causes.
    #[test]
    fn the_classes_separate_two_causes_the_sample_would_not() {
        let failed: Vec<String> = (0..3)
            .map(|i| format!("delete case a{i}.ts :: n: DELETE /tests/tests/x{i} -> HTTP 403 "))
            .chain((0..2).map(|i| {
                format!("post case b{i}.ts :: n: POST /tests/results -> HTTP 422 ")
            }))
            .collect();
        let classes = failure_classes(&failed);
        assert_eq!(
            classes,
            vec![
                ("DELETE /tests/tests -> HTTP 403".to_string(), 3),
                ("POST /tests/results -> HTTP 422".to_string(), 2),
            ]
        );
    }

    /// CONTROL: a line with no route still counts, and says so, rather than
    /// vanishing from the total.
    /// NEGATIVE PROOF: a case NAME containing the word DELETE. The first run
    /// that used these classes invented five one-off entries from lines like
    /// "a DELETE fires the guard" — the verb was real, the next word was not a
    /// route.
    #[test]
    fn a_verb_inside_a_case_name_is_not_a_route() {
        let failed = vec![
            "delete case a.ts :: a DELETE fires the guard: DELETE /tests/tests/x -> HTTP 403 "
                .to_string(),
        ];
        assert_eq!(
            failure_classes(&failed),
            vec![("DELETE /tests/tests -> HTTP 403".to_string(), 1)]
        );
    }

    #[test]
    fn a_line_with_no_route_is_still_counted() {
        let failed = vec!["could not serialise row".to_string()];
        assert_eq!(
            failure_classes(&failed),
            vec![("(no route in line) -> HTTP ?".to_string(), 1)]
        );
    }
}

/// #4222 — 1..n domains, end to end through the writer.
///
/// Jeff, after the fourth time: "its 1.n - period". The store always allowed it
/// (CodeFile's hasDomain is minCount 1, no maxCount) and a live PUT with four
/// domains returned 200. These pin the two places that flattened it.
#[cfg(test)]
mod multi_domain_4222 {
    use super::fields_json;
    use chorus_crawl::merge_row;

    fn f(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    /// NEGATIVE PROOF: with the old one-value-per-key writer this body was
    /// `{"hasDomain":"tests"}` — three of the four domains silently dropped.
    #[test]
    fn a_repeated_key_is_written_as_an_array() {
        let body = fields_json(&f(&[
            ("filePath", "a.rs"),
            ("hasDomain", "code"),
            ("hasDomain", "logs"),
            ("hasDomain", "tests"),
        ]));
        assert_eq!(body, r#"{"filePath":"a.rs","hasDomain":["code","logs","tests"]}"#);
    }

    /// One value still writes a plain string — the 6,000 single-domain rows
    /// must not change shape.
    #[test]
    fn one_value_stays_a_string() {
        let body = fields_json(&f(&[("hasDomain", "code")]));
        assert_eq!(body, r#"{"hasDomain":"code"}"#);
    }

    /// NEGATIVE PROOF: the old merge kept the first slot and overwrote it, so a
    /// row that used to be `logs` and is now `code`+`tests` came out as one
    /// value. All the caller's values replace all the stored ones.
    #[test]
    fn a_multi_valued_key_replaces_every_stored_value() {
        let out = merge_row(
            &f(&[("filePath", "a.rs"), ("hasDomain", "logs")]),
            &f(&[("hasDomain", "code"), ("hasDomain", "tests")]),
        );
        let domains: Vec<&str> = out
            .iter()
            .filter(|(k, _)| k == "hasDomain")
            .map(|(_, v)| v.as_str())
            .collect();
        assert_eq!(domains, vec!["code", "tests"]);
        assert!(out.iter().any(|(k, v)| k == "filePath" && v == "a.rs"));
    }

    /// A single supplied value still overwrites in place, one slot.
    #[test]
    fn a_single_valued_key_still_overwrites_in_place() {
        let out = merge_row(
            &f(&[("hasDomain", "logs")]),
            &f(&[("hasDomain", "code")]),
        );
        assert_eq!(out, f(&[("hasDomain", "code")]));
    }

    /// NEGATIVE PROOF: with `.domain()` at the write site a conflicted file
    /// contributed NO hasDomain field at all. This is the 217-file case.
    #[test]
    fn a_conflicted_file_contributes_every_domain_it_serves() {
        use chorus_crawl::domain::{Placement, Rule, Signal};
        let sig = |d: &str| Signal {
            rule: Rule::Route,
            domain: d.to_string(),
            evidence: format!("route {d}"),
        };
        let p = Placement::Conflict {
            signals: vec![sig("tests"), sig("code"), sig("logs")],
        };
        assert_eq!(
            super::domain_fields(&p),
            f(&[
                ("hasDomain", "code"),
                ("hasDomain", "logs"),
                ("hasDomain", "tests"),
            ])
        );
    }

    /// An agreed placement is still exactly one field.
    #[test]
    fn an_agreed_file_contributes_one_domain() {
        use chorus_crawl::domain::Placement;
        let p = Placement::Tagged {
            domain: "code".to_string(),
            signals: vec![],
        };
        assert_eq!(super::domain_fields(&p), f(&[("hasDomain", "code")]));
    }
}
