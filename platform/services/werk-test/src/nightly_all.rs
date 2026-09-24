//! #4145 — `werk-test --nightly --run-all`: the whole 03:00 run, in the runner.
//! The orchestration half (processes, files, clocks) of what `nightly-suites.sh`
//! did in bash; the pure half is `werk_test::nightly_run`. The child that runs
//! the suites is THIS binary with `--nightly`, streamed to the lane file and
//! folded live, so the 50-minute middle is unchanged and everything around it
//! is one process that knows its own owners, its own cases and its own clock.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use werk_test::nightly_run::{crawl_line, json_rows, 
    coverage_row, denominator_row, fail_log_name, fold_unit_line, last_run_rows, load_verdict,
    notify_messages, owner_for, owner_map, parse_floors, pipeline_run_body, run_summary_fields,
    suite_result_fields, unit_slice, SuiteRow,
};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

// #4035 — a stop (TERM/INT) mid-run writes `RUN|stopped`, reaps the child and
// frees the lock. std has no signal API and the crate has no deps (ADR-032
// §1); libc's signal(2) is declared here directly.
static STOP_SIGNAL: AtomicI32 = AtomicI32::new(0);
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);
extern "C" {
    fn signal(sig: i32, handler: extern "C" fn(i32)) -> usize;
}
extern "C" fn on_stop(sig: i32) {
    STOP_SIGNAL.store(sig, Ordering::SeqCst);
    STOP_REQUESTED.store(true, Ordering::SeqCst);
}
fn install_stop_handler() {
    unsafe {
        signal(15, on_stop); // SIGTERM
        signal(2, on_stop); // SIGINT
    }
}
fn stop_requested() -> bool {
    STOP_REQUESTED.load(Ordering::SeqCst)
}

fn env_or(k: &str, d: &str) -> String {
    std::env::var(k).ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| d.to_string())
}

/// #4251 — launchd's own words for why the job last exited, when it has any.
/// Best-effort: a missing or unreadable answer is None, never a guess.
fn launchd_exit_reason() -> Option<String> {
    let uid = Command::new("id").arg("-u").output().ok()?;
    let uid = String::from_utf8_lossy(&uid.stdout).trim().to_string();
    let out = Command::new("launchctl")
        .arg("print")
        .arg(format!("gui/{uid}/com.chorus.nightly-suites"))
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().find(|l| l.contains("last exit reason"))?;
    let reason = line.split('=').nth(1)?.trim();
    if reason.is_empty() || reason == "0" {
        return None;
    }
    Some(format!("launchd {reason}"))
}

fn now_stamp() -> String {
    let out = Command::new("date").arg("+%Y-%m-%dT%H:%M:%S").output().ok();
    out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

fn epoch() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

struct Ctx {
    root: String,
    app_root: String,
    home: String,
    log: String,
    fail_dir: String,
    lockdir: String,
    role: String,
    run_id: String,
    owlapi: String,
    api: String,
    ops_nudge: String,
    no_nudge: bool,
    owners: HashMap<String, String>,
}

impl Ctx {
    fn append_log(&self, line: &str) -> bool {
        if let Some(d) = Path::new(&self.log).parent() {
            let _ = std::fs::create_dir_all(d);
        }
        match OpenOptions::new().create(true).append(true).open(&self.log) {
            Ok(mut f) => writeln!(f, "{}", line).is_ok(),
            Err(_) => false,
        }
    }
    fn spine(&self, event: &str, fields: &[(String, String)]) {
        let bin = env_or("CHORUS_LOG_BIN", &format!("{}/platform/scripts/chorus-log", self.root));
        if !Path::new(&bin).is_file() {
            return;
        }
        let mut c = Command::new("bash");
        c.arg(&bin).arg(event).arg(&self.role).arg(format!("run_id={}", self.run_id));
        for (k, v) in fields {
            c.arg(format!("{}={}", k, v));
        }
        let _ = c.stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    fn nudge(&self, to: &str, msg: &str) {
        if self.no_nudge || !Path::new(&self.ops_nudge).is_file() {
            return;
        }
        let _ = Command::new(&self.ops_nudge).arg(to).arg(msg).arg("system").stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    fn owner(&self, path: &str) -> String {
        owner_for(path, &self.owners, &self.root, &self.app_root)
    }
    fn write_fail_log(&self, row: &SuiteRow, body: &str) {
        let _ = std::fs::create_dir_all(&self.fail_dir);
        let p = format!("{}/{}", self.fail_dir, fail_log_name(&row.kind, &row.path));
        let head = format!("# unit: {} ({}) — verdict {}\n# summary: {}\n---\n", row.path, row.kind, row.status, row.summary);
        let _ = std::fs::write(p, format!("{}{}", head, body));
    }
}

/// `sysctl hw.ncpu` × NIGHTLY_LOAD_MAX_PER_CORE (1.5) against the 1-minute load.
fn load_gate() -> (bool, String) {
    let cores: f64 = Command::new("sysctl")
        .args(["-n", "hw.ncpu"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
        .unwrap_or(8.0);
    let per: f64 = env_or("NIGHTLY_LOAD_MAX_PER_CORE", "1.5").parse().unwrap_or(1.5);
    let load: f64 = match std::env::var("NIGHTLY_LOAD_STUB").ok().filter(|v| !v.is_empty()) {
        Some(v) => v.parse().unwrap_or(0.0),
        None => Command::new("sysctl")
            .args(["-n", "vm.loadavg"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).split_whitespace().nth(1).and_then(|v| v.parse().ok()))
            .unwrap_or(0.0),
    };
    load_verdict(load, cores, per)
}

fn pid_alive(pid: u32) -> bool {
    Command::new("kill").args(["-0", &pid.to_string()]).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

/// Single flight (#3597/#4008): a mkdir lock with the holder pid; a dead holder
/// with no live runner is stale and taken over.
fn acquire_lock(lockdir: &str) -> Result<(), String> {
    if std::fs::create_dir(lockdir).is_ok() {
        let _ = std::fs::write(format!("{}/pid", lockdir), std::process::id().to_string());
        return Ok(());
    }
    let old: Option<u32> = std::fs::read_to_string(format!("{}/pid", lockdir)).ok().and_then(|s| s.trim().parse().ok());
    if let Some(p) = old {
        if pid_alive(p) {
            return Err(format!("holder pid {} is alive", p));
        }
    }
    // a dead holder is stale unless a runner is still alive (#4008). NIGHTLY_PS
    // is the test seam: a command that prints `PID PPID ELAPSED COMMAND` rows.
    let ps_cmd = env_or("NIGHTLY_PS", "ps -eo pid,ppid,etime,command");
    let out = Command::new("bash").arg("-c").arg(&ps_cmd).output().ok();
    if let Some(o) = out {
        let me = std::process::id().to_string();
        let parent = std::os::unix::process::parent_id().to_string();
        let marker = env_or("NIGHTLY_RUNNER_MARKER", "werk-test --nightly|nightly-suites.sh --run-all|werk-test-bin --nightly");
        for l in String::from_utf8_lossy(&o.stdout).lines().skip(1) {
            let cols: Vec<&str> = l.split_whitespace().collect();
            if cols.len() < 4 {
                continue;
            }
            let (pid, age) = (cols[0], cols[2]);
            if pid == me || pid == parent {
                continue;
            }
            let cmd = cols[3..].join(" ");
            if cmd.contains("--lock-probe") {
                continue;
            }
            if marker.split('|').any(|m| cmd.contains(m)) {
                return Err(format!(
                    "holder pid {} is dead but runner pid {} is alive (age {})",
                    old.map(|p| p.to_string()).unwrap_or("none".into()),
                    pid,
                    age
                ));
            }
        }
    }
    let _ = std::fs::remove_dir_all(lockdir);
    std::fs::create_dir(lockdir).map_err(|e| format!("lock {} could not be taken: {}", lockdir, e))?;
    let _ = std::fs::write(format!("{}/pid", lockdir), std::process::id().to_string());
    Ok(())
}

// #4152 — run_capped lives in lib.rs (drains pipes while the child runs).
use werk_test::run_capped;

fn stack_up(ctx: &Ctx) -> bool {
    let api = env_or("NIGHTLY_STACK_API_URL", &format!("{}/api/chorus/context/health", ctx.api));
    let fuseki = env_or("NIGHTLY_STACK_FUSEKI_URL", "http://localhost:3030/$/ping");
    let ok = |u: &str| {
        Command::new("curl")
            .args(["-fsS", "-m", "8", "--retry", "2", "--retry-delay", "5", "--retry-all-errors", "-o", "/dev/null", u])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    ok(&api) && ok(&fuseki)
}

// ───────────────────────── phase 1: the pre-checks ─────────────────────────

fn leg_lint(ctx: &Ctx) -> Option<SuiteRow> {
    let root = &ctx.root;
    if !(Path::new(&format!("{}/.eslint-baseline.json", root)).is_file() && Path::new(&format!("{}/eslint.config.js", root)).is_file()) {
        return None;
    }
    let mut c = Command::new("npm");
    c.args(["run", "lint:ratchet", "--silent"]).current_dir(root);
    let (rc, out) = run_capped(c, Duration::from_secs(600));
    let detail = out.lines().last().unwrap_or("").to_string();
    Some(if rc == 0 {
        SuiteRow::new("lint", root, "kade", "pass", &format!("1 pass, 0 fail (lint:ratchet clean — {})", detail))
    } else {
        SuiteRow::new("lint", root, "kade", "fail", &format!("0 pass, 1 fail (lint:ratchet drifted rc={} — {})", rc, detail))
    })
}

fn read_pct_ts(path: &str) -> Option<f64> {
    let s = std::fs::read_to_string(path).ok()?;
    // {"total":{"statements":{"pct":80.05,...
    let i = s.find("\"total\"")?;
    let j = s[i..].find("\"statements\"")? + i;
    let k = s[j..].find("\"pct\"")? + j;
    let rest = &s[k + 5..];
    let num: String = rest.chars().skip_while(|c| *c == ':' || c.is_whitespace()).take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    num.parse().ok()
}

fn read_pct_rust(path: &str) -> Option<f64> {
    let s = std::fs::read_to_string(path).ok()?;
    // {"data":[{"totals":{"lines":{"percent":72.3,...
    let i = s.find("\"totals\"")?;
    let j = s[i..].find("\"lines\"")? + i;
    let k = s[j..].find("\"percent\"")? + j;
    let rest = &s[k + 9..];
    let num: String = rest.chars().skip_while(|c| *c == ':' || c.is_whitespace()).take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    num.parse().ok()
}

fn leg_coverage(ctx: &Ctx) -> Vec<SuiteRow> {
    let floors_path = env_or("NIGHTLY_COVERAGE_FLOORS", &format!("{}/coverage-floors.yml", ctx.root));
    let yaml = match std::fs::read_to_string(&floors_path) {
        Ok(y) => y,
        Err(_) => return Vec::new(),
    };
    let fixtures = std::env::var("NIGHTLY_COVERAGE_FIXTURES").ok().filter(|v| !v.is_empty());
    let cap = Duration::from_secs(env_or("NIGHTLY_COVERAGE_TIMEOUT_S", "600").parse().unwrap_or(600));
    let workers = env_or("NIGHTLY_COVERAGE_WORKERS", "2");
    let mut rows = Vec::new();
    for (lang, rel, floor) in parse_floors(&yaml) {
        let owner = ctx.owner(&rel);
        let dir = format!("{}/{}", ctx.root, rel);
        let (rc, pct) = if let Some(fx) = &fixtures {
            let sj = if lang == "ts" { format!("{}/{}/coverage/coverage-summary.json", fx, rel) } else { format!("{}/{}/llvm-cov-summary.json", fx, rel) };
            (0, if lang == "ts" { read_pct_ts(&sj) } else { read_pct_rust(&sj) })
        } else if !Path::new(&dir).is_dir() {
            (127, None)
        } else if lang == "ts" {
            let sj = format!("{}/coverage/coverage-summary.json", dir);
            let prev = std::fs::read(&sj).ok();
            let mut c = Command::new("npx");
            c.args(["--no-install", "jest", "--coverage", "--coverageReporters=json-summary", "--passWithNoTests", "--silent", "--forceExit"])
                .arg(format!("--maxWorkers={}", workers))
                .current_dir(&dir);
            let (rc, _) = run_capped(c, cap);
            if rc != 0 {
                if let Some(p) = prev {
                    let _ = std::fs::write(&sj, p);
                }
            }
            (rc, read_pct_ts(&sj))
        } else {
            let tmp = format!("{}/.llvm-cov-summary.{}.tmp", dir, std::process::id());
            let mut c = Command::new("cargo");
            c.args(["llvm-cov", "--summary-only", "--json"]).current_dir(&dir);
            let (rc, out) = run_capped(c, cap);
            let sj = format!("{}/llvm-cov-summary.json", dir);
            if rc == 0 {
                let json_start = out.find('{').unwrap_or(0);
                let _ = std::fs::write(&tmp, &out[json_start..]);
                let _ = std::fs::rename(&tmp, &sj);
            } else {
                let tail: Vec<&str> = out.lines().rev().take(12).collect();
                eprintln!("coverage stderr for {} (rc={}, last 12 lines):\n{}", rel, rc, tail.into_iter().rev().collect::<Vec<_>>().join("\n"));
            }
            let _ = std::fs::remove_file(&tmp);
            (rc, if rc == 0 { read_pct_rust(&sj) } else { None })
        };
        rows.push(coverage_row(&rel, &owner, floor, rc, pct));
    }
    if let Some(d) = leg_denominator(ctx, &yaml) {
        rows.push(d);
    }
    rows
}

fn leg_denominator(ctx: &Ctx, floors_yaml: &str) -> Option<SuiteRow> {
    let services = format!("{}/platform/services", ctx.root);
    if !Path::new(&services).is_dir() {
        return None;
    }
    let configured: Vec<String> = parse_floors(floors_yaml).into_iter().filter(|(_, r, _)| r.starts_with("platform/services/")).map(|(_, r, _)| r).collect();
    let mut crates: Vec<String> = ctx.owners.keys().filter_map(|f| f.strip_prefix("platform/services/")).filter_map(|r| r.split('/').next()).map(String::from).collect();
    crates.sort();
    crates.dedup();
    if crates.is_empty() {
        eprintln!("coverage-denominator: source=glob-fallback (registry unreachable — LOUD, #3974)");
        if let Ok(rd) = std::fs::read_dir(&services) {
            for e in rd.flatten() {
                if e.path().join("Cargo.toml").is_file() {
                    crates.push(e.file_name().to_string_lossy().to_string());
                }
            }
        }
    }
    let present: Vec<String> = crates.into_iter().filter(|c| Path::new(&format!("{}/{}/Cargo.toml", services, c)).is_file()).collect();
    if present.is_empty() {
        return None;
    }
    let unconfigured: Vec<String> = present.iter().filter(|c| !configured.contains(&format!("platform/services/{}", c))).cloned().collect();
    let baseline_file = env_or("NIGHTLY_COV_DENOM_BASELINE", &format!("{}/.coverage-denominator-baseline", ctx.root));
    let baseline: usize = std::fs::read_to_string(&baseline_file).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(unconfigured.len());
    let (row, new_baseline) = denominator_row(configured.len(), present.len(), &unconfigured, baseline);
    if let Some(nb) = new_baseline {
        let _ = std::fs::write(&baseline_file, nb.to_string());
    }
    Some(row)
}

fn leg_smoke(ctx: &Ctx) -> Option<SuiteRow> {
    let sc = format!("{}/platform/scripts/smoke-check.sh", ctx.root);
    if !Path::new(&sc).is_file() {
        return None;
    }
    if !stack_up(ctx) {
        return Some(SuiteRow::new("smoke", &sc, "kade", "skip", "skipped — no live stack (#3557)"));
    }
    let mut c = Command::new("bash");
    c.arg(&sc).arg("--all");
    let (rc, out) = run_capped(c, Duration::from_secs(600));
    let row = if rc == 0 {
        SuiteRow::new("smoke", &sc, "kade", "pass", "1 pass, 0 fail (smoke --all clean)")
    } else {
        SuiteRow::new("smoke", &sc, "kade", "fail", &format!("0 pass, 1 fail (smoke --all rc={})", rc))
    };
    if rc != 0 {
        ctx.write_fail_log(&row, &out);
    }
    Some(row)
}

fn leg_app_eslint(ctx: &Ctx) -> Option<SuiteRow> {
    if !Path::new(&format!("{}/src", ctx.app_root)).is_dir() {
        return None;
    }
    let mut c = Command::new("npx");
    c.args(["eslint", "src/", "--max-warnings", "999"]).current_dir(&ctx.app_root);
    let (rc, out) = run_capped(c, Duration::from_secs(600));
    let detail = out.lines().last().unwrap_or("").to_string();
    Some(if rc == 0 {
        SuiteRow::new("app-eslint", &ctx.app_root, "kade", "pass", &format!("1 pass, 0 fail (app eslint clean — {})", detail))
    } else {
        SuiteRow::new("app-eslint", &ctx.app_root, "kade", "fail", &format!("0 pass, 1 fail (app eslint rc={} — {})", rc, detail))
    })
}

// ───────────────────────── the owner map, once ─────────────────────────

fn fetch_json(url: &str, secs: u32) -> Option<String> {
    let o = Command::new("curl").args(["-sf", "-m", &secs.to_string(), url]).output().ok()?;
    if !o.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&o.stdout).into_owned())
}

/// Minimal extraction of `"key":"value"` string pairs per object in a `data`
/// array — enough for /tests (filePath, covers) and /domains (name, ownedBy)
/// without a JSON dependency (ADR-032 §1).
fn rows_of(json: &str, a: &str, b: &str) -> Vec<(String, String)> {
    // #4147 — escapes honoured; the first-quote reader cut `has zero =\"//…` at the backslash
    json_rows(json, a, b)
}

/// ONE read of the registry per run (the wrapper did 385): the owner map and
/// the registered (file, name) list for the census come from the same body.
fn read_registry(ctx: &Ctx) -> (HashMap<String, String>, Vec<(String, String)>) {
    let tests = fetch_json(&format!("{}/tests?limit=25000", ctx.owlapi), 30);
    let domains = fetch_json(&format!("{}/domains?limit=200", ctx.owlapi), 15);
    match (tests, domains) {
        (Some(t), Some(d)) => {
            let tv = rows_of(&t, "filePath", "covers");
            let dv = rows_of(&d, "name", "ownedBy");
            let m = owner_map(tv.iter().map(|(a, b)| (a.as_str(), b.as_str())), dv.iter().map(|(a, b)| (a.as_str(), b.as_str())));
            let registered = rows_of(&t, "filePath", "testName");
            eprintln!("owner map: {} file(s) attributed from the model (built once, #4145)", m.len());
            (m, registered)
        }
        _ => {
            eprintln!("!! owner map UNAVAILABLE — registry unreachable; every row falls back to the path rule");
            (HashMap::new(), Vec::new())
        }
    }
}

// ───────────────────────── phase 2: the runner, streamed ─────────────────────────

struct LaneResult {
    rows: Vec<SuiteRow>,
    rc: i32,
    /// #4247 — (filePath, testName, result) for every case the lane reported.
    /// #4271 — plus the REGISTERED name that case answers for: a `describe.each`
    /// block generates N cases from one registered row, so identity and join
    /// key are two facts, carried together on the line.
    cases: Vec<(String, String, String, String)>,
    /// #4271 — results the lane actually wrote to the ledger, from its own
    /// `nightly-stored|run|<n> of <m>` line. None when the lane never said,
    /// and the PipelineRun then omits the test grain rather than sending 0.
    stored: Option<usize>,
}

fn run_runner(ctx: &Ctx, box_over_load: bool) -> LaneResult {
    let exe = env_or("NIGHTLY_RUNNER_CMD", &std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "werk-test".into()));
    let _ = std::fs::create_dir_all(&ctx.fail_dir);
    let lane_path = format!("{}/_lane-output.log", ctx.fail_dir);
    let mut lane = OpenOptions::new().create(true).write(true).truncate(true).open(&lane_path).ok();
    let mut cmd = Command::new(&exe);
    if !exe.ends_with(".sh") {
        cmd.arg("--nightly");
    }
    cmd.env("CHORUS_ROOT", &ctx.root).env("CHORUS_HOME", &ctx.home).env("ROLE", &ctx.role).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let row = SuiteRow::new("runner", "werk-test-nightly", "silas", "fail", &format!("0 pass, 1 fail (runner could not start: {} — runner lanes DID NOT RUN, #3920/#3974)", e));
            ctx.append_log(&row.line());
            return LaneResult { rows: vec![row], rc: 127, cases: Vec::new(), stored: None };
        }
    };
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        for l in BufReader::new(stderr).lines().flatten() {
            eprintln!("{}", l);
            s.push_str(&l);
            s.push('\n');
        }
        s
    });
    let mut rows = Vec::new();
    let mut lane_text = String::new();
    // #4247 — the run's own per-test record, read from the lane's own output.
    let mut cases: Vec<(String, String, String, String)> = Vec::new();
    let mut stored: Option<usize> = None;
    let mut nudged: std::collections::HashSet<String> = std::collections::HashSet::new();
    for line in BufReader::new(stdout).lines().flatten() {
        if stop_requested() {
            let _ = child.kill();
            break;
        }
        if let Some(f) = lane.as_mut() {
            let _ = writeln!(f, "{}", line);
        }
        lane_text.push_str(&line);
        lane_text.push('\n');
        if let Some(c) = werk_test::nightly_run::parse_case_line(&line) {
            cases.push(c);
        }
        if let Some(n) = werk_test::nightly_run::parse_run_stored_line(&line) {
            stored = Some(n);
        }
        // #4168 — the existence probe is the werk's own tree, resolved from the
        // run's root. A relative path is joined to root; an absolute one is
        // taken as given (the app_root units arrive absolute).
        let exists = |p: &str| {
            let abs = if p.starts_with('/') { p.to_string() } else { format!("{}/{}", ctx.root, p) };
            std::path::Path::new(&abs).exists()
        };
        if let Some((row, contradiction)) = fold_unit_line(&line, &|p| ctx.owner(p), box_over_load, &exists) {
            if contradiction {
                eprintln!("nightly: REPORTER CONTRADICTION — row says pass with failures; recording fail (#3753 AC4, row-level)");
                ctx.spine("nightly.reporter.contradiction", &[("suite".into(), row.suite_name().into()), ("kind".into(), row.kind.clone())]);
            }
            // #4071 — a red row nudges its owner the moment it lands, once per unit
            if row.status == "fail" && nudged.insert(row.path.clone()) {
                ctx.nudge(&row.owner, &format!("nightly RED now: {} — {} (run still going; read it now, not at the end)", row.path, row.summary));
            }
            // append ONLY: under launchd stdout is the log itself, so a println
            // here doubled every row (400 duplicates on the 2026-09-11 19:16 run)
            ctx.append_log(&row.line());
            rows.push(row);
        } else {
            println!("{}", line);
        }
    }
    let status = child.wait().ok();
    let err_text = err_thread.join().unwrap_or_default();
    lane_text.push_str(&err_text);
    if let Some(f) = lane.as_mut() {
        let _ = write!(f, "{}", err_text);
    }
    let rc = status.and_then(|s| s.code()).unwrap_or(1);
    if rows.is_empty() {
        let reason = err_text.lines().last().unwrap_or("no output").to_string();
        let row = SuiteRow::new("runner", "werk-test-nightly", "silas", "fail", &format!("0 pass, 1 fail (runner produced no unit results rc={} — {})", rc, reason));
        ctx.append_log(&row.line());
        rows.push(row);
    }
    // per-unit failure detail (#4004): this unit's slice, not the whole lane
    for r in rows.iter().filter(|r| r.status == "fail" || r.status == "unmeasured") {
        let unit = r.path.strip_prefix("platform/services/").unwrap_or(&r.path);
        let slice = unit_slice(&lane_text, unit).join("\n");
        ctx.write_fail_log(r, &format!("{}\n# full lane output: {}\n", slice, lane_path));
    }
    LaneResult { rows, rc, cases, stored }
}

// ───────────────────────── phase 3: the census, from the run's own record ─────────────────────────

// #4154 — the census as a RED ROW is gone, and stays gone. It was a second
// crawler inside the runner: the graph's registry is the crawler's to keep
// current (ADR-033), and 33 of the 37 red rows on 2026-09-12 06:00 were that
// row reporting files a land had deleted.
//
// #4247 brings back only the REPORT, never the verdict. Jeff, 2026-09-20:
// "rather than stating everything in a different unit of measure can we be
// consistent" — the run said "31 red" (suites) beside "8,537 ran" (tests) and
// the 154 registered tests with no result were nameless, inferred by matching
// two lists. These lines name them and count in one unit. They are written to
// the log and the spine; no SuiteRow is built, so a stale registry can report
// a gap without turning the night red.
fn report_no_result(ctx: &Ctx, registered: &[(String, String)], cases: &[(String, String, String, String)]) {
    if registered.is_empty() {
        ctx.append_log("RUN|tally|registry unreadable — the run cannot say what it did not run");
        return;
    }
    let tally = werk_test::nightly_run::registered_test_tally(registered, cases);
    ctx.append_log(&format!("RUN|tally|{}", tally));
    eprintln!("nightly: {}", tally);
    let missing = werk_test::nightly_run::tests_with_no_result(registered, cases);
    for (f, n) in missing.iter().take(200) {
        ctx.append_log(&format!("nightly-no-result|{}|{}", f, n));
    }
    if missing.len() > 200 {
        ctx.append_log(&format!("nightly-no-result|… {} more", missing.len() - 200));
    }
    ctx.spine(
        "nightly.tests.no_result",
        &[
            ("registered".into(), registered.len().to_string()),
            ("ran".into(), cases.len().to_string()),
            ("no_result".into(), missing.len().to_string()),
        ],
    );
}

// ───────────────────────── the run ─────────────────────────

/// Read-only modes the script used to answer for live callers:
/// `--last-run` (daily-review-quality), `--load-gate` (load-reclassify),
/// `--lock-probe` (the single-flight tests).
pub fn run_mode(args: &[String]) -> Option<Result<i32, String>> {
    let home_dir = env_or("HOME", "/tmp");
    if args.iter().any(|a| a == "--last-run") {
        let log = env_or("NIGHTLY_LOG_PATH", &format!("{}/Library/Logs/Chorus/nightly-suites.log", home_dir));
        let meta = match std::fs::metadata(&log) {
            Ok(m) => m,
            Err(_) => {
                println!("SUITE|meta|{}|silas|fail|0 pass, 1 fail (nightly log MISSING — no run to read, run the 03:00 nightly)", log);
                return Some(Ok(1));
            }
        };
        let _ = &meta;
        let body = std::fs::read_to_string(&log).unwrap_or_default();
        // #4251 — staleness is "has a run finished since the last scheduled
        // slot", read from the job's own plist. The old check was the LOG's
        // mtime against 26h: on 2026-09-21 a 21:11 hand run had written the
        // file, so the mtime was fresh, the 03:00 job had never launched, and
        // the readout showed the night before's four reds as that morning's.
        let plist = format!(
            "{}/Library/LaunchAgents/com.chorus.nightly-suites.plist",
            std::env::var("HOME").unwrap_or_default()
        );
        let slots = werk_test::nightly_run::slots_from_plist(
            &std::fs::read_to_string(&plist).unwrap_or_default(),
        );
        let rows = last_run_rows(&body);
        if !slots.is_empty() {
            let reds = rows.iter().filter(|l| l.contains("|fail|")).count();
            let summary = format!("{} red", reds);
            let reason = launchd_exit_reason();
            if let Some(line) = werk_test::nightly_run::unmeasured_since_slot(
                &now_stamp(),
                &slots,
                werk_test::nightly_run::last_complete_stamp(&body).as_deref(),
                &summary,
                reason.as_deref(),
            ) {
                // REPLACES the counts — a stale total must never read as current.
                println!("{}", line);
                return Some(Ok(1));
            }
        }
        for l in rows {
            println!("{}", l);
        }
        return Some(Ok(0));
    }
    if args.iter().any(|a| a == "--load-gate") {
        let (ok, line) = load_gate();
        println!("{}", line);
        return Some(Ok(if ok { 0 } else { 1 }));
    }
    // `--classify <verdict> <summary>`: the row-level fold, for the wrapper's
    // tests (#3753 AC2) — timeouts fold to unmeasurable only under load.
    if let Some(i) = args.iter().position(|a| a == "--classify") {
        let verdict = args.get(i + 1).cloned().unwrap_or_default();
        let summary = args.get(i + 2).cloned().unwrap_or_default();
        let (ok, _) = load_gate();
        let (v, _) = werk_test::nightly_run::classify_verdict(&verdict, &summary, !ok);
        println!("{}", v);
        return Some(Ok(0));
    }
    if args.iter().any(|a| a == "--lock-probe") {
        let lockdir = env_or("NIGHTLY_LOCKDIR", &format!("{}/chorus-nightly-suites.lock.d", env_or("TMPDIR", "/tmp").trim_end_matches('/')));
        match acquire_lock(&lockdir) {
            Ok(()) => {
                println!("ACQUIRED");
                let _ = std::fs::remove_dir_all(&lockdir);
            }
            Err(why) => println!("REFUSED {}", why),
        }
        return Some(Ok(0));
    }
    None
}

pub fn run_all(args: &[String]) -> Result<i32, String> {
    if let Some(r) = run_mode(args) {
        return r;
    }
    install_stop_handler();
    let root = std::env::var("CHORUS_ROOT").or_else(|_| std::env::var("CHORUS_HOME")).unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".into());
    let home = env_or("CHORUS_HOME", &root);
    let home_dir = env_or("HOME", "/tmp");
    let mut ctx = Ctx {
        app_root: env_or("APP_ROOT", "/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site"),
        log: env_or("NIGHTLY_LOG_PATH", &format!("{}/Library/Logs/Chorus/nightly-suites.log", home_dir)),
        fail_dir: env_or("NIGHTLY_FAIL_DIR", &format!("{}/.chorus/nightly-failures", home_dir)),
        lockdir: env_or("NIGHTLY_LOCKDIR", &format!("{}/chorus-nightly-suites.lock.d", env_or("TMPDIR", "/tmp").trim_end_matches('/'))),
        role: env_or("DEPLOY_ROLE", &env_or("CHORUS_ROLE", "system")),
        run_id: env_or("NIGHTLY_RUN_ID", &format!("nr-{}-{}", epoch(), std::process::id())),
        owlapi: env_or("OWLAPI", "http://localhost:3360"),
        api: env_or("NIGHTLY_API", "http://localhost:3340"),
        ops_nudge: env_or("OPS_NUDGE", &format!("{}/platform/scripts/ops-nudge", root)),
        no_nudge: std::env::var("NIGHTLY_NO_NUDGE").map(|v| v == "1").unwrap_or(false),
        owners: HashMap::new(),
        root: root.clone(),
        home,
    };
    // #3722 — a werk-rooted run never writes the team's log or nudges the team
    if ctx.root.contains("/chorus-werk/") {
        if std::env::var("NIGHTLY_LOG_PATH").map(|v| v.is_empty()).unwrap_or(true) {
            ctx.log = format!("/tmp/nightly-{}.log", Path::new(&ctx.root).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
        }
        ctx.no_nudge = true;
        eprintln!("nightly: WERK RUN — isolated to {}, team nudge suppressed (#3722)", ctx.log);
    }
    if let Err(why) = acquire_lock(&ctx.lockdir) {
        eprintln!("nightly-suites: REFUSED — {} — one run at a time (single-flight, #3597/#4008), lock {}", why, ctx.lockdir);
        ctx.nudge("silas", &format!("nightly-suites: scheduled run SKIPPED — {} (#4037/#4008). The run did not happen; check the straggler.", why));
        return Ok(0);
    }
    let result = run_locked(&mut ctx, args);
    let _ = std::fs::remove_dir_all(&ctx.lockdir);
    result
}

fn run_locked(ctx: &mut Ctx, _args: &[String]) -> Result<i32, String> {
    // #3753 — a loaded box defers, then declares the night unmeasurable
    let defer_max: u64 = env_or("NIGHTLY_LOAD_DEFER_SECS", "900").parse().unwrap_or(900);
    let recheck: u64 = env_or("NIGHTLY_LOAD_RECHECK_SECS", "60").parse().unwrap_or(60);
    let (mut ok, mut lg) = load_gate();
    let mut deferred = 0u64;
    while !ok && deferred < defer_max {
        eprintln!("nightly: box loaded ({}) — deferring {}s ({}/{}s used, #3753)", lg, recheck, deferred, defer_max);
        std::thread::sleep(Duration::from_secs(recheck));
        deferred += recheck;
        let g = load_gate();
        ok = g.0;
        lg = g.1;
    }
    if !ok {
        ctx.append_log(&format!("RUN|unmeasurable|{}|{}|deferred={}s", now_stamp(), lg, deferred));
        ctx.spine("nightly.run.unmeasurable", &[("deferred_s".into(), deferred.to_string()), ("reason".into(), "load".into())]);
        ctx.spine("nightly.run.summary", &[("suites".into(), "0".into()), ("failed".into(), "0".into()), ("unmeasurable".into(), "all".into()), ("zero_red".into(), "unmeasurable".into())]);
        eprintln!("nightly: UNMEASURABLE — {} after {}s defer; zero-red bar NOT measured tonight (#3753)", lg, deferred);
        return Ok(0);
    }
    let t0 = Instant::now();
    // #4271 — the run's id, minted ONCE. The log brackets the run with it and
    // the graph's PipelineRun is named from it, so the two rows join. It used
    // to be re-read at emit time, i.e. when the run FINISHED.
    let started_at = now_stamp();
    if !ctx.append_log(&format!("RUN|start|{}|pid={}", started_at, std::process::id())) {
        eprintln!("nightly: WARNING — cannot append to {}; this run's results reach NOBODY", ctx.log);
    }
    let (owners, registered) = read_registry(ctx);
    ctx.owners = owners;
    let mut rows: Vec<SuiteRow> = Vec::new();
    let push = |ctx: &Ctx, r: SuiteRow, rows: &mut Vec<SuiteRow>| {
        ctx.append_log(&r.line());
        rows.push(r);
    };
    // #4278 — sample every com.chorus.* agent's pid at the start; the same
    // sample at the end tells whether one died under the run. On 2026-09-23
    // launchd SIGTERMed chorus-hooks twice inside the nightly (10:01, 10:09)
    // and 424 unit suites said pass.
    let daemons_at_start = sample_daemons();
    let run_clock = Instant::now();
    if std::env::var("NIGHTLY_LEGS_NOOP").is_err() {
        // #4290 — first, because it is the prerequisite for everything that
        // reads the graph: is the graph what git is, on every crawler domain?
        push(ctx, leg_crawler_validate(ctx), &mut rows);
        if let Some(r) = leg_lint(ctx) {
            push(ctx, r, &mut rows);
        }
        for r in leg_coverage(ctx) {
            push(ctx, r, &mut rows);
        }
        if let Some(r) = leg_smoke(ctx) {
            push(ctx, r, &mut rows);
        }
        if let Some(r) = leg_app_eslint(ctx) {
            push(ctx, r, &mut rows);
        }
    }
    let (over, _) = load_gate();
    let lane = run_runner(ctx, !over);
    if stop_requested() {
        let sig = STOP_SIGNAL.load(Ordering::SeqCst);
        ctx.append_log(&format!("RUN|stopped|{}|signal={} pid={}", now_stamp(), if sig == 2 { "INT" } else { "TERM" }, std::process::id()));
        let _ = std::fs::remove_dir_all(&ctx.lockdir);
        std::process::exit(if sig == 2 { 130 } else { 143 });
    }
    rows.extend(lane.rows.iter().cloned());
    // #4278 — the production lanes: browser flows against the live surfaces,
    // the bdd features, and two perf lines (agents steady? run within budget?).
    // Jeff, 2026-09-23: "issues like this illustrate why i want api and ui runs
    // and bdd and perf too in our nightly runs." Each lane reports UNMEASURED
    // when it could not measure, never pass.
    // #4277 — under the hermetic seam (NIGHTLY_LEGS_NOOP) these lanes stay
    // off like the others: a fixture run must not launch Playwright, cucumber
    // or launchctl, and 4145's row count reads the runner's rows alone.
    if std::env::var("NIGHTLY_LEGS_NOOP").is_err() {
        push(ctx, leg_ui(ctx), &mut rows);
        push(ctx, leg_bdd(ctx), &mut rows);
        push(ctx, leg_daemons(ctx, &daemons_at_start, &sample_daemons()), &mut rows);
        push(ctx, leg_duration(run_clock.elapsed().as_secs()), &mut rows);
    }
    // #4247 — the census from the run's own record, in one unit, before the
    // completion line so a reader sees the tally with the run it belongs to.
    report_no_result(ctx, &registered, &lane.cases);
    ctx.append_log(&format!("RUN|complete|{}|suites={}", now_stamp(), rows.len()));
    // the tail: summary, record, per-row events, nudges, readout
    ctx.spine("nightly.run.summary", &run_summary_fields(&rows));
    emit_pipeline_run(
        ctx,
        &rows,
        t0.elapsed().as_millis(),
        &started_at,
        werk_test::nightly_run::run_test_counts(&lane.cases, lane.stored),
    );
    for r in &rows {
        let reason = if r.status == "fail" {
            let p = format!("{}/{}", ctx.fail_dir, fail_log_name(&r.kind, &r.path));
            std::fs::read_to_string(&p).ok().and_then(|t| {
                t.lines()
                    .find(|l| l.starts_with("!! ") && (l.contains("FAILED:") || l.contains("killed") || l.contains("FAIL LOUD") || l.contains("unavailable")))
                    .or_else(|| t.lines().rev().find(|l| ["error", "panic", "fail", "assert"].iter().any(|k| l.to_ascii_lowercase().contains(k))))
                    .map(|l| l.replace(['|', '"', '\n'], " ").chars().take(200).collect::<String>())
            })
        } else {
            None
        };
        let (fields, contradiction) = suite_result_fields(r, reason.as_deref());
        if contradiction {
            ctx.spine("nightly.reporter.contradiction", &[("suite".into(), r.suite_name().into()), ("kind".into(), r.kind.clone())]);
        }
        ctx.spine("test.suite.result", &fields);
    }
    if !ctx.no_nudge {
        let sec_owner = env_or("NIGHTLY_SECURITY_OWNER", "silas");
        // #4180 — the crawler's last scheduled pass rides the TOTAL line, so a
        // nightly that never fired or fired red is seen where the reds are.
        let crawl_log = env_or("CRAWL_NIGHTLY_LOG", &format!("{}/Library/Logs/Chorus/crawl-nightly.log", env_or("HOME", "/tmp")));
        let crawl_text = std::fs::read_to_string(&crawl_log).ok();
        let crawl = crawl_line(crawl_text.as_deref());
        for (to, msg) in notify_messages(&rows, &sec_owner, &crawl) {
            ctx.nudge(&to, &msg);
        }
        deliver_readout(ctx);
    }
    let _ = lane.rc;
    Ok(0)
}

fn emit_pipeline_run(
    ctx: &Ctx,
    rows: &[SuiteRow],
    duration_ms: u128,
    started_at: &str,
    tests: Option<werk_test::nightly_run::RunTestCounts>,
) {
    let tok = Command::new(format!("{}/platform/scripts/chorus-identity-token", ctx.root))
        .arg(env_or("NIGHTLY_PIPELINE_ROLE", "wren"))
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|t| !t.is_empty());
    let Some(tok) = tok else {
        eprintln!("nightly: pipeline-run emit SKIPPED — no identity token minted");
        return;
    };
    let name = werk_test::nightly_run::pipeline_run_name(started_at);
    let body = pipeline_run_body(rows, &name, &env_or("CHORUS_TRACE_ID", &format!("nightly-{}", epoch())), duration_ms, tests);
    let o = Command::new("curl")
        .args(["-s", "--max-time", "10", "-o", "/dev/null", "-w", "%{http_code}", "-X", "POST"])
        .arg(format!("{}/pipelineruns", ctx.owlapi))
        .arg("-H")
        .arg(format!("Authorization: Bearer {}", tok))
        .args(["-H", "Content-Type: application/json", "-d", &body])
        .output();
    let code = o.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
    let failed = rows.iter().filter(|r| r.status == "fail").count();
    if code.starts_with('2') {
        eprintln!("nightly: pipeline-run recorded ({}, {} suites, {} failed)", if failed == 0 { "green" } else { "red" }, rows.len(), failed);
    } else {
        eprintln!("nightly: pipeline-run emit REFUSED HTTP {}", code);
    }
}

fn deliver_readout(ctx: &Ctx) {
    let url = format!("{}/api/chorus/nightly/runs/latest?format=text", ctx.api);
    let o = Command::new("curl").args(["-s", "--max-time", "15", "-w", "\n%{http_code}", &url]).output().ok();
    let (text, code) = o
        .map(|o| {
            let s = String::from_utf8_lossy(&o.stdout).into_owned();
            let (t, c) = s.rsplit_once('\n').unwrap_or(("", "000"));
            (t.to_string(), c.trim().to_string())
        })
        .unwrap_or_default();
    if code == "200" && !text.trim().is_empty() {
        eprintln!("nightly: readout delivered to jeff:\n{}", text);
        ctx.nudge("jeff", &text);
    } else {
        eprintln!("nightly: readout UNAVAILABLE — {} answered HTTP {}; nudging jeff without numbers", ctx.api, code);
        ctx.nudge("jeff", &format!("nightly finished but its readout could not be built ({} answered HTTP {}). No numbers until the api answers: {}/nightly", ctx.api, code, ctx.api));
    }
}

// ───────────────────────────── #4278 — production lanes ─────────────────────────────
//
// UI (Playwright), bdd (cucumber-js), and two perf lines (agent uptime across
// the run, run duration vs budget). The pure parsers below are what the unit
// tests prove; the legs are thin shells around a seam command so a fixture
// can stand in for the real tool.

/// Counts from cucumber-js's `summary` formatter:
/// `35 scenarios (2 failed, 3 undefined, 30 passed)` / `162 steps (…)`.
#[derive(Debug, Default, PartialEq, Clone)]
pub struct CukeCounts {
    pub scenarios: usize,
    pub passed: usize,
    pub failed: usize,
    pub undefined: usize,
    pub skipped: usize,
    pub pending: usize,
    pub steps: usize,
}

pub fn parse_cucumber_summary(out: &str) -> Option<CukeCounts> {
    let mut c = CukeCounts::default();
    let mut saw = false;
    for raw in out.lines() {
        // strip ANSI colour: ESC [ … m
        let mut l = String::new();
        let mut in_esc = false;
        for ch in raw.chars() {
            if ch == '\u{1b}' { in_esc = true; continue; }
            if in_esc { if ch == 'm' { in_esc = false; } continue; }
            l.push(ch);
        }
        let l = l.trim();
        let (total, rest, is_scn) = if let Some(i) = l.find(" scenarios (") {
            (l[..i].trim().parse::<usize>().ok(), &l[i + " scenarios (".len()..], true)
        } else if let Some(i) = l.find(" scenario (") {
            (l[..i].trim().parse::<usize>().ok(), &l[i + " scenario (".len()..], true)
        } else if let Some(i) = l.find(" steps (") {
            (l[..i].trim().parse::<usize>().ok(), &l[i + " steps (".len()..], false)
        } else if let Some(i) = l.find(" step (") {
            (l[..i].trim().parse::<usize>().ok(), &l[i + " step (".len()..], false)
        } else { continue };
        let Some(total) = total else { continue };
        saw = true;
        if is_scn { c.scenarios = total; } else { c.steps = total; continue; }
        for part in rest.trim_end_matches(')').split(',') {
            let w: Vec<&str> = part.trim().split_whitespace().collect();
            if w.len() != 2 { continue; }
            let n: usize = match w[0].parse() { Ok(n) => n, Err(_) => continue };
            match w[1] {
                "passed" => c.passed = n,
                "failed" => c.failed = n,
                "undefined" => c.undefined = n,
                "skipped" => c.skipped = n,
                "pending" => c.pending = n,
                _ => {}
            }
        }
    }
    if saw { Some(c) } else { None }
}

/// The bdd verdict: undefined steps mean the feature could not be measured
/// (a scenario that skips every step is not a pass); a failed scenario is red;
/// zero scenarios is nothing measured.
pub fn cucumber_verdict(c: Option<&CukeCounts>) -> (&'static str, String) {
    match c {
        None => ("unmeasured", "0 pass, 0 fail (UNMEASURED — cucumber-js produced no summary)".into()),
        Some(c) if c.scenarios == 0 => ("unmeasured", "0 pass, 0 fail (UNMEASURED — no scenarios selected)".into()),
        Some(c) if c.undefined > 0 => ("unmeasured", format!(
            "0 pass, 0 fail (UNMEASURED — {} of {} scenarios have undefined steps; {} steps)", c.undefined, c.scenarios, c.steps)),
        Some(c) if c.failed > 0 => ("fail", format!("{} pass, {} fail ({} scenarios, {} steps)", c.passed, c.failed, c.scenarios, c.steps)),
        Some(c) => ("pass", format!("{} pass, 0 fail ({} scenarios, {} steps, {} skipped)", c.passed, c.scenarios, c.steps, c.skipped)),
    }
}

/// `launchctl list` rows for com.chorus.* → label → pid (None when '-').
pub fn parse_launchctl(out: &str) -> std::collections::BTreeMap<String, Option<u32>> {
    let mut m = std::collections::BTreeMap::new();
    for l in out.lines() {
        let cols: Vec<&str> = l.split('\t').collect();
        if cols.len() < 3 || !cols[2].starts_with("com.chorus.") { continue; }
        m.insert(cols[2].trim().to_string(), cols[0].trim().parse::<u32>().ok());
    }
    m
}

/// Agents that were running at the start and are not the same process at the
/// end — restarted (pid changed) or gone (no pid). Agents that were not running
/// at the start are not this line's business (a one-shot job is allowed to run).
pub fn daemon_restarts(
    start: &std::collections::BTreeMap<String, Option<u32>>,
    end: &std::collections::BTreeMap<String, Option<u32>>,
) -> Vec<String> {
    let mut out = Vec::new();
    for (label, pid0) in start {
        let Some(p0) = pid0 else { continue };
        match end.get(label).copied().flatten() {
            Some(p1) if p1 == *p0 => {}
            Some(p1) => out.push(format!("{} restarted (pid {} → {})", label, p0, p1)),
            None => out.push(format!("{} gone (was pid {})", label, p0)),
        }
    }
    out
}

/// Run duration against its budget. kind=perf, so the page folds a fail to
/// "slow (speed, not breakage)" (#4136) — over budget is a perf red, not a
/// product red.
pub fn duration_verdict(secs: u64, budget_secs: u64) -> (&'static str, String) {
    if secs <= budget_secs {
        ("pass", format!("1 pass, 0 fail (run took {}s, budget {}s)", secs, budget_secs))
    } else {
        ("fail", format!("0 pass, 1 fail (run took {}s, over the {}s budget by {}s)", secs, budget_secs, secs - budget_secs))
    }
}

fn sample_daemons() -> std::collections::BTreeMap<String, Option<u32>> {
    let cmd = env_or("NIGHTLY_LAUNCHCTL", "launchctl list");
    let out = Command::new("bash").arg("-c").arg(&cmd).output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    parse_launchctl(&out)
}

fn leg_daemons(
    _ctx: &Ctx,
    start: &std::collections::BTreeMap<String, Option<u32>>,
    end: &std::collections::BTreeMap<String, Option<u32>>,
) -> SuiteRow {
    let steady = start.values().filter(|p| p.is_some()).count();
    if steady == 0 {
        return SuiteRow::new("perf", "daemons:com.chorus.*", "silas", "unmeasured",
            "0 pass, 0 fail (UNMEASURED — launchctl listed no running com.chorus.* agent at run start)");
    }
    let restarts = daemon_restarts(start, end);
    if restarts.is_empty() {
        SuiteRow::new("perf", "daemons:com.chorus.*", "silas", "pass",
            &format!("1 pass, 0 fail ({} agents kept their pid across the run)", steady))
    } else {
        SuiteRow::new("perf", "daemons:com.chorus.*", "silas", "fail",
            &format!("0 pass, 1 fail ({} of {} agents did not survive the run: {})", restarts.len(), steady, restarts.join("; ")))
    }
}

/// #4290 — crawler-validate: the crawler's own read-only reconcile, both
/// directions, every crawler-generated domain, as a nightly lane. Jeff,
/// 2026-09-24: "its our data quality control for crawler - git and graph in
/// tight synch on all domains generated from crawler"; "its not an ad hoc
/// query its a control report that shows gaps in both directions". The lane
/// reads the VALIDATE| lines the crawler prints (one per domain) and is red
/// on any gap, UNMEASURED when a domain could not be read or no lines came.
fn leg_crawler_validate(ctx: &Ctx) -> SuiteRow {
    let bin = env_or(
        "NIGHTLY_CRAWL_BIN",
        &format!("{}/.chorus/bin/chorus-crawl", env_or("HOME", "/tmp")),
    );
    let path = "chorus-crawl --reconcile";
    if !Path::new(&bin).is_file() {
        return SuiteRow::new(
            "crawler-validate",
            path,
            "kade",
            "unmeasured",
            &format!("0 pass, 0 fail (UNMEASURED — no crawler binary at {bin})"),
        );
    }
    let mut c = Command::new(&bin);
    c.arg("--reconcile")
        .env("CHORUS_ROOT", &ctx.root)
        .env("CHORUS_ROLE", env_or("NIGHTLY_CRAWL_ROLE", "kade"))
        .current_dir(&ctx.root);
    let (rc, out) = run_capped(c, Duration::from_secs(1200));
    let (status, summary) = crawler_validate_verdict(rc, &out);
    let row = SuiteRow::new("crawler-validate", path, "kade", status, &summary);
    if status != "pass" {
        ctx.write_fail_log(&row, &out);
    }
    row
}

/// One parsed `VALIDATE|domain|class|tree=|graph=|missing=|stale=|measured` line.
struct ValidateLine {
    domain: String,
    class: String,
    missing: usize,
    stale: usize,
    measured: bool,
}

fn parse_validate_lines(out: &str) -> Vec<ValidateLine> {
    out.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.trim().strip_prefix("VALIDATE|")?.split('|').collect();
            if f.len() < 7 {
                return None;
            }
            let num = |s: &str, k: &str| s.strip_prefix(k).and_then(|v| v.parse::<usize>().ok());
            Some(ValidateLine {
                domain: f[0].to_string(),
                class: f[1].to_string(),
                missing: num(f[4], "missing=")?,
                stale: num(f[5], "stale=")?,
                measured: f[6] == "measured",
            })
        })
        .collect()
}

/// pass = every domain measured and 0/0 and the crawler exited 0; fail = any
/// gap; unmeasured = a domain the graph could not answer for, or no lines at
/// all (a clean verdict over nothing read is the hollow gate, #3734).
fn crawler_validate_verdict(rc: i32, out: &str) -> (&'static str, String) {
    let rows = parse_validate_lines(out);
    if rows.is_empty() {
        return (
            "unmeasured",
            format!("0 pass, 0 fail (UNMEASURED — the crawler printed no VALIDATE rows, rc={rc})"),
        );
    }
    let detail: Vec<String> = rows
        .iter()
        .map(|r| {
            if r.measured {
                format!("{} {}/{}", r.domain, r.missing, r.stale)
            } else {
                format!("{} UNMEASURED", r.domain)
            }
        })
        .collect();
    let unread: Vec<String> = rows
        .iter()
        .filter(|r| !r.measured)
        .map(|r| format!("{} ({})", r.domain, r.class))
        .collect();
    if !unread.is_empty() {
        return (
            "unmeasured",
            format!(
                "0 pass, 0 fail (UNMEASURED — {} row(s) the graph could not answer for: {})",
                unread.len(),
                unread.join(", ")
            ),
        );
    }
    let gaps: usize = rows.iter().map(|r| r.missing + r.stale).sum();
    if gaps == 0 && rc == 0 {
        (
            "pass",
            format!(
                "1 pass, 0 fail ({} domain rows, git = graph both ways: {})",
                rows.len(),
                detail.join(" · ")
            ),
        )
    } else {
        (
            "fail",
            format!(
                "0 pass, 1 fail ({gaps} gap(s) git↔graph as missing/stale: {}; rc={rc})",
                detail.join(" · ")
            ),
        )
    }
}

fn leg_duration(secs: u64) -> SuiteRow {
    // #4277 — the budget is a regression guard, so it sits above the measured
    // run, not at a wish: the two full runs of 2026-09-23 took 67 and 71 min
    // (4033s, 4252s) and the first budget (3600s) went red on a normal night.
    let budget: u64 = env_or("NIGHTLY_RUN_BUDGET_S", "5400").parse().unwrap_or(5400);
    let (status, summary) = duration_verdict(secs, budget);
    SuiteRow::new("perf", "nightly:duration", "kade", status, &summary)
}

fn leg_ui(ctx: &Ctx) -> SuiteRow {
    let path = "proving/flows";
    let cmd = env_or("NIGHTLY_PLAYWRIGHT_CMD", "npx --no-install playwright test --reporter=line");
    let mut c = Command::new("bash");
    c.arg("-c").arg(&cmd).current_dir(&ctx.root)
        .env("CHORUS_CONTEXT", "")
        .env("CLEARING_URL", env_or("CLEARING_URL", "http://localhost:3470"));
    let (rc, out) = run_capped(c, Duration::from_secs(1800));
    let owner = ctx.owner(path);
    match werk_test::parse_playwright_summary(&out) {
        None => SuiteRow::new("ui", path, &owner, "unmeasured", &format!(
            "0 pass, 0 fail (UNMEASURED — playwright produced no summary, rc={}: {})", rc,
            out.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().chars().take(120).collect::<String>())),
        Some((passed, failed)) if failed > 0 || rc != 0 => {
            let first = werk_test::playwright_failure_lines(&out).into_iter().take(3).collect::<Vec<_>>().join("; ");
            SuiteRow::new("ui", path, &owner, "fail", &format!("{} pass, {} fail (rc={}) {}", passed, failed, rc, first))
        }
        Some((passed, _)) => {
            let skipped = werk_test::parse_playwright_skipped(&out);
            SuiteRow::new("ui", path, &owner, "pass", &format!("{} pass, 0 fail ({} skipped)", passed, skipped))
        }
    }
}

fn leg_bdd(ctx: &Ctx) -> SuiteRow {
    let path = "platform/tests/features";
    let dir = format!("{}/platform/tests", ctx.root);
    if !Path::new(&format!("{}/node_modules/.bin/cucumber-js", dir)).exists()
        && std::env::var("NIGHTLY_CUCUMBER_CMD").is_err()
    {
        return SuiteRow::new("bdd", path, "kade", "unmeasured",
            "0 pass, 0 fail (UNMEASURED — platform/tests/node_modules/.bin/cucumber-js is not installed)");
    }
    let cmd = env_or("NIGHTLY_CUCUMBER_CMD", "npx --no-install cucumber-js --format summary");
    let mut c = Command::new("bash");
    c.arg("-c").arg(&cmd).current_dir(&dir).env("CHORUS_CONTEXT", "");
    let (_rc, out) = run_capped(c, Duration::from_secs(1800));
    let counts = parse_cucumber_summary(&out);
    let (status, summary) = cucumber_verdict(counts.as_ref());
    SuiteRow::new("bdd", path, &ctx.owner(path), status, &summary)
}

#[cfg(test)]
mod lanes_4278 {
    use super::*;

    #[test]
    fn cucumber_summary_parses_every_bucket() {
        let out = "35 scenarios (2 failed, 3 undefined, 30 passed)\n162 steps (2 failed, 6 undefined, 150 passed, 4 skipped)\n0m01.2s\n";
        let c = parse_cucumber_summary(out).unwrap();
        assert_eq!((c.scenarios, c.passed, c.failed, c.undefined, c.steps), (35, 30, 2, 3, 162));
        // ANSI colour, as the real formatter prints it
        let ansi = "35 scenarios (\u{1b}[36m35 skipped\u{1b}[39m)\n162 steps (\u{1b}[36m162 skipped\u{1b}[39m)\n";
        let c = parse_cucumber_summary(ansi).unwrap();
        assert_eq!((c.scenarios, c.skipped, c.steps), (35, 35, 162));
    }

    #[test]
    fn bdd_verdict_separates_pass_fail_and_unmeasured() {
        let ok = CukeCounts { scenarios: 35, passed: 35, steps: 162, ..Default::default() };
        assert_eq!(cucumber_verdict(Some(&ok)).0, "pass");
        let red = CukeCounts { scenarios: 35, passed: 33, failed: 2, steps: 162, ..Default::default() };
        assert_eq!(cucumber_verdict(Some(&red)).0, "fail");
        // NEGATIVE PROOF (#3734): a feature whose steps are undefined must not read as pass
        let undef = CukeCounts { scenarios: 35, passed: 30, undefined: 5, steps: 162, ..Default::default() };
        let (v, s) = cucumber_verdict(Some(&undef));
        assert_eq!(v, "unmeasured");
        assert!(s.contains("5 of 35 scenarios have undefined steps"), "{s}");
        assert_eq!(cucumber_verdict(None).0, "unmeasured");
        assert_eq!(cucumber_verdict(Some(&CukeCounts::default())).0, "unmeasured");
    }

    #[test]
    fn launchctl_rows_and_restarts() {
        let start = parse_launchctl("17630\t0\tcom.chorus.api\n-\t0\tcom.chorus.index-artifacts\n5020\t0\tcom.chorus.hooks\n99\t0\tcom.gathering.app\n");
        assert_eq!(start.len(), 3, "only com.chorus.* rows: {start:?}");
        assert_eq!(start["com.chorus.api"], Some(17630));
        assert_eq!(start["com.chorus.index-artifacts"], None);
        // steady
        let same = start.clone();
        assert!(daemon_restarts(&start, &same).is_empty());
        // NEGATIVE PROOF: the 2026-09-23 shape — hooks restarted, api steady
        let end = parse_launchctl("17630\t0\tcom.chorus.api\n-\t0\tcom.chorus.index-artifacts\n79676\t0\tcom.chorus.hooks\n");
        let r = daemon_restarts(&start, &end);
        assert_eq!(r, vec!["com.chorus.hooks restarted (pid 5020 → 79676)".to_string()]);
        // gone entirely
        let gone = parse_launchctl("17630\t0\tcom.chorus.api\n-\t0\tcom.chorus.hooks\n");
        assert_eq!(daemon_restarts(&start, &gone), vec!["com.chorus.hooks gone (was pid 5020)".to_string()]);
        // a one-shot that was not running at the start is not a restart
        let started_later = parse_launchctl("17630\t0\tcom.chorus.api\n4242\t0\tcom.chorus.index-artifacts\n5020\t0\tcom.chorus.hooks\n");
        assert!(daemon_restarts(&start, &started_later).is_empty());
    }

    /// #4290 NEGATIVE PROOF (#3734): the lane must go red on the state it
    /// exists to catch — one file in git without a row and one row without a
    /// file — and must name the domain with its two counts.
    #[test]
    fn crawler_validate_is_red_on_a_missing_file_and_a_stale_row() {
        let out = "chorus-crawl: reconcile: clean\nVALIDATE|code|CodeFile|tree=6310|graph=6310|missing=0|stale=0|measured\nVALIDATE|tests|Test (files)|tree=1094|graph=1094|missing=1|stale=1|measured\n";
        let (v, s) = crawler_validate_verdict(1, out);
        assert_eq!(v, "fail");
        assert!(s.contains("2 gap(s)"), "{s}");
        assert!(s.contains("tests 1/1"), "{s}");
        assert!(s.contains("code 0/0"), "{s}");
    }

    /// Control: every row 0/0, measured, rc 0 → pass, and the pass line still
    /// carries the rows so a reader sees what was compared.
    #[test]
    fn crawler_validate_passes_only_when_every_row_is_zero_both_ways() {
        let out = "VALIDATE|code|CodeFile|tree=6310|graph=6310|missing=0|stale=0|measured\nVALIDATE|tests|Test (files)|tree=1094|graph=1094|missing=0|stale=0|measured\n";
        let (v, s) = crawler_validate_verdict(0, out);
        assert_eq!(v, "pass");
        assert!(s.contains("2 domain rows"), "{s}");
        // rc 1 with zero gaps is not a pass either: the crawler saw something the lines did not carry
        assert_eq!(crawler_validate_verdict(1, out).0, "fail");
    }

    /// NEGATIVE PROOF: zero gaps over a domain the graph could not answer for
    /// is UNMEASURED, never pass; and no VALIDATE lines at all is UNMEASURED
    /// even at rc 0 (a deleted or renamed check must fail loudly).
    #[test]
    fn crawler_validate_never_passes_on_nothing_read() {
        let out = "VALIDATE|code|CodeFile|tree=6310|graph=6310|missing=0|stale=0|measured\nVALIDATE|logs|LogSource|tree=133|graph=0|missing=0|stale=0|UNMEASURED\n";
        let (v, s) = crawler_validate_verdict(0, out);
        assert_eq!(v, "unmeasured");
        assert!(s.contains("logs (LogSource)"), "{s}");
        let (v, s) = crawler_validate_verdict(0, "chorus-crawl: reconcile: clean — the graph matches the tree\n");
        assert_eq!(v, "unmeasured");
        assert!(s.contains("no VALIDATE rows"), "{s}");
        assert_eq!(parse_validate_lines("VALIDATE|x|y|tree=a|graph=1|missing=z|stale=0|measured").len(), 0, "a malformed line is not a row");
    }

    #[test]
    fn duration_is_a_perf_line_with_a_budget() {
        assert_eq!(duration_verdict(3420, 3600).0, "pass");
        // NEGATIVE PROOF: over budget reads fail (the page folds perf fail to slow)
        let (v, s) = duration_verdict(4020, 3600);
        assert_eq!(v, "fail");
        assert!(s.contains("over the 3600s budget by 420s"), "{s}");
    }

    fn ctx_for(root: &str) -> Ctx {
        Ctx {
            root: root.to_string(), app_root: root.to_string(), home: root.to_string(),
            log: format!("{}/nightly.log", root), fail_dir: format!("{}/fails", root),
            lockdir: format!("{}/lock.d", root), role: "kade".into(), run_id: "test".into(),
            owlapi: "http://localhost:1".into(), api: "http://localhost:1".into(),
            ops_nudge: "/usr/bin/true".into(), no_nudge: true, owners: HashMap::new(),
        }
    }

    #[test]
    fn ui_leg_reads_the_seam_command_and_never_passes_on_silence() {
        let dir = std::env::temp_dir().join(format!("lanes-4278-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ctx = ctx_for(dir.to_str().unwrap());
        // a run with a failure: the leg must be red and carry the case
        std::env::set_var("NIGHTLY_PLAYWRIGHT_CMD", "printf '  \\u2718  1 [chromium] \\u203a login-journey.spec.cjs:9:3 \\u203a signs in\\n  1 failed\\n  94 passed (1.2m)\\n'; exit 1");
        let r = leg_ui(&ctx);
        assert_eq!(r.status, "fail", "{}", r.summary);
        // NEGATIVE PROOF: no summary at all is UNMEASURED, not pass
        std::env::set_var("NIGHTLY_PLAYWRIGHT_CMD", "printf 'Error: no tests found\\n'; exit 1");
        let r = leg_ui(&ctx);
        assert_eq!(r.status, "unmeasured", "{}", r.summary);
        // a clean run passes with its count
        std::env::set_var("NIGHTLY_PLAYWRIGHT_CMD", "printf '  95 passed (2.0m)\\n'; exit 0");
        let r = leg_ui(&ctx);
        assert_eq!(r.status, "pass", "{}", r.summary);
        assert!(r.summary.starts_with("95 pass, 0 fail"), "{}", r.summary);
        std::env::remove_var("NIGHTLY_PLAYWRIGHT_CMD");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bdd_leg_reads_the_seam_command() {
        let dir = std::env::temp_dir().join(format!("lanes-4278-bdd-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("platform/tests")).unwrap();
        let ctx = ctx_for(dir.to_str().unwrap());
        std::env::set_var("NIGHTLY_CUCUMBER_CMD", "printf '35 scenarios (35 passed)\\n162 steps (162 passed)\\n'");
        let r = leg_bdd(&ctx);
        assert_eq!(r.status, "pass", "{}", r.summary);
        // NEGATIVE PROOF: a planted failing scenario reds the lane
        std::env::set_var("NIGHTLY_CUCUMBER_CMD", "printf '35 scenarios (1 failed, 34 passed)\\n162 steps (1 failed, 161 passed)\\n'; exit 1");
        let r = leg_bdd(&ctx);
        assert_eq!(r.status, "fail", "{}", r.summary);
        std::env::remove_var("NIGHTLY_CUCUMBER_CMD");
        // with no seam and no cucumber-js installed under the root: UNMEASURED, named
        let r = leg_bdd(&ctx);
        assert_eq!(r.status, "unmeasured", "{}", r.summary);
        assert!(r.summary.contains("cucumber-js is not installed"), "{}", r.summary);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
