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

// #3092 — per-role demo env lifecycle (env_start / env_deploy / env_stop).
// Encapsulated in demo_env.rs so the per-service plist/marker logic has ONE
// home instead of being scattered across werk-deploy + chorus-werk + future
// verbs. Named demo_env (not env) to avoid colliding with std::env in lib.rs.
pub mod demo_env;

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

/// #3092 — what KIND of thing is this name, and what does it need at deploy time?
///
/// Three classes today, all foreseeable from the existing repo + LaunchAgent layout:
/// - **RustService:** signed Rust binary with a com.chorus.<svc> LaunchAgent
///   (chorus-hooks/chorus-inject/chorus-mcp). Install + kickstart + cdhash verify.
/// - **TsService:** TypeScript service with a com.chorus.<svc> LaunchAgent that runs
///   `node dist/server.js` (chorus-api today). Install dist + kickstart + health smoke.
/// - **CliVerb:** standalone Rust binary, no LaunchAgent — runs once per invocation
///   (werk-pull/commit/push/build/deploy/accept/demo). Install + installed-cdhash
///   verify; NO kickstart (no service to restart).
///
/// Per Jeff's framing (2026-05-26 #3092): "build all things that need a build based
/// on the branch; deploy all things that get built." The class is data not enum —
/// the rule here recognizes the existing names today; new buildable things add a
/// rule. The kickstart-or-not question reduces to "does the LaunchAgent exist?"
/// — which is exactly what `RustService` / `TsService` carry vs `CliVerb` doesn't.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetClass {
    /// Rust binary + LaunchAgent. Existing chorus-hooks/inject/mcp path.
    RustService { svc: String, bin: String },
    /// TypeScript service + LaunchAgent. New path: dist install + smoke.
    /// `dist_dir_rel` is the service's dist directory relative to repo root
    /// (e.g., "platform/api/dist" for chorus-api). `smoke_url` is the health
    /// endpoint to wait on after kickstart.
    TsService { svc: String, dist_dir_rel: String, smoke_url: String },
    /// Rust binary, no LaunchAgent. werk-* CLI verbs.
    CliVerb { bin: String },
    /// #3126 — Shared TypeScript library (chorus-sdk). No LaunchAgent, no kickstart.
    /// Deploy: copy the lib's dist to canonical, then cascade-redeploy + verify every
    /// graph-discovered consumer resolves the just-built lib identity. `lib_dist_rel`
    /// is the lib's dist dir relative to repo root (e.g. "platform/chorus-sdk/dist").
    SharedLib { name: String, lib_dist_rel: String },
}

/// #3092 — classify a name from the build summary. Returns Err for unknown
/// names so unfamiliar additions get surfaced rather than silently mis-deployed.
///
/// The known set is the union of what werk-build can produce today; expanding
/// this list and the corresponding werk-build path is the same one-card move.
pub fn target_class(name: &str) -> R<TargetClass> {
    match name {
        // Rust services (LaunchAgents): the original werk-deploy slice.
        "chorus-hooks" => Ok(TargetClass::RustService {
            svc: "com.chorus.hooks".to_string(),
            bin: "chorus-hook-shim".to_string(),
        }),
        "chorus-inject" => Ok(TargetClass::RustService {
            svc: "com.chorus.inject".to_string(),
            bin: "chorus-inject".to_string(),
        }),
        "chorus-mcp" => Ok(TargetClass::RustService {
            svc: "com.chorus.mcp".to_string(),
            bin: "chorus-mcp".to_string(),
        }),
        // TS service (LaunchAgent runs `node dist/server.js`): the #3092 net-new path.
        "chorus-api" => Ok(TargetClass::TsService {
            svc: "com.chorus.api".to_string(),
            dist_dir_rel: "platform/api/dist".to_string(),
            // Health endpoint that v1 chorus-deploy.sh's CHORUS_API_HEALTH_URL also
            // hits post-kickstart. Bypassing MCP-init smoke because chorus-api no
            // longer hosts /mcp (#2998); a 200 on /health is the served-the-new-code
            // signal, same as v1's wait_for_chorus_api_mcp_ready inner branch.
            smoke_url: "http://localhost:3340/api/chorus/health".to_string(),
        }),
        // CLI verbs (no LaunchAgent, no kickstart): the #3092 second net-new path.
        n if n.starts_with("werk-") => Ok(TargetClass::CliVerb { bin: n.to_string() }),
        // #3126 — shared library: dist→canonical + cascade-redeploy consumers.
        "chorus-sdk" => Ok(TargetClass::SharedLib {
            name: "chorus-sdk".to_string(),
            lib_dist_rel: "platform/chorus-sdk/dist".to_string(),
        }),
        other => Err(format!(
            "target_class: unknown name '{}' — add a rule (Rust service, TS service, CLI verb, or shared lib)",
            other
        )),
    }
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

    // #3092 — env-* subcommands intercept BEFORE the standard <card> <role>
    // parse. Two verbs: env-up (build + bootstrap + smoke + markers; idempotent;
    // re-running refreshes against current werk source) and env-down (teardown).
    // Builds are cheap (~2s for TS), so up is also the "deploy a change to demo"
    // path — no separate start/deploy boundary.
    if let Some(first) = argv.first() {
        match first.as_str() {
            "env-up" => return run_env_up(&argv[1..]),
            "env-down" => return run_env_down(&argv[1..]),
            _ => {}
        }
    }

    let card_arg = argv.first().ok_or_else(|| "usage: werk-deploy <card> <role> [--target werk|canonical]  |  werk-deploy env-{up,down} <role> [card]".to_string())?;
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

/// #3092 — werk-deploy env-up <role> [card]
/// Stand up the role's demo environment: build dist per service inside the
/// werk, generate plists, bootstrap launchd variants, smoke, write markers.
/// Idempotent — re-running is the "deploy a change to demo" path.
/// If `card` is given, variants run from that card's werk; else discover the
/// role's first card werk under CHORUS_WERK_BASE.
fn run_env_up(args: &[String]) -> R<String> {
    let role = parse_explicit_role(args)?;
    let werk_base = env::var("CHORUS_WERK_BASE").map_err(|_| "CHORUS_WERK_BASE not set".to_string())?;
    let home = env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?;
    let card = parse_optional_card(args);
    // #3119: witness the standalone env-up path — it was the only verb path that
    // emitted NOTHING (the deploy() TS path already wraps env_up with jsonl).
    // Writes to ops/logs/werk-deploy.jsonl, ingested by promtail job=werk-verbs.
    let card_n = card.unwrap_or(0);
    let trace = resolve_trace(card_n);
    let home_p = Path::new(&home);
    jsonl(home_p, &role, card_n, &trace, "env.up.started", "");
    let result = crate::demo_env::werk_root_for(&role, card, &werk_base)
        .and_then(|werk_root| crate::demo_env::env_up(&role, &werk_root, &home));
    match &result {
        Ok(summary) => jsonl(home_p, &role, card_n, &trace, "env.up.completed",
            &format!(",\"detail\":\"{}\"", summary.replace('\\', "/").replace('"', "'"))),
        Err(e) => jsonl(home_p, &role, card_n, &trace, "env.up.failed",
            &format!(",\"error\":\"{}\"", e.replace('\\', "/").replace('"', "'"))),
    }
    result
}

/// #3092 — werk-deploy env-down <role>
/// Tear down the role's demo environment. Idempotent. Verifies post-bootout
/// that variants actually exited (retries once on lag).
fn run_env_down(args: &[String]) -> R<String> {
    let role = parse_explicit_role(args)?;
    let home = env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?;
    // #3119: witness env-down (same dark path as env-up).
    let card_n = parse_optional_card(args).unwrap_or(0);
    let trace = resolve_trace(card_n);
    let home_p = Path::new(&home);
    jsonl(home_p, &role, card_n, &trace, "env.down.started", "");
    let result = crate::demo_env::env_down(&role, &home);
    match &result {
        Ok(summary) => jsonl(home_p, &role, card_n, &trace, "env.down.completed",
            &format!(",\"detail\":\"{}\"", summary.replace('\\', "/").replace('"', "'"))),
        Err(e) => jsonl(home_p, &role, card_n, &trace, "env.down.failed",
            &format!(",\"error\":\"{}\"", e.replace('\\', "/").replace('"', "'"))),
    }
    result
}

/// Require an explicit role arg for env-* (don't fall back to DEPLOY_ROLE)
/// so a stray invocation can't silently spin up a variant from the calling
/// session's env. Caught on the maiden voyage 2026-05-26: a no-args sanity
/// check inherited DEPLOY_ROLE=silas and accidentally fired env_start.
fn parse_explicit_role(args: &[String]) -> R<String> {
    args.iter()
        .find(|s| !s.starts_with("--") && s.parse::<u64>().is_err())
        .cloned()
        .ok_or_else(|| {
            "env-*: role required as explicit positional arg (silas/kade/wren). \
             DEPLOY_ROLE fallback intentionally disabled so a stray invocation \
             can't spin up a variant. Pass the role: `werk-deploy env-up silas`".to_string()
        })
}

fn parse_optional_card(args: &[String]) -> Option<u64> {
    args.iter().find_map(|s| s.parse::<u64>().ok())
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

    // install + (prod) kickstart + verify, per unit, all-or-nothing.
    // #3092: dispatch by target_class so each kind (Rust service / TS service / CLI verb)
    // walks its own deploy path; no class enum on the verb's top level, just per-unit
    // dispatch (Jeff's framing: deploy all things that got built).
    for (name, built_identity) in &built {
        let class = target_class(name)?;
        match &class {
            TargetClass::RustService { svc, bin } => {
                deploy_rust_service(
                    home, &werk_s, role, card, target, &trace, name, built_identity, svc, bin,
                )?;
            }
            TargetClass::TsService { svc, dist_dir_rel, smoke_url } => {
                deploy_ts_service(
                    home, &werk_s, role, card, target, &trace, name, built_identity, svc,
                    dist_dir_rel, smoke_url,
                )?;
            }
            TargetClass::CliVerb { bin } => {
                deploy_cli_verb(
                    home, &werk_s, role, card, target, &trace, name, built_identity, bin,
                )?;
            }
            TargetClass::SharedLib { name: lib, lib_dist_rel } => {
                deploy_shared_lib(
                    home, &werk_s, role, card, target, &trace, lib, built_identity, lib_dist_rel,
                )?;
            }
        }
        let _ = register_gh(&werk_s, card, role, &trace, name, built_identity, target);
    }

    let joined = summary(&built);
    jsonl(home, role, card, &trace, "deploy.completed", &format!(",\"target\":\"{}\",\"deployed\":\"{}\"", target, joined));
    Ok(format!("{} target={}", joined, target))
}

fn summary(built: &[(String, String)]) -> String {
    built.iter().map(|(c, h)| format!("{}={}", c, h)).collect::<Vec<_>>().join(",")
}

/// #3092 — deploy a Rust SERVICE crate (chorus-hooks/inject/mcp).
/// Existing pre-#3092 behavior, extracted: chorus-bin-install → kickstart →
/// codesign cdhash verify running==built. The stale-build guard (cdhash-divergence
/// refuse) fires only on canonical (where there's a running binary to compare).
#[allow(clippy::too_many_arguments)]
fn deploy_rust_service(
    home: &Path, werk_s: &str, role: &str, card: u64, target: &str, trace: &str,
    crate_name: &str, built_cdhash: &str, svc: &str, bin: &str,
) -> R<()> {
    let built_path = format!("{}/platform/services/{}/target/release/{}", werk_s, crate_name, bin);

    // AC2 stale-build guard (canonical only).
    if target == "canonical" {
        let installed = installed_path(target, role, bin);
        let current = run_env(None, &[], "codesign", &["-d", "--verbose=4", &installed])
            .ok()
            .and_then(|o| extract_running_cdhash(&o));
        if current.as_deref() == Some(built_cdhash) && crate_source_changed(werk_s, crate_name) {
            jsonl(home, role, card, trace, "deploy.refused", ",\"reason\":\"cdhash-divergence\"");
            return Err(format!(
                "cdhash-divergence: rebuild of {} gave the running cdhash {} but its source changed this card — stale build, refusing (werk-build invariance suspect)",
                crate_name, built_cdhash
            ));
        }
    }

    if let Err(e) = run_env(
        Some(werk_s),
        &[("CHORUS_ROLE", role)],
        "chorus-bin-install",
        &["--target", target, &built_path, bin],
    ) {
        rollback(home, werk_s, role, card, trace, target, bin, "install-fail");
        return Err(format!("install of {} failed; rolled back: {}", bin, e));
    }
    jsonl(home, role, card, trace, "installed", &format!(",\"name\":\"{}\",\"kind\":\"rust-service\",\"target\":\"{}\"", crate_name, target));

    if target == "canonical" {
        if let Err(e) = run_env(None, &[], "launchctl", &["kickstart", "-k", &format!("gui/{}/{}", uid(), svc)]) {
            rollback(home, werk_s, role, card, trace, target, bin, "kickstart-fail");
            return Err(format!("kickstart {} failed; rolled back: {}", svc, e));
        }
        let installed = installed_path(target, role, bin);
        let cs = run_env(None, &[], "codesign", &["-d", "--verbose=4", &installed]).unwrap_or_default();
        match extract_running_cdhash(&cs) {
            Some(running) if running == built_cdhash => {
                jsonl(home, role, card, trace, "verified", &format!(",\"name\":\"{}\",\"cdhash\":\"{}\"", crate_name, running));
            }
            other => {
                rollback(home, werk_s, role, card, trace, target, bin, "cdhash-mismatch");
                return Err(format!(
                    "running != built for {}: built={} running={:?} — rolled back (stale-binary guard)",
                    bin, built_cdhash, other
                ));
            }
        }
    }
    Ok(())
}

/// #3092 — deploy a TS SERVICE (chorus-api today).
/// Net new path: copy dist/ to canonical, preserve dist.prev for rollback,
/// kickstart com.chorus.<svc>, wait on health smoke, identity-verify installed
/// dist sha == built dist sha. Mirrors v1 chorus-deploy.sh's chorus-api branch
/// (#2831 install pattern + #2925 AC4 dist.prev preservation + #2993 ready smoke).
///
/// target=werk: build is already done; the role-slot has no separate chorus-api
/// service today (one launchd unit, shared by all roles), so install + kickstart
/// are skipped — the build itself is the "test-in-demo" surface. Logged with a
/// named reason so the no-op is intentional, not a silent skip.
#[allow(clippy::too_many_arguments)]
fn deploy_ts_service(
    home: &Path, werk_s: &str, role: &str, card: u64, target: &str, trace: &str,
    svc_name: &str, built_dist_sha: &str, svc: &str, dist_dir_rel: &str, smoke_url: &str,
) -> R<()> {
    // #3092 — target=werk: spin up a per-role werk-api LaunchAgent on a
    // role-scoped port, running from the werk's dist. This makes "test in demo"
    // REAL for chorus-api: the role's session resolves to its werk port (via
    // chorus-env-setup.sh's CHORUS_API_PORT marker check), so the role tests
    // its own code in isolation while canonical :3340 stays serving everyone
    // else. The plist/marker/bootstrap logic lives in demo_env so chorus-mcp
    // and chorus-api (and future TS services) share one substrate.
    //
    // dist_dir_rel and the per-service smoke URL are owned by demo_env::env_services,
    // not by this call site — that's the encapsulation point Jeff named.
    if target == "werk" {
        let _ = (dist_dir_rel, smoke_url, built_dist_sha); // owned by demo_env now
        let canonical_root = canonical_root_path(home);
        jsonl(home, role, card, trace, "deploy.ts.env-up", &format!(",\"name\":\"{}\"", svc_name));
        let started = crate::demo_env::env_up(role, werk_s, &canonical_root)
            .map_err(|e| format!("env_up for {} failed: {}", svc_name, e))?;
        jsonl(home, role, card, trace, "deploy.ts.env-up-done", &format!(",\"name\":\"{}\",\"detail\":\"{}\"", svc_name, started.replace('"', "'")));
        // env_up's smoke (per-service GET on the role port) IS the deploy
        // verify for target=werk. The dist IS the source for the werk variant
        // (no separate installed copy to hash); the running variant serving
        // 200 on its endpoint is the proof.
        return Ok(());
    }

    // target=canonical: dist install + kickstart + smoke + identity verify.
    let canonical_root = canonical_root_path(home);
    let canonical_dist = format!("{}/{}", canonical_root, dist_dir_rel);
    let canonical_dist_prev = format!("{}.prev", canonical_dist);
    let werk_dist = format!("{}/{}", werk_s, dist_dir_rel);

    if !Path::new(&werk_dist).is_dir() {
        return Err(format!(
            "TS service deploy: werk dist not found at {} (werk-build should have produced it)",
            werk_dist
        ));
    }

    // Preserve current canonical dist (v1 #2925 AC4 pattern). Best-effort —
    // a non-existent canonical/dist just means this is a fresh install.
    if Path::new(&canonical_dist).is_dir() {
        let _ = fs::remove_dir_all(&canonical_dist_prev);
        if let Err(e) = fs::rename(&canonical_dist, &canonical_dist_prev) {
            return Err(format!("preserve {} → {} failed: {}", canonical_dist, canonical_dist_prev, e));
        }
    }

    // Copy werk dist → canonical dist. `cp -R` matches v1's atomic intent.
    if let Err(e) = run_env(None, &[], "cp", &["-R", &werk_dist, &canonical_dist]) {
        // Restore previous dist on failed install — keep canonical consistent.
        if Path::new(&canonical_dist_prev).is_dir() {
            let _ = fs::remove_dir_all(&canonical_dist);
            let _ = fs::rename(&canonical_dist_prev, &canonical_dist);
        }
        jsonl(home, role, card, trace, "deploy.rolledback", ",\"reason\":\"ts-install-fail\"");
        return Err(format!("install of {} dist failed; restored prev: {}", svc_name, e));
    }
    jsonl(home, role, card, trace, "installed", &format!(",\"name\":\"{}\",\"kind\":\"ts-service\",\"target\":\"{}\"", svc_name, target));

    // #3092 — identity verify BEFORE kickstart (maiden-voyage bug fix).
    // The cp -R has just landed the dist bytes; the canonical dist tree is now a
    // snapshot of the built artifact. Hash it NOW, before the node process starts
    // and writes transient files (sourcemap cache, runtime state) into dist that
    // would shift the hash without changing what was installed. The "installed
    // == built" assertion is about the bytes we copied, not about what the
    // running process leaves on disk afterwards.
    // #3092 — relative-path hash (werk-build/deploy must produce the SAME hash
    // for byte-identical content even though werk + canonical sit at different
    // absolute paths). Mirrors werk-build's `build_ts_service`.
    let snapshot_cmd = format!(
        "cd {} && find . -type f | LC_ALL=C sort | xargs shasum -a 256 | shasum -a 256 | cut -d' ' -f1",
        canonical_dist
    );
    let installed_sha = run_env(None, &[], "sh", &["-c", &snapshot_cmd]).map(|o| o.trim().to_string()).unwrap_or_default();
    if installed_sha != built_dist_sha {
        // Bytes don't match what werk-build hashed — cp corrupted something or
        // the source dist mutated mid-copy. Roll back BEFORE kickstart so the
        // running process never sees the bad install.
        let _ = fs::remove_dir_all(&canonical_dist);
        if Path::new(&canonical_dist_prev).is_dir() {
            let _ = fs::rename(&canonical_dist_prev, &canonical_dist);
        }
        jsonl(home, role, card, trace, "deploy.rolledback", ",\"reason\":\"ts-identity-mismatch-pre-kickstart\"");
        return Err(format!(
            "installed dist sha != built for {} (pre-kickstart snapshot): built={} installed={} — rolled back, prev preserved",
            svc_name, built_dist_sha, installed_sha
        ));
    }
    jsonl(home, role, card, trace, "verified", &format!(",\"name\":\"{}\",\"dist_sha\":\"{}\",\"phase\":\"pre-kickstart\"", svc_name, installed_sha));

    // Kickstart the LaunchAgent. v1 uses `launchctl kickstart -k gui/<uid>/<svc>`.
    if let Err(e) = run_env(None, &[], "launchctl", &["kickstart", "-k", &format!("gui/{}/{}", uid(), svc)]) {
        // Rollback: restore prev dist + re-kickstart so the prior version stays up.
        let _ = fs::remove_dir_all(&canonical_dist);
        if Path::new(&canonical_dist_prev).is_dir() {
            let _ = fs::rename(&canonical_dist_prev, &canonical_dist);
        }
        let _ = run_env(None, &[], "launchctl", &["kickstart", "-k", &format!("gui/{}/{}", uid(), svc)]);
        jsonl(home, role, card, trace, "deploy.rolledback", ",\"reason\":\"ts-kickstart-fail\"");
        return Err(format!("kickstart {} failed; restored prev dist: {}", svc, e));
    }

    // Health smoke — wait for the new process to start serving. The endpoint comes
    // from TargetClass::TsService (chorus-api uses /api/chorus/health per v1 #2998).
    // ~30s budget mirrors v1's CHORUS_MCP_SMOKE_TIMEOUT_S.
    if let Err(e) = wait_for_health(smoke_url, Duration::from_secs(30)) {
        let _ = fs::remove_dir_all(&canonical_dist);
        if Path::new(&canonical_dist_prev).is_dir() {
            let _ = fs::rename(&canonical_dist_prev, &canonical_dist);
        }
        let _ = run_env(None, &[], "launchctl", &["kickstart", "-k", &format!("gui/{}/{}", uid(), svc)]);
        jsonl(home, role, card, trace, "deploy.rolledback", ",\"reason\":\"ts-smoke-timeout\"");
        return Err(format!("health smoke for {} failed; restored prev dist: {}", svc_name, e));
    }

    // #3092 — identity verify happens BEFORE kickstart (above). The running
    // node process writes transient files into dist (sourcemap cache, runtime
    // artifacts), which would shift a post-kickstart hash and produce a false
    // mismatch (caught on the maiden voyage of #3096 against the new substrate
    // 2026-05-26: a clean install verified-out-of-date because the kickstarted
    // process had already touched dist before the re-hash ran). Pre-kickstart
    // snapshot is the right "installed == built" assertion; post-kickstart the
    // dist tree is the running process's working set, not a build artifact.
    Ok(())
}

/// #3092 — deploy a Rust CLI VERB (werk-* binaries).
/// No LaunchAgent, no kickstart. Install to a slot, verify installed cdhash
/// matches built cdhash. The "running" notion doesn't apply — CLI verbs run
/// once per invocation; identity is just installed-cdhash matching built.
///
/// #3101 — for `target=canonical`, install at `$CHORUS_BIN/<name>-bin` and
/// write a thin wrapper at `$CHORUS_BIN/<name>` that exec's
/// `$WERK_<ROLE>_BIN/<name>` first when CHORUS_ROLE is set + the slot binary
/// exists, else falls through to the canonical `-bin`. This makes the demo-er
/// actually run their new code mid-demo when paired with a self-reexec in the
/// orchestrator after build+deploy (Wren's lane on werk-demo). For
/// `target=werk` the installed binary IS the role's slot binary; no wrapper
/// (a wrapper there would loop into itself).
#[allow(clippy::too_many_arguments)]
fn deploy_cli_verb(
    home: &Path, werk_s: &str, role: &str, card: u64, target: &str, trace: &str,
    crate_name: &str, built_cdhash: &str, bin: &str,
) -> R<()> {
    let built_path = format!("{}/platform/services/{}/target/release/{}", werk_s, crate_name, bin);

    // target=werk: install raw binary at WERK_<ROLE>_BIN/<name> (existing path).
    // target=canonical: install at CHORUS_BIN/<name>-bin via chorus-bin-install,
    // then overwrite CHORUS_BIN/<name> with the wrapper. chorus-bin-install
    // takes a bin-name and writes to <slot>/<name>; passing `<bin>-bin` makes
    // it land at the -bin suffix path.
    let install_bin_name = if target == "canonical" {
        format!("{}-bin", bin)
    } else {
        bin.to_string()
    };

    if let Err(e) = run_env(
        Some(werk_s),
        &[("CHORUS_ROLE", role)],
        "chorus-bin-install",
        &["--target", target, &built_path, &install_bin_name],
    ) {
        rollback(home, werk_s, role, card, trace, target, bin, "install-fail");
        return Err(format!("install of {} failed; rolled back: {}", bin, e));
    }
    jsonl(home, role, card, trace, "installed", &format!(",\"name\":\"{}\",\"kind\":\"cli-verb\",\"target\":\"{}\",\"as\":\"{}\"", crate_name, target, install_bin_name));

    // Identity verify against the INSTALLED binary's cdhash. No kickstart so
    // no "running" cdhash; the contract is install-then-cdhash-match.
    let installed = installed_path(target, role, &install_bin_name);
    let cs = run_env(None, &[], "codesign", &["-d", "--verbose=4", &installed]).unwrap_or_default();
    match extract_running_cdhash(&cs) {
        Some(installed_cdhash) if installed_cdhash == built_cdhash => {
            jsonl(home, role, card, trace, "verified", &format!(",\"name\":\"{}\",\"cdhash\":\"{}\"", crate_name, installed_cdhash));
        }
        other => {
            rollback(home, werk_s, role, card, trace, target, bin, "cdhash-mismatch");
            return Err(format!(
                "installed != built for {}: built={} installed={:?} — rolled back",
                bin, built_cdhash, other
            ));
        }
    }

    // #3101 — for canonical CLI-verb installs, write the role-slot-first
    // wrapper at CHORUS_BIN/<name>. The Rust binary lives at CHORUS_BIN/<name>-bin;
    // the wrapper resolves the role's slot first if present, else falls back.
    if target == "canonical" {
        let chorus_bin = env::var("CHORUS_BIN").unwrap_or_else(|_| {
            format!("{}/.chorus/bin", env::var("HOME").unwrap_or_default())
        });
        let wrapper_path = format!("{}/{}", chorus_bin, bin);
        let bin_path = format!("{}/{}-bin", chorus_bin, bin);
        let wrapper = format!(
            "#!/bin/sh\n\
# Auto-generated by werk-deploy (#3101). Routes to role-slot first.\n\
case \"${{CHORUS_ROLE:-}}\" in\n\
  silas) slot=\"${{WERK_SILAS_BIN:-}}/{name}\" ;;\n\
  wren)  slot=\"${{WERK_WREN_BIN:-}}/{name}\" ;;\n\
  kade)  slot=\"${{WERK_KADE_BIN:-}}/{name}\" ;;\n\
  *)     slot=\"\" ;;\n\
esac\n\
if [ -n \"$slot\" ] && [ -x \"$slot\" ]; then\n\
  exec \"$slot\" \"$@\"\n\
fi\n\
exec \"{bin_path}\" \"$@\"\n",
            name = bin,
            bin_path = bin_path,
        );
        if let Err(e) = fs::write(&wrapper_path, &wrapper) {
            return Err(format!("deploy_cli_verb: write wrapper {}: {}", wrapper_path, e));
        }
        // chmod +x via std (Unix permissions; mode 0o755).
        let _ = run_env(None, &[], "chmod", &["+x", &wrapper_path]);
        jsonl(home, role, card, trace, "wrapper-installed", &format!(",\"name\":\"{}\",\"wrapper\":\"{}\",\"bin\":\"{}\"", crate_name, wrapper_path, bin_path));
    }
    Ok(())
}

/// #3126 — relative-path sha256 of a dist tree (location-independent: werk dist and
/// canonical dist hash equal for byte-identical content). Mirrors werk-build's dist_sha.
fn dist_sha(dist_dir: &str) -> R<String> {
    if !Path::new(dist_dir).is_dir() {
        return Err(format!("dist not found at {}", dist_dir));
    }
    let cmd = format!(
        "cd {} && find . -type f | LC_ALL=C sort | xargs shasum -a 256 | shasum -a 256 | cut -d' ' -f1",
        dist_dir
    );
    let sha = run_env(None, &[], "sh", &["-c", &cmd]).map(|o| o.trim().to_string()).unwrap_or_default();
    if sha.is_empty() || sha.len() < 32 {
        return Err(format!("dist hash invalid ({:?}) for {}", sha, dist_dir));
    }
    Ok(sha)
}

/// #3126 — std-only `(dep_name, file_target)` for every `"x":"file:<path>"` dep.
/// Self-contained per ADR-032 §1 (verbs don't import each other); mirrors
/// werk-build::extract_file_deps so consumer discovery is identical on both sides.
fn extract_file_deps(pkg_json: &str) -> Vec<(String, String)> {
    let tokens = quoted_tokens(pkg_json);
    let mut out = Vec::new();
    for i in 0..tokens.len() {
        if let Some(target) = tokens[i].strip_prefix("file:") {
            if i > 0 && !tokens[i - 1].is_empty() && !target.is_empty() {
                out.push((tokens[i - 1].clone(), target.to_string()));
            }
        }
    }
    out
}

fn pkg_name(pkg_json: &str) -> Option<String> {
    let tokens = quoted_tokens(pkg_json);
    for i in 0..tokens.len() {
        if tokens[i] == "name" {
            if let Some(v) = tokens.get(i + 1) {
                if !v.is_empty() {
                    return Some(v.clone());
                }
            }
        }
    }
    None
}

fn quoted_tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (mut in_str, mut cur, mut prev) = (false, String::new(), '\0');
    for ch in s.chars() {
        if ch == '"' && prev != '\\' {
            if in_str {
                out.push(std::mem::take(&mut cur));
                in_str = false;
            } else {
                in_str = true;
            }
        } else if in_str {
            cur.push(ch);
        }
        prev = ch;
    }
    out
}

/// #3126 — a graph-discovered consumer: package name + its dir relative to repo root.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Consumer {
    name: String,
    dir_rel: String,
}

/// #3126 — DISCOVER consumers of `lib_dir_abs` from the dependency graph: every
/// git-tracked package.json declaring a `file:` dep that resolves to the lib dir.
/// NOT a hardcoded list (the rot #3092 left). Sorted by dir for stable ordering.
fn discover_consumers(werk_s: &str, lib_dir_abs: &Path) -> Vec<Consumer> {
    let listing = run_env(Some(werk_s), &[], "git", &["-C", werk_s, "ls-files", "*package.json", "**/package.json"]).unwrap_or_default();
    let mut found: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for rel in listing.lines() {
        let rel = rel.trim();
        if rel.is_empty() {
            continue;
        }
        let abs = format!("{}/{}", werk_s, rel);
        let Ok(content) = fs::read_to_string(&abs) else { continue };
        let pkg_dir = match Path::new(&abs).parent() {
            Some(d) => d.to_path_buf(),
            None => continue,
        };
        for (_dep, target) in extract_file_deps(&content) {
            if let Ok(rc) = fs::canonicalize(pkg_dir.join(&target)) {
                if rc == lib_dir_abs {
                    let dir_rel = Path::new(rel).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                    let name = pkg_name(&content).unwrap_or_else(|| dir_rel.clone());
                    if !name.is_empty() && !dir_rel.is_empty() {
                        found.insert(dir_rel, name);
                    }
                }
            }
        }
    }
    found.into_iter().map(|(dir_rel, name)| Consumer { name, dir_rel }).collect()
}

/// #3126 — deploy a SHARED LIBRARY (chorus-sdk). The fix for the silent-stale-prod
/// class #3092 left open. Steps, all-or-nothing under the deploy lock:
///  1. Install the lib's dist to canonical (preserve .prev), identity-verify
///     installed == built (the merged lib identity H).
///  2. DISCOVER consumers from the dependency graph (never hardcoded) and install
///     each consumer's freshly-rebuilt dist to canonical (preserve .prev).
///  3. AC4 — anti-stale verify: for each consumer, hash the chorus-sdk it RESOLVES
///     at runtime (canonical <consumer>/node_modules/chorus-sdk/dist, following the
///     symlink) and assert it equals H. This is the real "a shared-lib change can no
///     longer merge green while prod runs stale" — a deployed consumer that doesn't
///     resolve the merged identity fails the deploy and rolls everything back.
///
/// No kickstart: a shared lib has no LaunchAgent; consumers (cards CLI) run
/// per-invocation. target=werk is a no-op (the werk dist IS what the role resolves).
#[allow(clippy::too_many_arguments)]
fn deploy_shared_lib(
    home: &Path, werk_s: &str, role: &str, card: u64, target: &str, trace: &str,
    lib: &str, built_identity: &str, lib_dist_rel: &str,
) -> R<()> {
    if target == "werk" {
        // The role's session resolves chorus-sdk through the werk's own symlink to
        // the werk's dist (already built by werk-build) — no canonical mutation.
        jsonl(home, role, card, trace, "deploy.sharedlib.werk-noop", &format!(",\"lib\":\"{}\"", lib));
        return Ok(());
    }

    let canonical_root = canonical_root_path(home);
    let werk_lib_dist = format!("{}/{}", werk_s, lib_dist_rel);
    if !Path::new(&werk_lib_dist).is_dir() {
        return Err(format!("shared-lib deploy: werk dist not found at {} (werk-build should have produced it)", werk_lib_dist));
    }
    let lib_dir_abs = fs::canonicalize(format!("{}/{}", werk_s, lib_dist_rel).trim_end_matches("/dist"))
        .map_err(|e| format!("cannot canonicalize werk lib dir: {}", e))?;

    // Rollback stack: (canonical_path_we_replaced, its_.prev_backup). Restored in
    // reverse on any failure so a partial cascade never leaves prod half-deployed.
    let mut moved: Vec<(String, String)> = Vec::new();
    let restore = |moved: &[(String, String)]| {
        for (canonical, prev) in moved.iter().rev() {
            let _ = fs::remove_dir_all(canonical);
            if Path::new(prev).is_dir() {
                let _ = fs::rename(prev, canonical);
            }
        }
    };

    // 1. Install lib dist → canonical (preserve .prev).
    let canonical_lib_dist = format!("{}/{}", canonical_root, lib_dist_rel);
    if let Err(e) = install_dist(&werk_lib_dist, &canonical_lib_dist, &mut moved) {
        restore(&moved);
        jsonl(home, role, card, trace, "deploy.rolledback", ",\"reason\":\"sharedlib-install-fail\"");
        return Err(format!("install of {} dist failed; rolled back: {}", lib, e));
    }
    // identity verify: installed lib dist == built H.
    let installed_lib_sha = dist_sha(&canonical_lib_dist).unwrap_or_default();
    if installed_lib_sha != built_identity {
        restore(&moved);
        jsonl(home, role, card, trace, "deploy.rolledback", ",\"reason\":\"sharedlib-identity-mismatch\"");
        return Err(format!("installed {} dist sha != built: built={} installed={} — rolled back", lib, built_identity, installed_lib_sha));
    }
    jsonl(home, role, card, trace, "installed", &format!(",\"name\":\"{}\",\"kind\":\"shared-lib\",\"target\":\"{}\",\"identity\":\"{}\"", lib, target, installed_lib_sha));

    // 2. Discover consumers from the graph + install each rebuilt dist → canonical.
    let consumers = discover_consumers(werk_s, &lib_dir_abs);
    jsonl(home, role, card, trace, "sharedlib.consumers.discovered",
        &format!(",\"lib\":\"{}\",\"count\":{},\"consumers\":\"{}\"", lib, consumers.len(),
            consumers.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join("|")));
    for c in &consumers {
        let werk_consumer_dist = format!("{}/{}/dist", werk_s, c.dir_rel);
        let canonical_consumer_dist = format!("{}/{}/dist", canonical_root, c.dir_rel);
        if !Path::new(&werk_consumer_dist).is_dir() {
            restore(&moved);
            jsonl(home, role, card, trace, "deploy.rolledback", ",\"reason\":\"consumer-dist-missing\"");
            return Err(format!("consumer {} werk dist not found at {} (werk-build cascade should have produced it)", c.name, werk_consumer_dist));
        }
        if let Err(e) = install_dist(&werk_consumer_dist, &canonical_consumer_dist, &mut moved) {
            restore(&moved);
            jsonl(home, role, card, trace, "deploy.rolledback", ",\"reason\":\"consumer-install-fail\"");
            return Err(format!("install of consumer {} dist failed; rolled back: {}", c.name, e));
        }
        jsonl(home, role, card, trace, "installed", &format!(",\"name\":\"{}\",\"kind\":\"consumer\",\"target\":\"{}\"", c.name, target));
    }

    // 3. AC4 — anti-stale verify: each consumer must RESOLVE the merged lib identity.
    for c in &consumers {
        let resolved_sdk_dist = format!("{}/{}/node_modules/{}/dist", canonical_root, c.dir_rel, lib);
        if !Path::new(&resolved_sdk_dist).exists() {
            restore(&moved);
            jsonl(home, role, card, trace, "deploy.rolledback", ",\"reason\":\"consumer-cannot-resolve-lib\"");
            return Err(format!(
                "anti-stale: consumer {} cannot resolve {} (no {}); it would run a missing/old lib — rolled back",
                c.name, lib, resolved_sdk_dist
            ));
        }
        let resolved_sha = dist_sha(&resolved_sdk_dist).unwrap_or_default();
        if resolved_sha != built_identity {
            restore(&moved);
            jsonl(home, role, card, trace, "deploy.rolledback", ",\"reason\":\"consumer-resolves-stale-lib\"");
            return Err(format!(
                "anti-stale: consumer {} resolves {} dist sha {} != merged {} — prod would run STALE, rolled back (#3126)",
                c.name, lib, resolved_sha, built_identity
            ));
        }
        jsonl(home, role, card, trace, "verified", &format!(",\"name\":\"{}\",\"kind\":\"consumer-resolves\",\"lib\":\"{}\",\"identity\":\"{}\"", c.name, lib, resolved_sha));
    }
    Ok(())
}

/// #3126 — copy a werk dist dir to canonical, preserving the prior canonical dist as
/// `<dist>.prev` and recording the move on the rollback stack. cp -R matches the
/// TS-service install's atomic intent.
fn install_dist(werk_dist: &str, canonical_dist: &str, moved: &mut Vec<(String, String)>) -> R<()> {
    let prev = format!("{}.prev", canonical_dist);
    if Path::new(canonical_dist).is_dir() {
        let _ = fs::remove_dir_all(&prev);
        fs::rename(canonical_dist, &prev).map_err(|e| format!("preserve {} → {}: {}", canonical_dist, prev, e))?;
        moved.push((canonical_dist.to_string(), prev.clone()));
    } else {
        // fresh install — record with an empty prev so rollback just removes it.
        if let Some(parent) = Path::new(canonical_dist).parent() {
            let _ = fs::create_dir_all(parent);
        }
        moved.push((canonical_dist.to_string(), prev.clone()));
    }
    run_env(None, &[], "cp", &["-R", werk_dist, canonical_dist]).map(|_| ())
}

/// #3092 — canonical repo root path (where TS service dist gets installed).
/// CHORUS_HOME is the canonical anchor for role sessions; werk_base lives next
/// to it. The TS dist install needs to write THERE, not into the werk.
fn canonical_root_path(home: &Path) -> String {
    env::var("CHORUS_HOME").unwrap_or_else(|_| home.to_string_lossy().to_string())
}

/// #3092 — poll a health URL until 200 or timeout. v1 chorus-deploy uses a
/// shell `curl` loop; we mirror the simplest form (curl -s -f) so a 5xx or
/// connection-refused both fail-fast within the budget.
fn wait_for_health(url: &str, timeout: Duration) -> R<()> {
    let start = Instant::now();
    loop {
        let out = Command::new("curl")
            .args(["-s", "-f", "-o", "/dev/null", "-w", "%{http_code}", "--max-time", "5", url])
            .output();
        if let Ok(o) = out {
            if o.status.success() {
                let code = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if code == "200" {
                    return Ok(());
                }
            }
        }
        if start.elapsed() >= timeout {
            return Err(format!("health smoke {} timed out after {:?}", url, timeout));
        }
        sleep(Duration::from_millis(500));
    }
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
