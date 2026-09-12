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

use werk_test::nightly_run::{
    census_row, coverage_row, denominator_row, fail_log_name, fold_unit_line, last_run_rows, load_verdict,
    notify_messages, owner_for, owner_map, parse_case_line, parse_floors, pipeline_run_body, run_summary_fields,
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

fn run_capped(mut cmd: Command, cap: Duration) -> (i32, String) {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return (127, format!("spawn failed: {}", e)),
    };
    let t0 = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(st)) => {
                let out = child.wait_with_output().map(|o| format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr))).unwrap_or_default();
                return (st.code().unwrap_or(1), out);
            }
            Ok(None) => {
                if t0.elapsed() > cap {
                    let _ = child.kill();
                    let _ = child.wait();
                    return (124, format!("SUITE TIMEOUT rc=124 after {}s", cap.as_secs()));
                }
                std::thread::sleep(Duration::from_secs(2));
            }
            Err(e) => return (1, format!("wait failed: {}", e)),
        }
    }
}

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
    let mut out = Vec::new();
    let ka = format!("\"{}\"", a);
    let kb = format!("\"{}\"", b);
    for obj in json.split("},").map(|s| s.to_string()) {
        let va = str_field(&obj, &ka);
        let vb = str_field(&obj, &kb);
        if let (Some(x), Some(y)) = (va, vb) {
            out.push((x, y));
        }
    }
    out
}

fn str_field(obj: &str, key: &str) -> Option<String> {
    let i = obj.find(key)? + key.len();
    let rest = obj[i..].trim_start().strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
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
    cases: Vec<(String, String)>,
    lane_text: String,
    rc: i32,
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
            return LaneResult { rows: vec![row], cases: Vec::new(), lane_text: String::new(), rc: 127 };
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
    let mut cases = Vec::new();
    let mut lane_text = String::new();
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
        if let Some(c) = parse_case_line(&line) {
            cases.push(c);
            continue;
        }
        if let Some((row, contradiction)) = fold_unit_line(&line, &|p| ctx.owner(p), box_over_load) {
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
    LaneResult { rows, cases, lane_text, rc }
}

// ───────────────────────── phase 3: the census, from the run's own record ─────────────────────────

fn census(ctx: &Ctx, registered: &[(String, String)], cases: &[(String, String)], lane_text: &str, ui_registered: &std::collections::BTreeSet<String>) -> (SuiteRow, Vec<String>) {
    let mut reg: Vec<(String, String)> = registered.to_vec();
    reg.sort();
    reg.dedup();
    let mut ex: Vec<(String, String)> = cases.to_vec();
    ex.sort();
    ex.dedup();
    let gap = werk_test::reconcile_gap(&reg, &ex);
    if gap.is_empty() {
        return (census_row(reg.len(), &gap, "", ""), Vec::new());
    }
    let exists = |f: &str| Path::new(&ctx.root).join(f).exists();
    let classified = werk_test::classify_gap(&gap, &ex, ui_registered, &exists);
    let by_state = werk_test::gap_state_split(&classified);
    let empty_units: Vec<&str> = lane_text.lines().filter(|l| l.starts_with("nightly-unit|") && l.contains("|fail|0 pass, 0 fail")).collect();
    let attributed = if empty_units.is_empty() {
        "no unit of this run failed empty, so these are registered names the runners never emit — the ledger does not cross-foot".to_string()
    } else {
        format!("{} unit(s) of this run failed empty", empty_units.len())
    };
    let detail: Vec<String> = werk_test::gap_state_report(&classified, &ex).lines().map(|l| format!("reconcile-detail|{}", l)).collect();
    (census_row(reg.len(), &gap, &by_state, &attributed), detail)
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
        let age = meta.modified().ok().and_then(|m| m.elapsed().ok()).map(|d| d.as_secs()).unwrap_or(0);
        if age > 93600 {
            println!("SUITE|meta|{}|silas|fail|0 pass, 1 fail (nightly log STALE — {}s old > 26h; the 03:00 run did not write)", log, age);
            return Some(Ok(1));
        }
        for l in last_run_rows(&std::fs::read_to_string(&log).unwrap_or_default()) {
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
    if !ctx.append_log(&format!("RUN|start|{}|pid={}", now_stamp(), std::process::id())) {
        eprintln!("nightly: WARNING — cannot append to {}; this run's results reach NOBODY", ctx.log);
    }
    let (owners, registered) = read_registry(ctx);
    ctx.owners = owners;
    let mut rows: Vec<SuiteRow> = Vec::new();
    let push = |ctx: &Ctx, r: SuiteRow, rows: &mut Vec<SuiteRow>| {
        ctx.append_log(&r.line());
        rows.push(r);
    };
    if std::env::var("NIGHTLY_LEGS_NOOP").is_err() {
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
    // census from the run's own record — registry read once, above
    let ui: std::collections::BTreeSet<String> = registered.iter().map(|(f, _)| f.clone()).filter(|f| f.ends_with(".spec.ts")).collect();
    if registered.is_empty() {
        let r = SuiteRow::new("reconcile", "tests-domain", "kade", "unmeasured", "0 pass, 0 fail (census could not be taken — registry unreachable)");
        push(ctx, r, &mut rows);
    } else {
        let (r, detail) = census(ctx, &registered, &lane.cases, &lane.lane_text, &ui);
        push(ctx, r, &mut rows);
        for d in detail {
            ctx.append_log(&d);
        }
    }
    ctx.append_log(&format!("RUN|complete|{}|suites={}", now_stamp(), rows.len()));
    // the tail: summary, record, per-row events, nudges, readout
    ctx.spine("nightly.run.summary", &run_summary_fields(&rows));
    emit_pipeline_run(ctx, &rows, t0.elapsed().as_millis());
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
        for (to, msg) in notify_messages(&rows, &sec_owner) {
            ctx.nudge(&to, &msg);
        }
        deliver_readout(ctx);
    }
    let _ = lane.rc;
    Ok(0)
}

fn emit_pipeline_run(ctx: &Ctx, rows: &[SuiteRow], duration_ms: u128) {
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
    let name = format!("nightly-{}", now_stamp().replace(':', "-"));
    let body = pipeline_run_body(rows, &name, &env_or("CHORUS_TRACE_ID", &format!("nightly-{}", epoch())), duration_ms);
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
