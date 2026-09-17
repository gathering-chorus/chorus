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
        // Only .rs files are opened at all, and only to answer "does this
        // declare tests" — every other classification is a pure path decision.
        let is_rs = rel.ends_with(".rs");
        let verdict = classify(rel, is_rs && rust_declares_tests(&format!("{root}/{rel}")));
        let classified = matches!(verdict, Verdict::Classified(..));
        let sha = match hashes.get(rel) {
            Some(s) => s.clone(),
            None => {
                // Tracked but git has no hash for it in this index. A fact
                // about this run, not about the repo — refuse deletes. Partial
                // outranks Scoped: a failed read is worse than a narrow one.
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
    !path.contains("platform/services/shared/")
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
    declared: usize,
    inferred: usize,
    complete: bool,
}

fn parse_cases(root: &str, test_files: &[&str]) -> Parsed {
    let mut p = Parsed {
        desired: Vec::new(),
        parsed_files: Vec::new(),
        no_case: Vec::new(),
        declared: 0,
        inferred: 0,
        complete: true,
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
            continue;
        }
        let fc = cases::file_class(path, &content);
        if fc.declared {
            p.declared += 1
        } else {
            p.inferred += 1
        }
        let covers = cases::covers_with_concern(path, fc.concern).to_string();
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
            let path = arg(2);
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            println!(
                "{}",
                cases::covers_with_concern(&path, cases::file_class(&path, &content).concern)
            );
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

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    if seam(&argv) {
        return;
    }
    let root = std::env::var("CHORUS_ROOT").unwrap_or_else(|_| ".".to_string());
    let dry_run = std::env::args().any(|a| a == "--dry-run");
    let reconciling = std::env::args().any(|a| a == "--reconcile");

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
    let role = std::env::var("CHORUS_ROLE").unwrap_or_else(|_| "crawler".to_string());

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
    let c = counts(&actions);

    // #4185 — the case pass: every kind=test file this run walked, parsed.
    let test_files: Vec<&str> = disk
        .iter()
        .filter(|f| f.classified)
        .filter(|f| {
            matches!(
                classify(
                    &f.path,
                    f.path.ends_with(".rs") && rust_declares_tests(&format!("{root}/{}", f.path))
                ),
                Verdict::Classified(Kind::Test, _)
            )
        })
        .map(|f| f.path.as_str())
        .collect();
    let parsed = parse_cases(&root, &test_files);
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
        if dry_run {
            "  (dry-run — nothing written)"
        } else {
            ""
        }
    );
    println!(
        "chorus-crawl: cases posted={} replaced={} unchanged={} deleted={} · test files parsed={} declared={} inferred={} no-case={}{}",
        cc.posted, cc.replaced, cc.unchanged, cc.deleted,
        parsed.parsed_files.len(), parsed.declared, parsed.inferred, parsed.no_case.len(),
        if dry_run { "  (dry-run — nothing written)" } else { "" }
    );
    if !parsed.no_case.is_empty() {
        println!("chorus-crawl: {}", cases::no_case_report(&parsed.no_case));
    }
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
        let drift = reconcile(&disk, &graph);
        // #4180 — the counts line above reads exactly like a run's outcome, and
        // --reconcile writes NOTHING: it exited here with "posted=1" on the line
        // and the row still absent, and I read that as work done. Say what the
        // numbers are.
        if c.posted + c.replaced + c.deleted > 0 {
            println!(
                "chorus-crawl: reconcile is read-only — the {} post / {} replace / {} delete above are what a write pass WOULD do, not what happened",
                c.posted, c.replaced, c.deleted
            );
        }
        println!("chorus-crawl: {}", drift.report());
        let all_test_files: Vec<String> = test_files
            .iter()
            .map(|s| s.to_string())
            .filter(|p| registers_cases(p))
            .collect();
        let case_drift = cases::reconcile_cases(&parsed.desired, &all_test_files, &case_graph);
        println!("chorus-crawl: {}", case_drift.report());
        std::process::exit(if drift.is_clean() && case_drift.is_clean() {
            0
        } else {
            1
        });
    }

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
    let mut wrote = 0usize;
    let mut failed: Vec<String> = Vec::new();
    let mut batch: Vec<String> = Vec::new();

    let ident = ident.as_ref().expect("writes happen only with an identity");
    let mut prefixes = PrefixMemory::default();
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
                let is_rs = path.ends_with(".rs");
                if let Verdict::Classified(k, l) = classify(
                    path,
                    is_rs && rust_declares_tests(&format!("{root}/{path}")),
                ) {
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
                let is_rs = path.ends_with(".rs");
                let Verdict::Classified(k, l) = classify(
                    path,
                    is_rs && rust_declares_tests(&format!("{root}/{path}")),
                ) else {
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
                let mut fields = merge_row(existing, &owned);
                let name = stable_name(path);
                // The door names the prefix its mint adds; a refusal that names
                // one is retried once with those values bared, never guessed at
                // from a table kept in step by hand — and once learned, the
                // prefix is stripped from every later row BEFORE its first PUT (#4192).
                prefixes.apply(&mut fields);
                let mut attempt = 0;
                loop {
                    let body = fields_json(&fields);
                    match write(ident, &api, "PUT", &format!("{coll}/{name}"), Some(&body)) {
                        Ok(_) => {
                            wrote += 1;
                            break;
                        }
                        Err(e)
                            if attempt == 0
                                && prefixes.learn(&e)
                                && prefixes.apply(&mut fields) =>
                        {
                            attempt += 1
                        }
                        Err(e) => {
                            failed.push(format!("update {path}: {e}"));
                            break;
                        }
                    }
                }
            }
            Action::Delete { path } => {
                let name = stable_name(path);
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
            if seen_files.contains(&r.file.as_str()) {
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
                    // testConcern is OPTIONAL: an absent one must not survive as the old value
                    if row.concern.is_none() {
                        fields.retain(|(k, _)| k != "testConcern");
                    }
                    prefixes.apply(&mut fields);
                    let mut attempt = 0;
                    loop {
                        let body = fields_json(&fields);
                        match write(
                            ident,
                            &api,
                            "PUT",
                            &format!("{case_coll}/{name}"),
                            Some(&body),
                        ) {
                            Ok(_) => {
                                wrote += 1;
                                break;
                            }
                            Err(e)
                                if attempt == 0
                                    && prefixes.learn(&e)
                                    && prefixes.apply(&mut fields) =>
                            {
                                attempt += 1
                            }
                            Err(e) => {
                                failed
                                    .push(format!("update case {} :: {}: {e}", row.file, row.case));
                                break;
                            }
                        }
                    }
                }
                CaseAction::Delete { name, file, case } => {
                    if let Err(e) =
                        write(ident, &api, "DELETE", &format!("{case_coll}/{name}"), None)
                    {
                        failed.push(format!("delete case {file} :: {case}: {e}"));
                    }
                }
                CaseAction::Unchanged { .. } => {}
            }
        }
        cflush(&mut cbatch, &mut failed, &mut wrote);
    }

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
        eprintln!(
            "chorus-crawl: {} write(s) failed — the run is RED, not partially green",
            failed.len()
        );
        std::process::exit(1);
    }
}

/// The joined size of a batch body so far (rows plus the commas between them).
fn batch_bytes(batch: &[String]) -> usize {
    batch.iter().map(|b| b.len()).sum::<usize>() + batch.len().saturating_sub(1)
}

fn scope_was_full_walk(scope: &Scope) -> bool {
    matches!(scope, Scope::Full { .. })
}

/// A flat field map as a JSON object body.
fn fields_json(fields: &[(String, String)]) -> String {
    let body: Vec<String> = fields
        .iter()
        .map(|(k, v)| format!("\"{}\":\"{}\"", json_escape(k), json_escape(v)))
        .collect();
    format!("{{{}}}", body.join(","))
}
