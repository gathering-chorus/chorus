//! werk-deploy — `/werk/deploy` v2 logic (card #3062).
//!
//! Self-contained: std only; calls werk-build, chorus-bin-install, launchctl,
//! codesign, gh as subprocesses. No dependency on any other chorus code
//! (ADR-032 §1 blueprint; mirrors werk-pull #3045 / werk-build #3061).
//!
//! Owns ALL system mutation. GUARANTEES a rebuild (werk-build → fresh cdhash),
//! installs to the right slot, and (for prod) kickstarts + verifies running==built.
//!
//! TWO SLOT TARGETS (the test-in-demo vs test-in-prod cases):
//! - target=werk (TEST-IN-DEMO): install to $WERK_<ROLE>_BIN, NO kickstart — the
//!   role's own session PATH-resolves it to demo the card's binary in isolation,
//!   without touching canonical or other roles.
//! - target=canonical (TEST-IN-PROD): install to $CHORUS_BIN (~/.chorus/bin),
//!   kickstart the launchd service, verify running cdhash == just-built cdhash.
//!   The shared prod deploy (acp-time).
//!
//! All-or-nothing: any failure rolls the prior binary back atomically under the lock.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}
const LOCK_EX_NB: i32 = 0x02 | 0x04;
const LOCK_UN: i32 = 0x08;

pub type R<T> = Result<T, String>;

// --- pure helpers (unit-tested) ---

pub fn branch_name(role: &str, card: u64) -> String {
    format!("{}/{}", role, card)
}

fn mint_trace() -> String {
    let ns = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}-{:x}", ns, std::process::id())
}

/// Shared trace per ADR-032 §3: CHORUS_TRACE_ID env → /tmp/<card>-trace → mint+persist.
pub fn resolve_trace(card: u64) -> String {
    if let Ok(t) = env::var("CHORUS_TRACE_ID") {
        if !t.trim().is_empty() {
            return t.trim().to_string();
        }
    }
    let p = format!("/tmp/{}-trace", card);
    if let Ok(t) = fs::read_to_string(&p) {
        if !t.trim().is_empty() {
            return t.trim().to_string();
        }
    }
    let t = mint_trace();
    let _ = fs::write(&p, &t);
    t
}

/// Parse `--target werk|canonical` from args; default canonical (prod), mirroring
/// chorus-deploy. werk = test-in-demo (role slot, no kickstart); canonical = test-in-prod.
pub fn parse_target(args: &[String]) -> R<&'static str> {
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--target" {
            return match args.get(i + 1).map(|s| s.as_str()) {
                Some("werk") => Ok("werk"),
                Some("canonical") => Ok("canonical"),
                other => Err(format!("--target must be 'werk' or 'canonical' (got {:?})", other)),
            };
        }
        i += 1;
    }
    Ok("canonical")
}

/// werk-build prints `crate=cdhash` (comma-joined) on success. Parse to pairs.
pub fn parse_build_summary(out: &str) -> Vec<(String, String)> {
    out.split([',', '\n'])
        .filter_map(|p| {
            let p = p.trim();
            let (c, h) = p.split_once('=')?;
            if c.is_empty() || h.is_empty() { None } else { Some((c.to_string(), h.to_string())) }
        })
        .collect()
}

/// Extract a cdhash from `codesign -d --verbose=4` output (`CDHash=<hash>`).
pub fn extract_running_cdhash(codesign_out: &str) -> Option<String> {
    for line in codesign_out.lines() {
        if let Some(rest) = line.trim().strip_prefix("CDHash=") {
            let h = rest.trim();
            if !h.is_empty() {
                return Some(h.to_string());
            }
        }
    }
    None
}

/// (identifier, binary) for a crate — mirrors werk-build::crate_spec (build-signed.sh map).
pub fn crate_binary(crate_name: &str) -> String {
    match crate_name {
        "chorus-hooks" => "chorus-hook-shim".to_string(),
        other => other.to_string(),
    }
}

/// launchd service name for a crate (kickstart target on prod deploy).
pub fn service_for_crate(crate_name: &str) -> String {
    format!("com.chorus.{}", crate_name.strip_prefix("chorus-").unwrap_or(crate_name))
}

// --- side-effecting helpers ---

fn jsonl(home: &Path, role: &str, card: u64, trace: &str, event: &str, extra: &str) {
    let p = home.join("ops/logs/werk-deploy.jsonl");
    if let Some(d) = p.parent() {
        let _ = fs::create_dir_all(d);
    }
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let line = format!(
        "{{\"ts\":{},\"event\":\"{}\",\"role\":\"{}\",\"card_id\":{},\"trace_id\":\"{}\"{}}}\n",
        ts, event, role, card, trace, extra
    );
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&p) {
        let _ = f.write_all(line.as_bytes());
    }
}

fn run_env(dir: Option<&str>, envs: &[(&str, &str)], cmd: &str, args: &[&str]) -> R<String> {
    let mut c = Command::new(cmd);
    c.args(args);
    if let Some(d) = dir {
        c.current_dir(d);
    }
    for (k, v) in envs {
        c.env(k, v);
    }
    let out = c.output().map_err(|e| format!("{} failed to start: {}", cmd, e))?;
    if !out.status.success() {
        return Err(format!("{} {}: {}", cmd, args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

pub struct FlockGuard(std::fs::File);
impl Drop for FlockGuard {
    fn drop(&mut self) {
        unsafe { flock(self.0.as_raw_fd(), LOCK_UN) };
    }
}

/// Lock the deploy (system mutation must serialize). Lives in the werk.
pub fn lock(werk: &Path, timeout: Duration) -> R<FlockGuard> {
    let p = werk.join(".git-deploy.lock");
    let f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&p)
        .map_err(|e| format!("cannot open lock {}: {}", p.display(), e))?;
    let start = Instant::now();
    loop {
        if unsafe { flock(f.as_raw_fd(), LOCK_EX_NB) } == 0 {
            return Ok(FlockGuard(f));
        }
        if start.elapsed() >= timeout {
            return Err("another deploy holds the werk lock (timed out)".to_string());
        }
        sleep(Duration::from_millis(100));
    }
}

fn path(p: &Path) -> R<&str> {
    p.to_str().ok_or_else(|| format!("non-utf8 path: {}", p.display()))
}

/// Entry: `werk-deploy <card> <role> [--target werk|canonical]`.
pub fn run_deploy() -> R<String> {
    let argv: Vec<String> = env::args().skip(1).collect();
    let card_arg = argv.first().ok_or_else(|| "usage: werk-deploy <card> <role> [--target werk|canonical]".to_string())?;
    let card: u64 = card_arg.parse().map_err(|_| format!("card id is not a number: {}", card_arg))?;
    let role = argv
        .get(1)
        .filter(|s| !s.starts_with("--"))
        .cloned()
        .or_else(|| env::var("DEPLOY_ROLE").ok())
        .ok_or_else(|| "usage: werk-deploy <card> <role> [--target ...] (or set DEPLOY_ROLE)".to_string())?;
    let target = parse_target(&argv)?;
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);
    let werk_base = PathBuf::from(env::var("CHORUS_WERK_BASE").map_err(|_| "CHORUS_WERK_BASE not set".to_string())?);
    deploy(card, &role, target, &home, &werk_base)
}

/// The whole verb, all inputs explicit (testable: deps injected via PATH — real or
/// shimmed werk-build / chorus-bin-install / launchctl / codesign).
pub fn deploy(card: u64, role: &str, target: &str, home: &Path, werk_base: &Path) -> R<String> {
    let trace = resolve_trace(card);
    let branch = branch_name(role, card);
    let werk = werk_base.join(format!("{}-{}", role, card));
    let werk_s = path(&werk)?.to_string();

    jsonl(home, role, card, &trace, "deploy.started", &format!(",\"target\":\"{}\"", target));

    // no-werk-refuse guard (ADR-032 §4): never deploy from canonical.
    if !werk.is_dir() {
        jsonl(home, role, card, &trace, "deploy.refused", ",\"reason\":\"no-werk\"");
        return Err(format!("no werk at {} — pull #{} first (deploy never touches canonical source)", werk.display(), card));
    }
    let cur = run_env(Some(&werk_s), &[], "git", &["-C", &werk_s, "rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
    if cur.trim() != branch {
        jsonl(home, role, card, &trace, "deploy.refused", ",\"reason\":\"branch-mismatch\"");
        return Err(format!("werk {} is on '{}', not '{}'", werk.display(), cur.trim(), branch));
    }

    // serialize all system mutation under one lock.
    let _lock = lock(&werk, Duration::from_secs(180))?;
    jsonl(home, role, card, &trace, "lock.acquired", "");

    // AC2: GUARANTEE a rebuild — werk-build compiles+signs in the werk, emits fresh cdhash.
    // CHORUS_TRACE_ID threads the chain into the child verb.
    let build_out = run_env(
        Some(&werk_s),
        &[("CHORUS_TRACE_ID", &trace), ("DEPLOY_ROLE", role), ("CHORUS_ROLE", role)],
        "werk-build",
        &[&card.to_string(), role],
    )
    .map_err(|e| format!("rebuild failed (werk-build); nothing installed: {}", e))?;
    let built = parse_build_summary(&build_out);
    if built.is_empty() {
        return Err("werk-build produced no crate=cdhash pairs — nothing to deploy".to_string());
    }
    jsonl(home, role, card, &trace, "rebuilt", &format!(",\"built\":\"{}\"", summary(&built)));

    // install + (prod) kickstart + verify, per crate, all-or-nothing.
    for (crate_name, built_cdhash) in &built {
        let bin = crate_binary(crate_name);
        let built_path = format!("{}/platform/services/{}/target/release/{}", werk_s, crate_name, bin);

        // AC2 stale-build guard (canonical only — there's a running binary to compare):
        // if the fresh rebuild produced the SAME cdhash that's already running WHILE this
        // card changed the crate's source, werk-build's build-invariance is suspect (a
        // stale/cached build that didn't pick up the change). Refuse BEFORE mutating —
        // nothing is touched. (Unchanged source legitimately yields the same cdhash =
        // an idempotent no-op, NOT a divergence, so the source-changed clause is required.)
        if target == "canonical" {
            let installed = installed_path(target, role, &bin);
            let current = run_env(None, &[], "codesign", &["-d", "--verbose=4", &installed])
                .ok()
                .and_then(|o| extract_running_cdhash(&o));
            if current.as_deref() == Some(built_cdhash.as_str()) && crate_source_changed(&werk_s, crate_name) {
                jsonl(home, role, card, &trace, "deploy.refused", ",\"reason\":\"cdhash-divergence\"");
                return Err(format!(
                    "cdhash-divergence: rebuild of {} gave the running cdhash {} but its source changed this card — stale build, refusing (werk-build invariance suspect)",
                    crate_name, built_cdhash
                ));
            }
        }

        // install to the slot. chorus-bin-install --target werk → $WERK_<ROLE>_BIN (demo),
        // --target canonical → $CHORUS_BIN (prod). CHORUS_ROLE drives WERK_<ROLE>_BIN.
        if let Err(e) = run_env(
            Some(&werk_s),
            &[("CHORUS_ROLE", role)],
            "chorus-bin-install",
            &["--target", target, &built_path, &bin],
        ) {
            rollback(home, &werk_s, role, card, &trace, target, &bin, "install-fail");
            return Err(format!("install of {} failed; rolled back: {}", bin, e));
        }
        jsonl(home, role, card, &trace, "installed", &format!(",\"crate\":\"{}\",\"target\":\"{}\"", crate_name, target));

        // TEST-IN-PROD (canonical) only: kickstart + verify running==built. TEST-IN-DEMO
        // (werk slot) skips kickstart — the role's session resolves the slot binary directly,
        // so there's no daemon to restart and no running-binary to verify yet.
        if target == "canonical" {
            let svc = service_for_crate(crate_name);
            if let Err(e) = run_env(None, &[], "launchctl", &["kickstart", "-k", &format!("gui/{}/{}", uid(), svc)]) {
                rollback(home, &werk_s, role, card, &trace, target, &bin, "kickstart-fail");
                return Err(format!("kickstart {} failed; rolled back: {}", svc, e));
            }
            // AC3: running == built. Read the installed binary's cdhash, assert it matches.
            let installed = installed_path(target, role, &bin);
            let cs = run_env(None, &[], "codesign", &["-d", "--verbose=4", &installed]).unwrap_or_default();
            match extract_running_cdhash(&cs) {
                Some(running) if &running == built_cdhash => {
                    jsonl(home, role, card, &trace, "verified", &format!(",\"crate\":\"{}\",\"cdhash\":\"{}\"", crate_name, running));
                }
                other => {
                    rollback(home, &werk_s, role, card, &trace, target, &bin, "cdhash-mismatch");
                    return Err(format!(
                        "running != built for {}: built={} running={:?} — rolled back (stale-binary guard)",
                        bin, built_cdhash, other
                    ));
                }
            }
        }
        let _ = register_gh(&werk_s, card, role, &trace, crate_name, built_cdhash, target);
    }

    let joined = summary(&built);
    jsonl(home, role, card, &trace, "deploy.completed", &format!(",\"target\":\"{}\",\"deployed\":\"{}\"", target, joined));
    Ok(format!("{} target={}", joined, target))
}

fn summary(built: &[(String, String)]) -> String {
    built.iter().map(|(c, h)| format!("{}={}", c, h)).collect::<Vec<_>>().join(",")
}

/// Did this card change the crate's source (vs origin/main)? Drives the AC2
/// stale-build guard: source-changed + rebuild-cdhash == running-cdhash => stale.
fn crate_source_changed(werk_s: &str, crate_name: &str) -> bool {
    let diff = run_env(Some(werk_s), &[], "git", &["-C", werk_s, "diff", "origin/main", "--name-only"]).unwrap_or_default();
    let prefix = format!("platform/services/{}/", crate_name);
    diff.lines().any(|l| l.trim().starts_with(&prefix))
}

fn uid() -> u32 {
    // best-effort: $UID or 501 default (the install host).
    env::var("UID").ok().and_then(|s| s.parse().ok()).unwrap_or(501)
}

fn installed_path(target: &str, role: &str, bin: &str) -> String {
    let home = env::var("HOME").unwrap_or_default();
    if target == "werk" {
        let slot = env::var(format!("WERK_{}_BIN", role.to_uppercase())).unwrap_or_default();
        format!("{}/{}", slot, bin)
    } else {
        env::var("CHORUS_BIN").map(|d| format!("{}/{}", d, bin)).unwrap_or_else(|_| format!("{}/.chorus/bin/{}", home, bin))
    }
}

/// gh commit-status chorus/deploy/<card> carrying the deployed cdhash + target (best-effort).
fn register_gh(werk_s: &str, card: u64, role: &str, trace: &str, crate_name: &str, cdhash: &str, target: &str) -> R<()> {
    let sha = run_env(Some(werk_s), &[], "git", &["-C", werk_s, "rev-parse", "HEAD"])?.trim().to_string();
    let endpoint = format!("repos/{{owner}}/{{repo}}/statuses/{}", sha);
    let desc = format!("role={} trace={} crate={} cdhash={} target={} status=deployed", role, trace, crate_name, cdhash, target);
    run_env(
        Some(werk_s),
        &[],
        "gh",
        &["api", &endpoint, "-f", "state=success", "-f", &format!("context=chorus/deploy/{}", card), "-f", &format!("description={}", desc)],
    )
    .map(|_| ())
}

/// All-or-nothing rollback (AC4): restore the prior binary atomically. Best-effort but
/// loud — a failed rollback is the worst case (logged for ops). For canonical, restore
/// via chorus-bin-install --rollback (prior binary kept by the install primitive) +
/// re-kickstart; for werk slot, the role slot is disposable so removal suffices.
// All 8 inputs are genuinely needed for an atomic restore + its witness line
// (who/what/where/why); bundling them into a struct would add indirection without
// reducing the real fan-in. The lint is the wrong call for a finalize-step helper.
#[allow(clippy::too_many_arguments)]
fn rollback(home: &Path, werk_s: &str, role: &str, card: u64, trace: &str, target: &str, bin: &str, reason: &str) {
    jsonl(home, role, card, trace, "deploy.rolledback", &format!(",\"reason\":\"{}\",\"target\":\"{}\"", reason, target));
    let _ = run_env(Some(werk_s), &[("CHORUS_ROLE", role)], "chorus-bin-install", &["--target", target, "--rollback", bin]);
    if target == "canonical" {
        let svc = service_for_crate(bin);
        let _ = run_env(None, &[], "launchctl", &["kickstart", "-k", &format!("gui/{}/{}", uid(), svc)]);
    }
}
