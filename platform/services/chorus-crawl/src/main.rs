//! chorus-crawl — the one herald (#4173).
//!
//! Walks the repo, keeps a row per tracked file in the graph through the
//! generated door, and says what it did. Zero external crates: `git` and
//! `curl` as subprocesses, std for the rest.
//!
//! Every decision this binary makes lives in lib.rs as a pure function. main
//! is I/O and reporting only — so the rules are testable without a repo, a
//! server, or a clock.

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
        return Err(format!("{cmd} {:?} exited {}: {}", args, out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stderr).trim()));
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
    sh("git", &["cat-file", "-e", &format!("{sha}^{{commit}}")], root).is_ok()
}

fn changes_since(root: &str, from: &str, to: &str) -> Result<Vec<Change>, String> {
    let raw = sh("git", &["diff", "--name-status", "-M", &format!("{from}..{to}")], root)?;
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
    std::fs::read_to_string(abs).map(|s| s.contains("#[test]") || s.contains("#[cfg(test)]")).unwrap_or(false)
}

/// The file list the tree reports, classified. Returns the reading quality
/// alongside: if any file could not be read, deletes are refused for this run.
fn read_tree(root: &str, paths: &[String], hashes: &HashMap<String, String>, scoped: bool) -> (Vec<OnDisk>, TreeRead) {
    let mut out = Vec::new();
    let mut complete = if scoped { TreeRead::Scoped } else { TreeRead::Complete };
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
        out.push(OnDisk { path: rel.clone(), sha, classified });
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
    let at = doc.find(&needle).ok_or_else(|| format!("discovery does not serve {kind} — refusing to guess a route"))?;
    let tail = &doc[at..];
    let key = "\"collection\": \"";
    let cs = tail.find(key).ok_or("discovery row has no collection")? + key.len();
    let ce = tail[cs..].find('"').ok_or("unterminated collection")? + cs;
    Ok(tail[cs..ce].trim_start_matches("/v1").to_string())
}

/// This run's identity. The crawler holds its own credential (principal-crawler,
/// #4154) — it never reaches for shared admin, which is the thing that makes a
/// write attributable at all.
fn identity_token(root: &str, role: &str) -> Result<String, String> {
    if let Ok(t) = std::env::var("CHORUS_IDENTITY_TOKEN") {
        if !t.trim().is_empty() {
            return Ok(t);
        }
    }
    let script = format!("{root}/platform/scripts/chorus-identity-token");
    let t = sh(&script, &[role], root)?.trim().to_string();
    if t.is_empty() {
        return Err(format!("no identity token for {role}"));
    }
    Ok(t)
}

fn curl(api: &str, method: &str, path: &str, body: Option<&str>, token: Option<&str>) -> Result<String, String> {
    let url = format!("{api}{path}");
    let mut args: Vec<String> = vec![
        "-s".into(), "--max-time".into(), "180".into(),
        "-X".into(), method.into(),
        "-w".into(), "\n%{http_code}".into(),
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
    let out = Command::new("curl").args(&args).output().map_err(|e| format!("curl: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (payload, code) = text.rsplit_once('\n').unwrap_or(("", "000"));
    if !code.trim().starts_with('2') {
        return Err(format!("{method} {path} -> HTTP {} {}", code.trim(), payload.chars().take(200).collect::<String>()));
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
    format!("file-{}-{:08x}", out.trim_matches('-'), (h & 0xffff_ffff) as u32)
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
        assert_eq!(stable_name("platform/api/src/server.ts"), stable_name("platform/api/src/server.ts"));
        assert!(stable_name("platform/api/src/server.ts").starts_with("file-platform-api-src-server-ts-"));
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
    let coll = collection_for(api, "CodeFile")?;
    let mut out = Vec::new();
    let mut next = format!("{coll}?limit=1000");
    let mut pages = 0usize;
    loop {
        let page = curl(api, "GET", &next, None, Some(token))?;
        for chunk in page.split("\"filePath\"").skip(1) {
            let v = chunk.trim_start().trim_start_matches(':').trim().trim_start_matches('"');
            let path = match v.find('"') { Some(i) => &v[..i], None => continue };
            let sha = chunk
                .find("\"fileSha\"")
                .and_then(|i| {
                    let t = chunk[i..].trim_start_matches("\"fileSha\"").trim_start().trim_start_matches(':').trim().trim_start_matches('"');
                    t.find('"').map(|j| t[..j].to_string())
                })
                .unwrap_or_default();
            out.push(InGraph { path: path.to_string(), sha });
        }
        pages += 1;
        // The door's own "next", with the /v1 prefix stripped the way every
        // other caller addresses it. No next link means this was the last page.
        let link = page
            .find("\"next\"")
            .and_then(|i| {
                let t = page[i..].trim_start_matches("\"next\"").trim_start().trim_start_matches(':').trim().trim_start_matches('"');
                t.find('"').map(|j| t[..j].to_string())
            })
            .filter(|l| !l.is_empty());
        match link {
            Some(l) => next = l.trim_start_matches("/v1").to_string(),
            None => break,
        }
        if pages > 10_000 {
            return Err("paging did not terminate after 10,000 pages — the door is not advancing".into());
        }
    }
    Ok(out)
}

fn main() {
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
    let reachable = watermark.as_deref().map(|w| commit_is_reachable(&root, w)).unwrap_or(false);
    // scope_for cannot know WHY the watermark is absent; on a reconcile we
    // cleared it ourselves, and reporting that as "first run" would have the
    // run narrate a state it is not in.
    let scope = match scope_for(watermark.as_deref(), &head, reachable) {
        Scope::Full { .. } if reconciling => Scope::Full { why: "--reconcile: forced full walk" },
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

    let api = std::env::var("CHORUS_OWL_API").unwrap_or_else(|_| "http://localhost:3360".to_string());
    let role = std::env::var("CHORUS_ROLE").unwrap_or_else(|_| "crawler".to_string());

    // What the graph already holds. Read BEFORE deciding anything — the walk is
    // idempotent by diff, not by luck.
    let (graph, token) = if dry_run && !reconciling {
        (Vec::new(), String::new())
    } else {
        let t = match identity_token(&root, &role) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("chorus-crawl: {e} — refusing to write without an identity");
                std::process::exit(2);
            }
        };
        match existing_rows(&api, &t) {
            Ok(g) => (g, t),
            Err(e) => {
                eprintln!("chorus-crawl: cannot read existing rows ({e}) — refusing to plan against an unknown graph");
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
        (Scope::Full { why: "graph was reset under a live watermark" }, all, d, r)
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
    let c = counts(&actions);

    println!("chorus-crawl: {} · tracked={} read={:?}", scope.label(), paths.len(), read);
    println!(
        "chorus-crawl: posted={} replaced={} unchanged={} deleted={} skipped={}{}",
        c.posted,
        c.replaced,
        c.unchanged,
        c.deleted,
        c.skipped,
        if dry_run { "  (dry-run — nothing written)" } else { "" }
    );
    if read == TreeRead::Partial {
        println!("chorus-crawl: tree read was PARTIAL — deletes refused this run (#4022: absent must not mean delete)");
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
        println!("chorus-crawl: {}", drift.report());
        std::process::exit(if drift.is_clean() { 0 } else { 1 });
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

    let flush = |batch: &mut Vec<String>, failed: &mut Vec<String>, wrote: &mut usize| {
        if batch.is_empty() {
            return;
        }
        let body = format!("[{}]", batch.join(","));
        match curl(&api, "POST", &format!("{coll}/batch"), Some(&body), Some(&token)) {
            Ok(_) => *wrote += batch.len(),
            Err(e) => failed.push(format!("batch of {}: {e}", batch.len())),
        }
        batch.clear();
    };

    for a in &actions {
        match a {
            Action::Post { path } | Action::Replace { path } => {
                let f = match by_path.get(path.as_str()) { Some(f) => *f, None => continue };
                let is_rs = path.ends_with(".rs");
                if let Verdict::Classified(k, l) = classify(path, is_rs && rust_declares_tests(&format!("{root}/{path}"))) {
                    batch.push(row_json(f, k.as_str(), l));
                    if batch.len() >= 200 {
                        flush(&mut batch, &mut failed, &mut wrote);
                    }
                }
            }
            Action::Delete { path } => {
                let name = stable_name(path);
                if let Err(e) = curl(&api, "DELETE", &format!("{coll}/{name}"), None, Some(&token)) {
                    failed.push(format!("delete {path}: {e}"));
                }
            }
            _ => {}
        }
    }
    flush(&mut batch, &mut failed, &mut wrote);

    println!("chorus-crawl: wrote={} failed={}", wrote, failed.len());

    // The watermark moves only when this run earned it.
    let scope_was_full = matches!(scope, Scope::Full { .. });
    match watermark_after(&head, read, failed.len(), scope_was_full) {
        Watermark::Advance(sha) => {
            println!("chorus-crawl: watermark -> {}", &sha[..sha.len().min(9)]);
            if let Err(e) = std::fs::write(format!("{root}/.chorus-crawl-watermark"), &sha) {
                eprintln!("chorus-crawl: could not record the watermark ({e}) — next run walks full");
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
        eprintln!("chorus-crawl: {} write(s) failed — the run is RED, not partially green", failed.len());
        std::process::exit(1);
    }
}
