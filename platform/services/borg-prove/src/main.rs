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
    let out = Command::new("curl").args(["-sf", "--max-time", "15"]).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
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
            // #4536 — the latest result of the test, from its case events in
            // Loki (card runs and the nightly both log there; TestResult rows
            // are no longer written), from this card's own runs only. No event in
            // 30 days → None → UNMEASURED.
            let loki = env("LOKI_URL", "http://127.0.0.1:3102");
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
            let url = format!("{loki}/loki/api/v1/query_range");
            let (start, end) = (format!("start={}000000000", now - 30 * 86400), format!("end={now}000000000"));
            let q = format!("query={}", test_logql(&c.target, card));
            let body = curl(&["-G", &url, "--data-urlencode", &q, "--data-urlencode", &start,
                "--data-urlencode", &end, "--data-urlencode", "limit=1", "--data-urlencode", "direction=backward"])?;
            latest_case_result(&body)
        }
        _ => None,
    }
}

fn checks(card: u32) -> Vec<Check> {
    // BORG_CHECKS_TSV: read the rows from a file (a fixture, or rows not yet in
    // the store); the probes still go to the running system, read-only
    let file = std::env::var("BORG_CHECKS_TSV").ok().and_then(|p| std::fs::read_to_string(p).ok());
    let Some(tsv) = file.or_else(|| sparql(&checks_query(card), "text/tab-separated-values")) else {
        eprintln!("borg-prove: the store did not answer for #{card}'s CardAC rows");
        exit(1);
    };
    checks_from_tsv(&tsv).unwrap_or_else(|e| {
        eprintln!("borg-prove: {e}");
        exit(1)
    })
}

fn run_checks(card: u32, base: &str) -> Vec<(Check, Outcome)> {
    println!("#{card} against {base}");
    let cs = checks(card);
    let results: Vec<(Check, Outcome)> = cs.into_iter().map(|c| { let o = judge(&c, &probe(&c, base, card)); (c, o) }).collect();
    for (c, o) in &results {
        println!("{:<12} {:<10} {}\n             {}", c.ac, o.word(), c.statement, o.evidence());
    }
    results
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
        eprintln!("usage: borg-prove intake|verify|stage|verdict|prod|record|route <card> <role> [run-id routes]");
        exit(2);
    };
    let role = a.get(2).cloned().unwrap_or_default();
    match mode.as_str() {
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
        // accept never runs after a red or an unmeasured AC. A card with no
        // CardAC rows lands as before and says "unproven".
        "prod" => {
            let r = run_checks(card, &env("BORG_PROD_BASE", "http://localhost:3340"));
            let path = results_path(card);
            if let Err(e) = std::fs::create_dir_all(path.parent().unwrap()).and_then(|_| std::fs::write(&path, results_to_tsv(&r))) {
                eprintln!("#{card} prod: the results were not written to {}: {e}", path.display());
                exit(1);
            }
            // "Not met", not "Regressed": a card that never worked did not regress (Wren)
            let v = if r.is_empty() { NO_ROWS } else { verdict(&r) };
            println!("#{card} verify in prod: {v}");
            if v != "Proven" && v != NO_ROWS { exit(1) }
        }
        "record" => {
            let path = results_path(card);
            let r = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| results_from_tsv(&t)).unwrap_or_else(|e| {
                eprintln!("#{card} record: prod's results are not readable at {} ({e}): nothing recorded, accept held", path.display());
                exit(1)
            });
            let v = if r.is_empty() { NO_ROWS } else { verdict(&r) };
            println!("#{card} record: {v}");
            record(card, &role, v, &r);
            // #4691 — the verdict for check, written as whoever ran record:
            // in the land that is Borg Queen's own session, her Mac account
            let word = if v == NO_ROWS { "unproven" } else { v };
            if let Err(e) = std::fs::write(run_file(card, "verdict"), format!("{word}\n")) {
                eprintln!("#{card} record: the verdict file was not written: {e}");
                exit(1);
            }
            if v != "Proven" && v != NO_ROWS { exit(1) }
        }
        // #4691 — the land asks Borg Queen, then waits at her step (a human task,
        // the way a pair waits for its navigator's green)
        "ask" => {
            let run = env("DAG_RUN_ID", "");
            let cfg = env("DAGU_CONFIG", &format!("{}/platform/config/dagu.yaml", env("CHORUS_HOME", "")));
            let text = format!("#{card} landed and is in prod; prove it. In your session:\n  DAG_RUN_ID={run} borg-prove record {card} {role}\nthen:\n  dagu --config {cfg} human-task complete cicd-land --run-id {run} --step skill_borg_queen_proves --input by=borg-queen --input decision=recorded");
            let nudge = env("BORG_NUDGE", &format!("{}/platform/scripts/ops-nudge", env("CHORUS_HOME", "")));
            if !matches!(Command::new(&nudge).args(["borg-queen", &text]).status(), Ok(s) if s.success()) {
                eprintln!("#{card} ask: Borg Queen was not nudged ({nudge})");
                exit(1);
            }
            println!("#{card} ask: Borg Queen nudged for run {run}");
        }
        "check" => {
            let path = run_file(card, "verdict");
            let owner = Command::new("stat").args(["-f", "%Su"]).arg(&path).output().ok()
                .filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
            let v = std::fs::read_to_string(&path).ok();
            match signed_verdict(&role, owner.as_deref(), v.as_deref(), &env("BORG_QUEEN_ACCOUNT", "chorus-borg-queen")) {
                Ok(()) => println!("#{card} check: Borg Queen's verdict, {}", v.unwrap_or_default().trim()),
                Err(e) => { eprintln!("#{card} check: {e}"); exit(1) }
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
