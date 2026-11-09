//! borg-prove (#4456) — the I/O half. Modes match the Proving skills:
//!   intake|verify|stage|verdict|prod|record <card> <role>
//!   route <card> <role> <dagu-run-id> <step=route,…>
//! Exit: 0 ok, 1 refused/failed (the step fails, so its onFail routes it).
use borg_prove::*;
use std::process::{exit, Command};

fn env(k: &str, d: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| d.to_string())
}

fn curl(args: &[&str]) -> Option<String> {
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        if let Ok(out) = Command::new("curl").args(["-sf", "--max-time", "15"]).args(args).output() {
            if out.status.success() {
                return Some(String::from_utf8_lossy(&out.stdout).into_owned());
            }
        }
    }
    None
}

/// The store needs its admin credential for reads of most graphs. It goes to
/// curl on stdin (-K -), never on argv, and is never printed.
fn sparql(query: &str, accept: &str) -> Option<String> {
    use std::io::Write;
    let q = format!("query={query}");
    let endpoint = env("FUSEKI_QUERY", "http://localhost:3030/pods/query");
    let mut cmd = Command::new("curl");
    cmd.args(["-sf", "--max-time", "30", "-K", "-", "-H", &format!("Accept: {accept}"), "--data-urlencode", &q, &endpoint])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    if let (Ok(u), Ok(p)) = (std::env::var("FUSEKI_ADMIN_USER"), std::env::var("FUSEKI_ADMIN_PASSWORD")) {
        let _ = writeln!(child.stdin.as_mut()?, "user = \"{u}:{p}\"");
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// What the running system answers for one check. Read-only: GETs and ASKs.
fn probe(c: &Check, base: &str, card: u32) -> Seen {
    match c.kind.as_str() {
        "http" => {
            let url = if c.target.starts_with("http") { c.target.clone() } else { format!("{base}{}", c.target) };
            if is_status(&c.expect) {
                // an expect of three digits is the HTTP status, not body text
                let out = Command::new("curl").args(["-s", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "15", &url]).output().ok()?;
                let code = String::from_utf8_lossy(&out.stdout).trim().to_string();
                (code != "000").then_some(code)
            } else {
                curl(&[&url])
            }
        }
        "sparql" => sparql(&c.target, "application/sparql-results+json"),
        "test" => {
            // #4536 — the latest result of the test, from its case events in Loki (card
            // runs and the nightly both log there), from this card's own runs, and main's
            // nightly from the land on (#4824).
            // #4889 (land 11-05) — the card's window starts at its creation, never 30 days
            // back, and a Loki answer that is not 200 is an error, not "no result yet".
            let loki = env("LOKI_URL", "http://127.0.0.1:3102");
            let url = format!("{loki}/loki/api/v1/query_range");
            let ask = |q: String, start: String| -> Result<Option<(u128, String)>, String> {
                let out = Command::new("curl").args(["-s", "--max-time", "60", "-G", &url, "--data-urlencode", &format!("query={q}"),
                    "--data-urlencode", &start, "--data-urlencode", &format!("limit={}", borg_prove::CASE_LIMIT),
                    "--data-urlencode", "direction=backward", "-w", "\n%{http_code}"]).output()
                    .map_err(|e| format!("error: Loki not asked: {e}"))?;
                let text = String::from_utf8_lossy(&out.stdout).into_owned();
                let (body, code) = text.rsplit_once('\n').unwrap_or(("", text.as_str()));
                borg_prove::loki_seen(code, body)
            };
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
            let own = match card_view(card).as_deref().and_then(borg_prove::card_created).as_deref().and_then(borg_prove::rfc3339_epoch) {
                Some(created) => ask(test_logql(&c.target, card), format!("start={}000000000", borg_prove::window_start(created, now))),
                None => Err(format!("error: #{card}'s Created time is not readable (unmeasured)")),
            };
            let main = match land_time(card) {
                Some(t) => ask(test_logql_main(&c.target), format!("start={t}000000000")),
                None => Ok(None),
            };
            borg_prove::test_seen(own, main)
        }
        "crawl" => {
            // #4824 — the crawl's row for the class, only once the crawl has reached the land
            let body = curl(&[&format!("{base}/api/crawler/validate")])?;
            let (head, row) = crawl_row(&body, &c.target)?;
            let landed = land_time(card)?;
            (commit_time(&head)? >= landed).then(|| format!("{row} head={}", &head[..head.len().min(9)]))
        }
        // #4823 AC2 — `check: by #N`: card N's board status, read-only
        "by" => {
            let cards = env("BORG_CARDS", &format!("{}/platform/scripts/cards", env("CHORUS_HOME", "")));
            let out = Command::new("bash").arg(&cards).args(["view", &c.target]).output().ok()?;
            let view = String::from_utf8_lossy(&out.stdout);
            out.status.success().then(|| view.lines().find_map(|l| l.trim().strip_prefix("Status:").map(|s| s.trim().to_string())))?
        }
        _ => None,
    }
}

fn checks(card: u32) -> Vec<Check> {
    // #4760 — the AC is read from the card itself, never from a copy.
    // BORG_CARD_DESC: read the card's text from a file (a fixture); the probes
    // still go to the running system, read-only
    let desc = std::env::var("BORG_CARD_DESC").ok().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| desc_of(&t)).or_else(|| card_text(card));
    let Some(desc) = desc else {
        eprintln!("borg-prove: the board did not answer for #{card}");
        exit(1);
    };
    let cs = checks_from_card(card, &desc);
    // a card written before AC lines carried checks has nothing to prove: it
    // lands as before, unproven (intake at pull refuses new ones)
    if cs.iter().all(|c| c.kind == "none") { Vec::new() } else { cs }
}

/// The card's description as the board holds it (`cards view`), cut before
/// the board's own fields so a comment's checkbox is never read as an AC.
/// The whole `cards view` (its header lines hold Created:).
fn card_view(card: u32) -> Option<String> {
    let cards = env("BORG_CARDS", &format!("{}/platform/scripts/cards", env("CHORUS_HOME", "")));
    let out = Command::new("bash").arg(&cards).args(["view", &card.to_string()]).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn card_text(card: u32) -> Option<String> {
    card_view(card).map(|t| desc_of(&t))
}

/// The description part of a `cards view` (between `  Desc:` and `  Domains:`);
/// text without those markers (a bare description file) is returned whole, so
/// both paths read AC lines the same way.
fn desc_of(text: &str) -> String {
    const DESC: &str = "\n  Desc:";
    let start = text.find(DESC).map(|i| i + DESC.len()).unwrap_or(0);
    let end = text[start..].find("\n  Domains:").map(|i| start + i).unwrap_or(text.len());
    text[start..end].to_string()
}

/// #4824 — when the card reached main: the newest first-parent commit on main naming it.
fn land_time(card: u32) -> Option<u64> {
    let home = env("CHORUS_HOME", "");
    let out = Command::new("git").args(["-C", &home, "-c", "safe.directory=*", "log", "origin/main", "--first-parent", "-1",
        "--extended-regexp", &format!("--grep=#{card}([^0-9]|$)"), "--format=%ct"]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// #4824 — a commit's time, for "has the crawl reached the land".
fn commit_time(sha: &str) -> Option<u64> {
    let home = env("CHORUS_HOME", "");
    let out = Command::new("git").args(["-C", &home, "-c", "safe.directory=*", "log", "-1", "--format=%ct", sha]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

fn read_codesign_cdhash(codesign_bin: &str, bin: &str) -> Option<String> {
    let bin_dir = env("CHORUS_BIN_DIR", &format!("{}/.chorus/bin", env("HOME", "")));
    let p = format!("{bin_dir}/{bin}");
    let out = Command::new(codesign_bin)
        .args(["--display", "--verbose=3", &p])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stderr);
    text.lines().find_map(|l| l.strip_prefix("CDHash=").map(|s| s.trim().to_string()))
}

fn check_manifest(
    card: u32,
    expected_override: Option<&[String]>,
    deploy_log_path: Option<&str>,
    codesign_bin: Option<&str>,
) -> Outcome {
    let door = env("CHORUS_DOOR", "http://127.0.0.1:3360");
    let url = format!("{}/v1/cicd/deployments", door.trim_end_matches('/'));
    let Some(body) = curl(&[&url]) else {
        return Outcome::Unmeasured(format!("door did not answer at {url}"));
    };
    let records = match parse_deployments_for_card_checked(&body, card) {
        Ok(r) => r,
        Err(e) => return Outcome::Unmeasured(format!("door returned unparseable manifest: {e}")),
    };

    let expected = if let Some(exp) = expected_override {
        exp.to_vec()
    } else {
        let home = env("CHORUS_HOME", "");
        let default_log = format!("{home}/ops/logs/werk-deploy.jsonl");
        let log_path = deploy_log_path.unwrap_or(&default_log);
        let log_text = match std::fs::read_to_string(log_path) {
            Ok(t) => t,
            Err(e) => return Outcome::Unmeasured(format!("deploy log is unreadable at {log_path}: {e}")),
        };
        let land_trace = std::env::var("CHORUS_TRACE_ID").ok().filter(|s| !s.trim().is_empty())
            .or_else(|| std::fs::read_to_string(format!("/tmp/{card}-trace")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
        units_from_deploy_log(&log_text, card, land_trace.as_deref())
    };

    let codesign = codesign_bin.unwrap_or("codesign");
    let reader = |bin: &str| read_codesign_cdhash(codesign, bin);
    match verify_manifest_records(&records, &expected, card, reader) {
        Ok(0) => Outcome::Met("no binaries deployed for this card".into()),
        Ok(n) => Outcome::Met(format!("verified {n} deployment record(s)")),
        Err(why) => Outcome::NotMet(why),
    }
}

fn run_checks(card: u32, base: &str) -> Vec<(Check, Outcome)> {
    println!("#{card} against {base}");
    let cs = checks(card);
    let mut results: Vec<(Check, Outcome)> = cs.into_iter().map(|c| { let o = judge(&c, &probe(&c, base, card)); (c, o) }).collect();

    // #4773 AC4 / AC5 — manifest step after the card's own checks (runs even if cs is empty)
    let manifest_check = Check {
        ac: format!("ac-{card}-manifest"),
        statement: "deployment manifest matches running cdhash".to_string(),
        kind: "manifest".to_string(),
        target: "cicd/deployments".to_string(),
        expect: "verified".to_string(),
    };
    let manifest_outcome = check_manifest(card, None, None, None);
    results.push((manifest_check, manifest_outcome));

    for (c, o) in &results {
        println!("{:<12} {:<10} {}\n             {}", c.ac, o.word(), c.statement, o.evidence());
    }
    results
}

/// #4889 (Kade 19:56) — the open deferred checks: one file per card, one line per AC
/// (`ac \t statement \t check text`), written at that card's land, read at every land after.
fn deferred_dir() -> std::path::PathBuf {
    let dir = env("BORG_RESULTS_DIR", &format!("{}/ops/borg-prove", env("CHORUS_HOME", "/tmp")));
    std::path::PathBuf::from(dir).join("deferred")
}

/// At a card's own land: its `after` checks are written down to be re-run later.
fn defer_open(card: u32, r: &[(Check, Outcome)]) {
    let lines: Vec<String> = r.iter().filter(|(c, _)| c.kind == "after")
        .map(|(c, _)| format!("{}\t{}\t{}", c.ac, c.statement.replace('\t', " "), c.target)).collect();
    if lines.is_empty() { return; }
    let d = deferred_dir();
    let path = d.join(format!("{card}.tsv"));
    if let Err(e) = std::fs::create_dir_all(&d).and_then(|_| std::fs::write(&path, lines.join("\n") + "\n")) {
        eprintln!("#{card} record: the deferred checks were not written to {}: {e}", path.display());
        exit(1);
    }
    let cards = env("BORG_CARDS", &format!("{}/platform/scripts/cards", env("CHORUS_HOME", "")));
    let names: Vec<&str> = r.iter().filter(|(c, _)| c.kind == "after").map(|(c, _)| c.ac.as_str()).collect();
    let _ = Command::new("bash").arg(&cards).args(["comment", &card.to_string(),
        &format!("borg-prove: deferred: {} — re-run at the next land; met clears it, red reopens this card", names.join(", "))]).status();
}

/// At every land: re-run every other card's open deferred checks against prod. Met clears
/// the line; not met reopens that card (slice 5's path); unmeasured stays open.
/// #4916 — deduplicated per card so multiple red ACs on a card trigger only one reopen.
fn rerun_deferred(this: u32, base: &str) {
    let Ok(entries) = std::fs::read_dir(deferred_dir()) else { return };
    let cards = env("BORG_CARDS", &format!("{}/platform/scripts/cards", env("CHORUS_HOME", "")));
    for e in entries.flatten() {
        let Some(card) = e.path().file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<u32>().ok()) else { continue };
        if card == this { continue; }
        let text = std::fs::read_to_string(e.path()).unwrap_or_default();
        let mut keep = Vec::new();
        let mut red_acs = Vec::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let mut f = line.splitn(3, '\t');
            let (ac, statement, target) = (f.next().unwrap_or(""), f.next().unwrap_or(""), f.next().unwrap_or(""));
            let outer = Check { ac: ac.into(), statement: statement.into(), kind: "after".into(), target: target.into(), expect: "later".into() };
            let Some(c) = inner_check(&outer) else { keep.push(line.to_string()); continue };
            let o = judge(&c, &probe(&c, base, card));
            println!("#{card} deferred {ac}: {} — {}", o.word(), o.evidence());
            match rerun_action(&o) {
                Rerun::Cleared => { let _ = Command::new("bash").arg(&cards).args(["comment", &card.to_string(),
                    &format!("borg-prove: deferred {ac} met at the land of #{this}: {}", o.evidence())]).status(); }
                Rerun::Reopen => {
                    red_acs.push((ac.to_string(), line.to_string()));
                }
                Rerun::KeepOpen => keep.push(line.to_string()),
            }
        }
        if !red_acs.is_empty() {
            // #4916 (Wren): A card already in Later gets a new comment every land. Skip it when nothing changed.
            let cur_status = card_view(card).and_then(|v| v.lines().find_map(|l| l.trim().strip_prefix("Status:").map(|s| s.trim().to_string())));
            let already_later = matches!(cur_status.as_deref(), Some("Later"));
            let ok = if already_later {
                true
            } else {
                let names = red_acs.iter().map(|(ac, _)| ac.as_str()).collect::<Vec<_>>().join(", ");
                reopen(card, "Later", &format!("Not met on deferred {names} (at the land of #{this})"))
            };
            for (_, line) in red_acs {
                if keeps_deferral(&Rerun::Reopen, ok) { keep.push(line); }
            }
        }
        let _ = if keep.is_empty() { std::fs::remove_file(e.path()) } else { std::fs::write(e.path(), keep.join("\n") + "\n") };
    }
}

/// Where prod leaves its results for record: one file per card per run.
fn results_path(card: u32) -> std::path::PathBuf {
    run_file(card, "tsv")
}

/// #4691 — a land run's files: prod's results (.tsv) and her verdict (.verdict).
fn run_file(card: u32, ext: &str) -> std::path::PathBuf {
    let dir = env("BORG_RESULTS_DIR", &format!("{}/ops/borg-prove", env("CHORUS_HOME", "/tmp")));
    std::path::PathBuf::from(dir).join(format!("{card}-{}.{ext}", env("DAG_RUN_ID", "manual")))
}

/// #4683 — the verdict on the spine (prover borg-queen: the pipeline itself
/// until Borg Queen is provisioned) and on the card, one line per AC.
fn record(card: u32, role: &str, v: &str, r: &[(Check, Outcome)]) {
    let log = env("BORG_CHORUS_LOG", &format!("{}/platform/scripts/chorus-log", env("CHORUS_HOME", "")));
    let cards = env("BORG_CARDS", &format!("{}/platform/scripts/cards", env("CHORUS_HOME", "")));
    let word = if v == NO_ROWS { "unproven" } else { v };
    let spine = Command::new("bash").arg(&log)
        .args(["borg.verdict", if role.is_empty() { "system" } else { role }, &format!("card={card}"), &format!("verdict={word}"), "prover=borg-queen"])
        .status();
    let mut text = format!("borg-prove in prod: {v} (prover borg-queen)");
    for (c, o) in r {
        text.push_str(&format!("\n{} {}: {}", c.ac, o.word(), o.evidence()));
    }
    let comment = Command::new("bash").arg(&cards).args(["comment", &card.to_string(), &text]).status();
    if !matches!(spine, Ok(s) if s.success()) || !matches!(comment, Ok(s) if s.success()) {
        eprintln!("#{card} record: the verdict was not written (spine {spine:?}, card {comment:?})");
        exit(1);
    }
    let reds = reds(r);
    if v == NO_ROWS || reds.is_empty() {
        return;
    }
    // AC3 — one child per red AC, owned by the parent's owner, filed by
    // borg-queen (not an agent role, so no bouncer); a rerun files none twice
    let view = Command::new("bash").arg(&cards).args(["view", &card.to_string()]).output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    // #4687 (Wren) — a child that is not filed is said on the parent, where
    // Jeff reads, not only on the step's stderr
    let say_on_parent = |why: &str| {
        eprintln!("#{card} {why}");
        let _ = Command::new("bash").arg(&cards).args(["comment", &card.to_string(), &format!("borg-prove: {why}")]).status();
    };
    let Some(owner) = owner_of(&view) else {
        say_on_parent(&format!("has no owner on the board: no child cards filed for {}", reds.iter().map(|(c, _)| c.ac.as_str()).collect::<Vec<_>>().join(", ")));
        exit(1);
    };
    let Some(have) = child_titles(&view) else {
        eprintln!("#{card}: its children could not be read from the board (the count it states is not the count read): no child cards filed");
        exit(1);
    };
    for (c, o) in reds {
        let title = child_title(card, c);
        if have.contains(&title) {
            println!("#{card} {}: child already filed", c.ac);
            continue;
        }
        let added = Command::new("bash").arg(&cards)
            .args(["add", &title, "--owner", &owner, "--parent", &card.to_string(), "--priority", "P2",
                "--domain", "chorus", "--type", "fix", "--origin", "reactive", "--seq", "borg", "--desc", &child_desc(card, c, o)])
            .env("DEPLOY_ROLE", "borg-queen")
            .status();
        if !matches!(added, Ok(s) if s.success()) {
            say_on_parent(&format!("{}: its child card could not be filed (cards add: {added:?})", c.ac));
            exit(1);
        }
    }
}

/// #4889 / #4916 — a red in prod reopens the card:
/// - The landing card stays in WIP for its fix round (target="WIP").
/// - Other cards re-run from deferred checks move to Later (target="Later").
/// Moves the card to target with the verdict as a comment. true only when both took.
fn reopen(card: u32, target: &str, verdict: &str) -> bool {
    let cards = env("BORG_CARDS", &format!("{}/platform/scripts/cards", env("CHORUS_HOME", "")));
    let moved = Command::new("bash").arg(&cards).args(["move", &card.to_string(), target]).env("DEPLOY_ROLE", "borg-queen").status();
    let said = Command::new("bash").arg(&cards)
        .args(["comment", &card.to_string(), &format!("borg-prove: reopened to {target} — prod reads {verdict}; accept held until a new round proves it")]).status();
    let done = matches!(moved, Ok(s) if s.success()) && matches!(said, Ok(s) if s.success());
    if !done {
        eprintln!("#{card} record: reopen did not complete (move {moved:?}, comment {said:?})");
    }
    done
}

/// The step that failed in a dagu run, from `dagu status` ("├─name (…) [failed]").
fn failed_step(dag: &str, run: &str) -> String {
    let cfg = env("DAGU_CONFIG", &format!("{}/platform/config/dagu.yaml", env("CHORUS_HOME", "")));
    let out = Command::new("dagu").args(["status", dag, "--run-id", run, "--config", &cfg]).output();
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    failed_in_status(&text)
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (Some(mode), Some(card)) = (a.first(), a.get(1).and_then(|c| c.trim_start_matches('#').parse::<u32>().ok())) else {
        eprintln!("usage: borg-prove ac|intake|verify|stage|verdict|prod|record|route <card> <role> [run-id routes]");
        exit(2);
    };
    let role = a.get(2).cloned().unwrap_or_default();
    match mode.as_str() {
        // #4773 AC4 / AC5 — read manifest from door, assert prod running hashes match
        "manifest" => {
            let expected_arg: Option<Vec<String>> = a.get(2).and_then(|s| {
                if s == "-" || s == "auto" {
                    None
                } else {
                    Some(s.split(',').map(|u| u.trim().to_string()).filter(|u| !u.is_empty()).collect())
                }
            });
            let log_arg = a.get(3).and_then(|s| if s == "-" { None } else { Some(s.as_str()) });
            let codesign_arg = a.get(4).map(|s| s.as_str());
            match check_manifest(card, expected_arg.as_deref(), log_arg, codesign_arg) {
                Outcome::Met(ev) => {
                    println!("#{card} manifest: {ev}");
                    exit(0);
                }
                Outcome::NotMet(why) => {
                    eprintln!("#{card} manifest: REFUSED: {why}");
                    exit(1);
                }
                Outcome::Unmeasured(why) | Outcome::Deferred(why) => {
                    eprintln!("#{card} manifest: {why}");
                    exit(1);
                }
            }
        }
        // #4875 — the card's AC as the go message shows them: one line each, box state from the
        // same parser borg-prove proves with. Exit 1 (named) when the board does not answer.
        "ac" => {
            let desc = std::env::var("BORG_CARD_DESC").ok().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| desc_of(&t)).or_else(|| card_text(card));
            let Some(desc) = desc else { eprintln!("borg-prove: the board did not answer for #{card}"); exit(1) };
            for (done, c) in borg_prove::ac_lines(card, &desc) {
                println!("{}\t{}\t{}", if done { "✓" } else { "☐" }, c.statement, c.kind);
            }
        }
        "intake" => match intake(&checks(card)) {
            Ok(()) => println!("#{card} intake: every AC has a check"),
            Err(errs) => {
                eprintln!("#{card} Rejected at intake:");
                errs.iter().for_each(|e| eprintln!("  {e}"));
                exit(1);
            }
        },
        // verify and stage both read the demo copy until the werk-only probe exists
        "verify" | "stage" | "verdict" => {
            let base = env("BORG_DEMO_BASE", "http://localhost:3343");
            let r = run_checks(card, &base);
            let v = verdict(&r);
            println!("#{card} {mode}: {v}");
            if v != "Proven" { exit(1) }
        }
        // #4683 — prod and record run inside cicd-land, before accept. prod
        // measures and writes its results (its step continues on failure, so
        // record runs on the red path too); record judges those same results
        // (it never measures again: Silas, Wren at the demo), writes the
        // verdict and is the gate: it fails on any verdict but Proven, so
        // accept never runs after a red or an unmeasured AC. #4889: a card with no
        // AC check fails too (it was "lands as before"), and a red reopens the card.
        "prod" => {
            let prod_base = env("BORG_PROD_BASE", "http://localhost:3340");
            // #4889 — every land re-runs the other cards' open deferred checks first
            rerun_deferred(card, &prod_base);
            let r = run_checks(card, &prod_base);
            let path = results_path(card);
            if let Err(e) = std::fs::create_dir_all(path.parent().unwrap()).and_then(|_| std::fs::write(&path, results_to_tsv(&r))) {
                eprintln!("#{card} prod: the results were not written to {}: {e}", path.display());
                exit(1);
            }
            // "Not met", not "Regressed": a card that never worked did not regress (Wren)
            let card_has_ac = !checks(card).is_empty();
            let v = if !card_has_ac { NO_ROWS } else { verdict(&r) };
            println!("#{card} verify in prod: {v}");
            if !prod_passes(v) { exit(1) }
        }
        "record" => {
            // #4760 — the land runs record; it hands itself to her account (her
            // signature on the verdict file), carrying the run in its args
            if let (Some(run), Some(home)) = (a.get(3), a.get(4)) {
                std::env::set_var("DAG_RUN_ID", run);
                std::env::set_var("CHORUS_HOME", home);
            }
            let queen = env("BORG_QUEEN_ACCOUNT", "chorus-borg-queen");
            let me = Command::new("id").arg("-un").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
            let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "borg-prove".into());
            if let Some(hop) = queen_hop(&me, &queen, &exe, &card.to_string(), &role, &env("DAG_RUN_ID", "manual"), &env("CHORUS_HOME", "")) {
                let st = Command::new(env("BORG_SUDO", "sudo")).args(&hop).status();
                match st {
                    Ok(s) if s.success() => exit(0),
                    Ok(s) => { eprintln!("#{card} record: as {queen} it ended {s} (a sudo refusal means the sudoers line is missing): accept held"); exit(1) }
                    Err(e) => { eprintln!("#{card} record: could not run as {queen}: {e}: accept held"); exit(1) }
                }
            }
            let path = results_path(card);
            let r = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| results_from_tsv(&t)).unwrap_or_else(|e| {
                eprintln!("#{card} record: prod's results are not readable at {} ({e}): nothing recorded, accept held", path.display());
                exit(1)
            });
            let card_has_ac = !checks(card).is_empty();
            let v = if !card_has_ac { NO_ROWS } else { verdict(&r) };
            println!("#{card} record: {v}");
            record(card, &role, v, &r);
            // #4889 — a Proven card with `after` checks lands Done; they stay open, named on it
            if prod_passes(v) { defer_open(card, &r); }
            // #4691 — the verdict for check, written as whoever ran record:
            // in the land that is Borg Queen's own session, her Mac account
            let word = if v == NO_ROWS { "unproven" } else { v };
            if let Err(e) = std::fs::write(run_file(card, "verdict"), format!("{word}\n")) {
                eprintln!("#{card} record: the verdict file was not written: {e}");
                exit(1);
            }
            // #4889 — a red reopens the card: it goes back to WIP, said on the card, and
            // accept never runs (this step fails)
            if !prod_passes(v) {
                // #4889 (code review, run 17) — a card whose every AC is `after` is Unproven by
                // design; werk-demo's validate refuses it before the go, so reaching here says so
                if !r.is_empty() && r.iter().all(|(c, _)| c.kind == "after") {
                    eprintln!("#{card} record: every AC is `check: after`, so nothing proved it before the land; validate should have refused it");
                }
                reopen(card, "WIP", word);
                exit(1)
            }
        }
        "route" => {
            let (Some(_run), Some(routes)) = (a.get(3), a.get(4)) else {
                eprintln!("usage: borg-prove route <card> <role> <dagu-run-id> <step=route,…>");
                exit(2);
            };
            // dagu names only the handler itself to the handler, so the failed
            // step is read from the run's own status (DAG_NAME + the run id)
            let failed = std::env::var("BORG_FAILED_STEP").ok().unwrap_or_else(|| failed_step(&env("DAG_NAME", ""), _run));
            let r = route_for(&failed, &parse_routes(routes), &role);
            println!("#{card} {failed:?} → {r:?}");
            if r == Route::Unrouted { exit(1) }
        }
        other => {
            eprintln!("borg-prove: unknown mode '{other}'");
            exit(2);
        }
    }
}
