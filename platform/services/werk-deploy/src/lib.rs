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

// #3513 — the ONE shared failure classifier (failure_class / fail_extra).
include!("../../shared/failure_class.rs");

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
/// #3517 — parse the optional `--landedCommit <sha>` flag. None = flag ABSENT (a manual / --atomic
/// recovery deploy not via the pipeline → ungated, skip the one-sha gate). Some(value) = flag PRESENT
/// (the pipeline passed it; value may be "" if the werk.yml capture failed → Some("") → RED, never
/// silent-pass). Mirrors parse_target's argv scan exactly — no new path, no competing impl.
pub fn parse_landed_commit(args: &[String]) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--landedCommit" {
            return Some(args.get(i + 1).cloned().unwrap_or_default());
        }
        i += 1;
    }
    None
}

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

/// #3316 — the demo-phase cdhashes for a card, read from the werk-deploy jsonl witness.
/// The demo (target=werk) writes a `"rebuilt"` event with `"built":"crate=cdhash,..."`.
/// We take the LAST such event for the card (most recent demo build). Best-effort:
/// no demo recorded → empty → no comparison, never a false divergence.
pub fn demo_cdhashes(jsonl_content: &str, card: u64) -> Vec<(String, String)> {
    let key = format!("\"card_id\":{}", card);
    jsonl_content
        .lines()
        .filter(|l| l.contains(&key) && l.contains("\"event\":\"rebuilt\"") && l.contains("\"built\":\""))
        .next_back()
        .and_then(|l| {
            let start = l.find("\"built\":\"")? + "\"built\":\"".len();
            let rest = &l[start..];
            let end = rest.find('"')?;
            Some(parse_build_summary(&rest[..end]))
        })
        .unwrap_or_default()
}

/// #3316 — compare demo'd cdhashes against prod-built cdhashes. A divergence is a crate the
/// demo recorded whose prod-built hash differs — "what you demo'd is NOT what shipped" (the
/// integration trap: source moved between demo and land). Because build is a pure function of
/// source, an identical-source rebuild MUST match; divergence means the source changed.
/// Crates with no demo baseline are skipped (nothing to compare → no false divergence).
/// Returns (crate, demo_hash, prod_hash) per divergence.
pub fn cdhash_divergences(
    demo: &[(String, String)],
    prod: &[(String, String)],
) -> Vec<(String, String, String)> {
    prod.iter()
        .filter_map(|(c, ph)| {
            demo.iter()
                .find(|(dc, _)| dc == c)
                .filter(|(_, dh)| dh != ph)
                .map(|(_, dh)| (c.clone(), dh.clone(), ph.clone()))
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

/// #3317 — every binary a crate emits, discovered STRUCTURALLY (`crate_binaries_in`),
/// each flagged `is_service` (true = the binary the crate's launchd service runs;
/// that's the one whose cdhash gates the deploy). The service binary is the one named
/// like the crate (cargo's package binary — chorus-hooks' daemon, not its shim);
/// when no binary carries the crate name, the first discovered one is the service.
/// Replaces the hardcoded per-crate map (#3179's fix, which itself drifted the moment
/// a crate gained a binary nobody added by hand — the #3250/#3313 allowlist class).
pub fn crate_binaries_with_service_in(crate_dir: &Path, crate_name: &str) -> Vec<(String, bool)> {
    let bins = crate_binaries_in(crate_dir);
    let svc_name = if bins.iter().any(|b| b == crate_name) {
        crate_name.to_string()
    } else {
        bins.first().cloned().unwrap_or_else(|| crate_name.to_string())
    };
    bins.into_iter().map(|b| { let is_svc = b == svc_name; (b, is_svc) }).collect()
}

/// #3317 — STRUCTURAL binary discovery (port of bash chorus-deploy's crate_binaries; the
/// #3179/#3250 union rule). Every binary a crate emits = the UNION of cargo's two default
/// rules: explicit `[[bin]] name = ...` entries ∪ `src/bin/*.rs` autobins, with the package
/// name as the single-binary fallback (when no [[bin]], or when src/main.rs exists alongside
/// autobins). NO hardcoded list — a crate that gains a binary is discovered structurally,
/// closing the allowlist class (#3132/#3313). Dedup, first-seen order; no Cargo.toml → the
/// dir basename. Supersedes the hardcoded crate_binaries(&str) once callers pass the dir.
pub fn crate_binaries_in(crate_dir: &Path) -> Vec<String> {
    let toml = match fs::read_to_string(crate_dir.join("Cargo.toml")) {
        Ok(c) => c,
        Err(_) => {
            return crate_dir
                .file_name()
                .and_then(|s| s.to_str())
                .map(|s| vec![s.to_string()])
                .unwrap_or_default();
        }
    };
    let explicit = bin_names_in_toml(&toml);
    let pkg = package_name_in_toml(&toml);
    let mut autobins: Vec<String> = Vec::new();
    if let Ok(rd) = fs::read_dir(crate_dir.join("src").join("bin")) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("rs") {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    autobins.push(stem.to_string());
                }
            }
        }
        autobins.sort(); // read_dir is unordered — make discovery deterministic
    }
    let mut out: Vec<String> = Vec::new();
    if !explicit.is_empty() {
        out.extend(explicit);
    } else if autobins.is_empty() {
        // Single-binary fallback needs an actual binary root. A LIB-ONLY crate
        // (src/lib.rs, no main.rs, no [[bin]], no src/bin — e.g. werk-teardown,
        // #3431) emits no binaries; returning the package name here would send
        // deploy hunting for a target/release binary that cannot exist.
        if crate_dir.join("src").join("main.rs").is_file() {
            out.extend(pkg.clone());
        }
    } else if crate_dir.join("src").join("main.rs").is_file() {
        out.extend(pkg.clone());
    }
    out.extend(autobins);
    let mut seen = std::collections::HashSet::new();
    out.into_iter().filter(|b| !b.is_empty() && seen.insert(b.clone())).collect()
}

/// `[[bin]] name = "..."` entries from a Cargo.toml (mirrors chorus-deploy's awk: inside a
/// [[bin]] table until the next [ header, capture name = "...").
fn bin_names_in_toml(toml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_bin = false;
    for line in toml.lines() {
        let t = line.trim_start();
        if t.starts_with("[[bin]]") {
            in_bin = true;
            continue;
        }
        if t.starts_with('[') {
            in_bin = false;
        }
        if in_bin {
            if let Some(name) = toml_name_value(t) {
                out.push(name);
            }
        }
    }
    out
}

/// `[package] name = "..."` from a Cargo.toml.
fn package_name_in_toml(toml: &str) -> Option<String> {
    let mut in_pkg = false;
    for line in toml.lines() {
        let t = line.trim_start();
        if t.starts_with("[package]") {
            in_pkg = true;
            continue;
        }
        if t.starts_with('[') {
            in_pkg = false;
        }
        if in_pkg {
            if let Some(name) = toml_name_value(t) {
                return Some(name);
            }
        }
    }
    None
}

/// Parse a `name = "value"` line → the value between the first pair of quotes.
fn toml_name_value(line: &str) -> Option<String> {
    let t = line.trim_start();
    if t.split('=').next()?.trim() != "name" {
        return None;
    }
    let after = t.split_once('=')?.1;
    let start = after.find('"')? + 1;
    let rest = &after[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// launchd service name for a crate (kickstart target on prod deploy).
pub fn service_for_crate(crate_name: &str) -> String {
    format!("com.chorus.{}", crate_name.strip_prefix("chorus-").unwrap_or(crate_name))
}

/// What KIND of thing is this unit, and what does it need at deploy time?
///
/// #3132 — these classes are now DERIVED FROM STRUCTURE (`target_class` below scans
/// the werk), not from a hardcoded name allowlist. The kickstart-or-not question
/// reduces to "is there a committed LaunchAgent plist referencing this unit?" — a
/// fact read from the repo, not a list a human keeps in sync.
/// - **RustService:** signed Rust crate with a committed `com.chorus.<svc>` plist.
/// - **TsService:** TS package whose dist a committed plist runs (`node dist/...`).
/// - **CliVerb:** standalone Rust binary, no plist — runs once per invocation.
/// - **SharedLib:** TS package that is the target of a `file:` dep (cascade consumers).
/// - **TsPackage:** TS package with a build script but no plist and no consumers
///   (a CLI like `cards`/`clearing`) — copy dist to canonical, no restart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetClass {
    /// Rust binary + LaunchAgent.
    RustService { svc: String, bin: String },
    /// TypeScript service + LaunchAgent: dist install + kickstart + (declared) smoke.
    /// `dist_dir_rel` is the dist dir relative to repo root. `smoke_url` is the
    /// self-declared health endpoint (empty = fall back to launchd liveness, never
    /// gate the deploy on a missing declaration — #3132 Kade constraint).
    TsService { svc: String, dist_dir_rel: String, smoke_url: String },
    /// Rust binary, no LaunchAgent. werk-* CLI verbs.
    CliVerb { bin: String },
    /// #3126 — shared TS library: dist→canonical + cascade-redeploy graph consumers.
    SharedLib { name: String, lib_dist_rel: String },
    /// #3132 — TS package, build script, no plist, not a lib: copy dist, no restart.
    TsPackage { name: String, dist_dir_rel: String },
}

/// #3132 — std-only: collect paths named `filename` under `root`, skipping
/// node_modules/.git/target. Mirrors werk-build's helper (verbs don't import each
/// other, ADR-032 §1). Sorted for deterministic results.
fn find_files(root: &Path, filename: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let nm = e.file_name();
            let nm = nm.to_string_lossy();
            if p.is_dir() {
                if nm == "node_modules" || nm == ".git" || nm == "target" {
                    continue;
                }
                stack.push(p);
            } else if nm == filename {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// #3132 — does this package.json declare a `build` script? (mirror of werk-build's.)
fn has_build_script(pkg_json: &str) -> bool {
    let tokens = quoted_tokens(pkg_json);
    let Some(at) = tokens.iter().position(|t| t == "scripts") else { return false };
    tokens[at + 1..].iter().any(|t| t == "build")
}

/// #3132 — self-declared health URL: package.json `"chorus":{"health":"<url>"}` or
/// Cargo.toml `chorus_health = "<url>"` under `[package.metadata.chorus]`. The token
/// scan keys on a `health` token followed by its value (JSON) — shape-tolerant,
/// std-only. Absent = empty string = liveness floor (never gates the deploy).
fn declared_health(manifest: &str) -> String {
    // JSON: ... "chorus" ... "health" "<url>" ...
    let tokens = quoted_tokens(manifest);
    if let Some(i) = tokens.iter().position(|t| t == "health") {
        if let Some(v) = tokens.get(i + 1) {
            if v.starts_with("http") {
                return v.clone();
            }
        }
    }
    // TOML: chorus_health = "<url>"  (quoted_tokens grabs "<url>" as the only http token
    // following a line we can't see; fall back to first http(s) quoted token).
    for t in &tokens {
        if t.starts_with("http") {
            return t.clone();
        }
    }
    String::new()
}

/// #3132 — the committed LaunchAgent label whose plist references `dir_rel`, or None.
/// Scans committed `*.plist` (find_files) for one whose body contains the unit's
/// directory path (`node dist/...` or WorkingDirectory) and is NOT a per-role `.werk.`
/// variant. The label is the plist's filename stem (`com.gathering.messaging`).
/// This replaces the hardcoded svc-name map: "is it a service?" is now read from the
/// repo's own plists, including a brand-new committed-but-not-yet-loaded one (#3132
/// Kade constraint #3 — never derive targets from loaded agents alone).
fn service_label_for_dir(werk_root: &Path, dir_rel: &str) -> Option<String> {
    let needle = format!("{}/", dir_rel.trim_end_matches('/'));
    let mut best: Option<String> = None;
    for pl in find_plists(werk_root) {
        let stem = pl.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if stem.is_empty() || stem.contains(".werk.") {
            continue;
        }
        let Ok(body) = fs::read_to_string(&pl) else { continue };
        if body.contains(&needle) || body.contains(dir_rel.trim_end_matches('/')) {
            // Prefer a label that isn't a probe/herald sidecar; first wins otherwise.
            best.get_or_insert_with(|| stem.to_string());
        }
    }
    best
}

/// #3132 — committed `*.plist` files under the werk (any depth), excl node_modules.
fn find_plists(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let nm = e.file_name();
            let nm = nm.to_string_lossy();
            if p.is_dir() {
                if nm == "node_modules" || nm == ".git" || nm == "target" {
                    continue;
                }
                stack.push(p);
            } else if nm.ends_with(".plist") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// #3132 — locate a built unit (by its build-summary name) in the werk and classify
/// it STRUCTURALLY. The name is a Rust crate dir under platform/services/, or the
/// `"name"` of a TS package.json with a build script. Returns Err only if the name
/// can't be located at all (a real contract break between build and deploy), never
/// for "not on a list".
pub fn target_class(name: &str) -> R<TargetClass> {
    // Backwards-compatible entry: resolve the werk root from env, then derive.
    let werk_root = env::var("CHORUS_DEPLOY_WERK_ROOT")
        .map(PathBuf::from)
        .map_err(|_| "target_class: CHORUS_DEPLOY_WERK_ROOT not set (use target_class_in)".to_string())?;
    target_class_in(name, &werk_root)
}

/// #3132 — structural classification rooted at an explicit werk dir (testable).
pub fn target_class_in(name: &str, werk_root: &Path) -> R<TargetClass> {
    // werk-* CLI verbs short-circuit: standalone Rust binaries, no plist.
    if name.starts_with("werk-") {
        return Ok(TargetClass::CliVerb { bin: name.to_string() });
    }

    // Rust crate? platform/services/<name>/Cargo.toml.
    let crate_dir = werk_root.join(format!("platform/services/{}", name));
    if crate_dir.join("Cargo.toml").is_file() {
        // #3317 — the service binary comes from structural discovery (the binary named
        // like the crate = cargo's package binary), not a hardcoded crate→bin map.
        let bin = crate_binaries_with_service_in(&crate_dir, name)
            .into_iter()
            .find(|(_, is_service)| *is_service)
            .map(|(b, _)| b)
            .unwrap_or_else(|| name.to_string());
        let svc = service_for_crate(name);
        // RustService iff a committed plist with that label exists in the repo.
        let has_plist = find_plists(werk_root).iter().any(|p| {
            p.file_stem().and_then(|s| s.to_str()).map(|st| st == svc).unwrap_or(false)
        });
        return if has_plist {
            Ok(TargetClass::RustService { svc, bin })
        } else {
            Ok(TargetClass::CliVerb { bin })
        };
    }

    // TS package: find the package.json whose "name" matches.
    let pkg_files = find_files(werk_root, "package.json");
    // Shared-lib dirs = canonicalized `file:` dep targets across all packages.
    let mut sharedlib_dirs: std::collections::BTreeSet<PathBuf> = std::collections::BTreeSet::new();
    for pj in &pkg_files {
        let Ok(content) = fs::read_to_string(pj) else { continue };
        let pdir = pj.parent().unwrap_or(werk_root);
        for (_d, target) in extract_file_deps(&content) {
            if let Ok(c) = fs::canonicalize(pdir.join(&target)) {
                sharedlib_dirs.insert(c);
            }
        }
    }
    for pj in &pkg_files {
        let Ok(content) = fs::read_to_string(pj) else { continue };
        if pkg_name(&content).as_deref() != Some(name) || !has_build_script(&content) {
            continue;
        }
        let pdir = pj.parent().unwrap_or(werk_root);
        let dir_rel = pdir.strip_prefix(werk_root).unwrap_or(pdir).to_string_lossy().to_string();
        let dist_dir_rel = format!("{}/dist", dir_rel);
        let is_lib = fs::canonicalize(pdir).map(|c| sharedlib_dirs.contains(&c)).unwrap_or(false);
        if is_lib {
            return Ok(TargetClass::SharedLib { name: name.to_string(), lib_dist_rel: dist_dir_rel });
        }
        if let Some(svc) = service_label_for_dir(werk_root, &dir_rel) {
            return Ok(TargetClass::TsService {
                svc,
                dist_dir_rel,
                smoke_url: declared_health(&content),
            });
        }
        return Ok(TargetClass::TsPackage { name: name.to_string(), dist_dir_rel });
    }

    Err(format!(
        "target_class: '{}' is in the build summary but no matching crate or build-script package was found in the werk — build/deploy contract break",
        name
    ))
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

/// #3167 — witness `deploy.failed{reason}` on a TRUE terminal Err: a deploy that DIED
/// before it could roll back (werk-build subprocess fail, empty build summary, lock
/// timeout). Distinct from `deploy.rolled_back` (caught + reverted) and `deploy.refused`
/// (guard-rejected) — both of which already emit. Same witness channel
/// (ops/logs/werk-deploy.jsonl) + card_id/trace the other events use; the rollup counts
/// `.failed`. Returns the message so call sites read `return Err(died(..))` /
/// `.map_err(|e| died(.., e))?` with no behavior change beyond the added witness line.
/// disposition=died distinguishes it from reverted at a glance.
fn died(home: &Path, role: &str, card: u64, trace: &str, reason: &str, msg: String) -> String {
    jsonl(home, role, card, trace, "deploy.failed",
        &format!("{},\"disposition\":\"died\"", fail_extra(reason)));
    msg
}

/// #3215 — build the positional arg vector for a structured spine event.
/// Pure (no IO) so the event shape is unit-testable. Mirrors the
/// werk-push/werk-build/werk-pull `spine_args` convention (ADR-033: copy the
/// common convention into each verb crate) — event, role, card=, trace=, then
/// k=v extras. Shared across this crate (lib.rs handlers + demo_env per-service).
pub fn spine_args(event: &str, role: &str, card: u64, trace: &str, extras: &[(&str, &str)]) -> Vec<String> {
    let mut v = vec![
        event.to_string(),
        role.to_string(),
        format!("card={}", card),
        format!("trace={}", trace),
    ];
    for (k, val) in extras {
        v.push(format!("{}={}", k, val));
    }
    v
}

/// #3215 — emit a STRUCTURED event onto the spine (chorus-log → spine event
/// store), not just the jsonl witness. The demo-env lifecycle was dark to
/// Borg's trace-reader because env.up/env.down were jsonl-witness-only (#3119)
/// and never reached the spine — so a teardown that never fired was invisible.
/// Best-effort: a missing/failing chorus-log never affects the deploy, same as
/// the werk-push emit_spine. The jsonl witness stays (its own Loki consumer,
/// job=werk-verbs); this adds the spine plane Borg reads.
pub fn emit_spine(home: &Path, event: &str, role: &str, card: u64, trace: &str, extras: &[(&str, &str)]) {
    let log = home.join("platform/scripts/chorus-log");
    let log_s = match log.to_str() {
        Some(s) => s,
        None => return,
    };
    let args = spine_args(event, role, card, trace, extras);
    let mut c = Command::new("bash");
    c.arg(log_s);
    for a in &args {
        c.arg(a);
    }
    let _ = c.output(); // best-effort; a missing/failing chorus-log never affects the deploy
}

/// #3192 — resolve `chorus-bin-install` to an ABSOLUTE path; never rely on PATH.
/// werk-deploy runs under the werk PATH (`~/.chorus/bin` + role bin-slots), which does
/// NOT include `platform/scripts/`, so spawning the helper by bare name ENOENTs and
/// breaks every canonical deploy (the same bare-PATH class as #3151). Resolution order:
/// explicit `CHORUS_BIN_INSTALL` override → canonical (`home`)/platform/scripts →
/// werk/platform/scripts → bare name. Canonical is tried before the werk so a card's own
/// branch cannot redefine how installs happen; the bare-name tail lets PATH-shimmed tests
/// resolve their shim when no real `platform/scripts` is present.
pub fn chorus_bin_install_cmd(home: &Path, werk_s: &str) -> String {
    if let Ok(p) = env::var("CHORUS_BIN_INSTALL") {
        if !p.is_empty() {
            return p;
        }
    }
    for root in [home.to_string_lossy().to_string(), werk_s.to_string()] {
        let cand = format!("{}/platform/scripts/chorus-bin-install", root);
        if Path::new(&cand).exists() {
            return cand;
        }
    }
    "chorus-bin-install".to_string()
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

/// #3315 — ADR-037 approval gate for deploy (the OTHER irreversible verb; mirrors
/// werk-merge's #3297 require_approval). ONE gate, two doors — adapted for deploy's
/// --target axis:
///   • in-flow (no --atomic): the werk-land GO already authorized; the land sets
///     $ACCEPTER → record, don't gate. Returns the accepter, or "flow". NEVER blocks —
///     this is the load-bearing safety (no double-gate deadlock of the pipeline).
///   • standalone `--atomic --target canonical` (mutates PROD): DEMANDS an accepter or
///     refuses — the standalone door must never be a quiet unauthorized prod ship.
///   • `--atomic --target werk` (the local demo slot, reversible): no gate.
/// Returns who authorized (for the {who, what, when} deploy.approved spine event).
pub fn require_approval(atomic: bool, target: &str, accepter: Option<String>) -> R<String> {
    match accepter {
        Some(a) if !a.trim().is_empty() => Ok(a),
        _ if atomic && target == "canonical" => Err(
            "no-approval: deploy --atomic --target canonical mutates prod — set ACCEPTER=<who> to authorize this standalone prod deploy"
                .to_string(),
        ),
        _ => Ok("flow".to_string()),
    }
}

/// Entry: `werk-deploy <card> <role> [--target werk|canonical] [--atomic]`.
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
            // #3317 — crate-scoped canonical deploy, the standalone surface that
            // replaced bash `chorus-deploy <crate> [--rollback]`.
            "crate" => return run_crate_mode(&argv[1..]),
            _ => {}
        }
    }

    let card_arg = argv.first().ok_or_else(|| "usage: werk-deploy <card> <role> [--target werk|canonical]  |  werk-deploy env-{up,down} <role> [card]  |  werk-deploy crate <name> [--rollback]".to_string())?;
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
    // #3315 — ADR-037 approval gate up front. --atomic standalone → canonical DEMANDS
    // $ACCEPTER or refuses; in-flow (no --atomic) records the land's GO as "flow" and
    // never blocks. Record {who, what, when} on prod (canonical) deploys.
    let atomic = argv.iter().any(|a| a == "--atomic");
    let accepter = env::var("ACCEPTER").ok().filter(|s| !s.trim().is_empty());
    let approver = require_approval(atomic, target, accepter)?;
    if target == "canonical" {
        let trace = resolve_trace(card);
        emit_spine(
            &home,
            "deploy.approved",
            &role,
            card,
            &trace,
            &[("approver", approver.as_str()), ("atomic", if atomic { "true" } else { "false" })],
        );
    }
    // #3517 — thread the pipeline's landedCommit (--landedCommit) so deploy_canonical's verify
    // gates deployed-commit == landedCommit. Absent (manual/--atomic recovery) → None → ungated.
    let landed = parse_landed_commit(&argv);
    deploy_with_landed(card, &role, target, &home, &werk_base, landed.as_deref())
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
    emit_spine(home_p, "env.up.started", &role, card_n, &trace, &[]);
    let result = crate::demo_env::werk_root_for(&role, card, &werk_base)
        .and_then(|werk_root| crate::demo_env::env_up(&role, &werk_root, &home, card_n, &trace));
    match &result {
        Ok(summary) => {
            let detail = summary.replace('\\', "/").replace('"', "'");
            jsonl(home_p, &role, card_n, &trace, "env.up.completed",
                &format!(",\"detail\":\"{}\"", detail));
            emit_spine(home_p, "env.up.completed", &role, card_n, &trace, &[("detail", &detail)]);
        }
        Err(e) => {
            let err = e.replace('\\', "/").replace('"', "'");
            jsonl(home_p, &role, card_n, &trace, "env.up.failed",
                &format!("{},\"error\":\"{}\"", fail_extra("env-up-fail"), err));
            emit_spine(home_p, "env.up.failed", &role, card_n, &trace,
                &[("reason", "env-up-fail"), ("failureClass", failure_class("env-up-fail")), ("disposition", "died"), ("error", &err)]);
        }
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
    emit_spine(home_p, "env.down.started", &role, card_n, &trace, &[]);
    let result = crate::demo_env::env_down(&role, &home, card_n, &trace);
    match &result {
        Ok(summary) => {
            let detail = summary.replace('\\', "/").replace('"', "'");
            jsonl(home_p, &role, card_n, &trace, "env.down.completed",
                &format!(",\"detail\":\"{}\"", detail));
            emit_spine(home_p, "env.down.completed", &role, card_n, &trace, &[("detail", &detail)]);
        }
        Err(e) => {
            let err = e.replace('\\', "/").replace('"', "'");
            jsonl(home_p, &role, card_n, &trace, "env.down.failed",
                &format!("{},\"error\":\"{}\"", fail_extra("env-down-fail"), err));
            emit_spine(home_p, "env.down.failed", &role, card_n, &trace,
                &[("reason", "env-down-fail"), ("failureClass", failure_class("env-down-fail")), ("disposition", "died"), ("error", &err)]);
        }
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
    // #3517 — manual/test/--atomic-recovery deploys carry NO pipeline landedCommit → ungated
    // (None = skip the one-sha gate; there's no pipeline-landed sha to verify against).
    deploy_with_landed(card, role, target, home, werk_base, None)
}

/// #3517 — deploy with the pipeline's landedCommit threaded for the one-sha verify gate.
/// `landed_commit`: None = manual/recovery (skip gate); Some(sha) = pipeline land (deploy_canonical
/// gates deployed-commit == landedCommit); Some("") = pipeline-passed-but-capture-failed (RED).
#[allow(clippy::too_many_arguments)]
pub fn deploy_with_landed(card: u64, role: &str, target: &str, home: &Path, werk_base: &Path, landed_commit: Option<&str>) -> R<String> {
    let trace = resolve_trace(card);
    let branch = branch_name(role, card);
    let werk = werk_base.join(format!("{}-{}", role, card));
    let werk_s = path(&werk)?.to_string();

    jsonl(home, role, card, &trace, "deploy.started", &format!(",\"target\":\"{}\"", target));

    // no-werk-refuse guard (ADR-032 §4): never deploy from canonical.
    if !werk.is_dir() {
        jsonl(home, role, card, &trace, "deploy.refused", &fail_extra("no-werk"));
        return Err(format!("no werk at {} — pull #{} first (deploy never touches canonical source)", werk.display(), card));
    }
    let cur = run_env(Some(&werk_s), &[], "git", &["-C", &werk_s, "rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
    if cur.trim() != branch {
        jsonl(home, role, card, &trace, "deploy.refused", &fail_extra("branch-mismatch"));
        return Err(format!("werk {} is on '{}', not '{}'", werk.display(), cur.trim(), branch));
    }

    // #3222 — target=canonical no longer builds from the werk. It builds the card's
    // crate(s) from CANONICAL ff-synced to origin/main (werk-build --target canonical
    // --only <crates>, the one structural build tool) and installs/verifies via the single
    // prod path (chorus-deploy --target canonical). This RETIRES the werk-sourced canonical
    // build (the merged≠live root) — and with it the #3186 werk-stale guard the from-werk
    // path needed: a build of main can't ship a stale werk tree, so there is nothing to be
    // stale against. The werk is still read here, but ONLY to derive which crates changed.
    if target == "canonical" {
        return deploy_canonical(home, &werk_s, role, card, &trace, landed_commit);
    }

    // serialize all system mutation under one lock.
    let _lock = lock(&werk, Duration::from_secs(180))
        .map_err(|e| died(home, role, card, &trace, "lock-timeout", e))?;
    jsonl(home, role, card, &trace, "lock.acquired", "");

    // AC2: GUARANTEE a rebuild — werk-build compiles+signs in the werk, emits fresh cdhash.
    // CHORUS_TRACE_ID threads the chain into the child verb.
    let build_out = run_env(
        Some(&werk_s),
        &[("CHORUS_TRACE_ID", &trace), ("DEPLOY_ROLE", role), ("CHORUS_ROLE", role)],
        "werk-build",
        &[&card.to_string(), role],
    )
    .map_err(|e| died(home, role, card, &trace, "build-subprocess-fail",
        format!("rebuild failed (werk-build); nothing installed: {}", e)))?;
    let built = parse_build_summary(&build_out);
    if built.is_empty() {
        return Err(died(home, role, card, &trace, "empty-summary",
            "werk-build produced no crate=cdhash pairs — nothing to deploy".to_string()));
    }
    jsonl(home, role, card, &trace, "rebuilt", &format!(",\"built\":\"{}\"", summary(&built)));

    // install + (prod) kickstart + verify, per unit, all-or-nothing.
    // #3092: dispatch by target_class so each kind (Rust service / TS service / CLI verb)
    // walks its own deploy path; no class enum on the verb's top level, just per-unit
    // dispatch (Jeff's framing: deploy all things that got built).
    for (name, built_identity) in &built {
        // #3132 — classify STRUCTURALLY from the werk (no name allowlist).
        let class = target_class_in(name, &werk)?;
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
            TargetClass::TsPackage { name: pkg, dist_dir_rel } => {
                deploy_ts_package(
                    home, &werk_s, role, card, target, &trace, pkg, built_identity, dist_dir_rel,
                )?;
            }
        }
        let _ = register_gh(&werk_s, card, role, &trace, name, built_identity, target);
    }

    let joined = summary(&built);
    jsonl(home, role, card, &trace, "deploy.completed", &format!(",\"target\":\"{}\",\"deployed\":\"{}\"", target, joined));
    Ok(format!("{} target={}", joined, target))
}

// === #3317 — NATIVE canonical deploy engine (absorbs bash chorus-deploy) ===
// The shell-out seam to platform/scripts/chorus-deploy is gone; install/verify/
// kickstart/smoke for canonical deploys is native Rust, driven by structural
// discovery (crate_binaries_in / target_class_in) — no KNOWN_CRATES allowlist.

/// Card id from a commit subject ("silas: #3317 (#534)" → 3317). Bash parity:
/// first `#NNNN` wins; 0 when absent (hand-edited commits).
pub fn card_from_subject(subject: &str) -> u64 {
    let bytes = subject.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' {
            let digits: String = subject[i + 1..].chars().take_while(|c| c.is_ascii_digit()).collect();
            if !digits.is_empty() {
                return digits.parse().unwrap_or(0);
            }
        }
        i += 1;
    }
    0
}

/// #3270 port — is what's now live actually origin/main? true only when HEAD is
/// neither behind nor ahead. Pure half, unit-testable.
pub fn live_main_flag(behind: u64, ahead: u64) -> &'static str {
    if behind == 0 && ahead == 0 { "true" } else { "false" }
}

/// #3517 — the one-sha invariant gate, PURE half (inc1). `deployed` = the origin/main HEAD the
/// build/deploy ran against; `landed` = the trigger's landedCommit threaded forward through
/// werk.yml. RED (false) when they differ (main advanced between trigger and build = drift) OR
/// when `landed` is EMPTY (the werk.yml capture failed → unresolvable → "unknown"=RED, never
/// silent-pass — the claimed≠verified rule at the anchor layer). The one-sha invariant is proven
/// HERE at the verify, not via a risky checkout (#2706 shared-HEAD / detached-HEAD class).
pub fn landed_commit_ok(deployed: &str, landed: &str) -> bool {
    !landed.trim().is_empty() && deployed.trim() == landed.trim()
}

/// #3517 inc2 — the running-proof verdict (the 2026-06-04 stale-daemon catcher), PURE core.
#[derive(Debug, PartialEq, Eq)]
pub enum RunVerdict {
    /// running == built — the daemon restarted onto the built file, or it's a one-shot CLI verb.
    Ok,
    /// installed != built — a broken install → rollback + Err (regardless of process state).
    Mismatch,
    /// the daemon did NOT restart since the deploy (old inode) → kickstart -k ONCE → re-decide.
    Stale,
    /// PID / start-time unresolvable → RED (the contract's "unknown" = RED).
    Unknown,
}

/// #3517 inc2 — decide whether the ACTUALLY-RUNNING binary is the one we built. macOS exposes NO
/// CLI to read a running process's in-memory cdhash: codesign reads the FILE at a path, and an
/// atomic-move install makes codesign(program-path) == built REGARDLESS of whether the daemon
/// restarted. So path-cdhash ALONE false-passes a stale (non-restarted) daemon — exactly the
/// 2026-06-04 outage. "running == built" can only be proven as (installed == built) AND (the
/// process RESTARTED onto that file). `restarted` — not the codesign — is what actually catches
/// stale. CLI verbs are one-shot (no running process) → install-verify (inc1) is their proof → Ok.
/// `restarted` is computed in the thin shell (start_epoch >= install_epoch, from LC_ALL=C
/// lstart→date-j); None = unresolvable PID → Unknown (RED). Pure so the 5 branches unit-test hard.
pub fn running_verdict(
    class_is_daemon: bool,
    built: &str,
    installed: &str,
    restarted: Option<bool>,
) -> RunVerdict {
    if !class_is_daemon {
        return RunVerdict::Ok; // one-shot CLI — no live process; install-verify already gated it
    }
    if installed != built {
        return RunVerdict::Mismatch; // broken install, regardless of the process state
    }
    match restarted {
        None => RunVerdict::Unknown,        // can't resolve the live process → RED
        Some(true) => RunVerdict::Ok,       // restarted onto the built file
        Some(false) => RunVerdict::Stale,   // did not restart = old inode = stale (06-04)
    }
}

fn live_main(root: &str) -> &'static str {
    if run_env(Some(root), &[], "git", &["-C", root, "rev-parse", "--verify", "-q", "origin/main"]).is_err() {
        return "unknown";
    }
    let count = |range: &str| -> u64 {
        run_env(Some(root), &[], "git", &["-C", root, "rev-list", "--count", range])
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(1)
    };
    live_main_flag(count("HEAD..origin/main"), count("origin/main..HEAD"))
}

/// #3181 port — canonical build-source must be synced BEFORE building/installing.
/// Fast-forward canonical to origin/main; REFUSE (don't force) when it can't ff
/// cleanly (ahead/diverged/dirty). No-ops without git / origin/main (can't determine
/// sync state → don't block). This was the merged≠live engine: canonical sat 13
/// commits behind and a deploy shipped a stale daemon while reporting success.
pub fn canonical_ff_sync(root: &str) -> R<()> {
    if run_env(Some(root), &[], "git", &["-C", root, "rev-parse", "--git-dir"]).is_err() {
        return Ok(());
    }
    let _ = run_env(Some(root), &[], "git", &["-C", root, "fetch", "-q", "origin", "main"]);
    if run_env(Some(root), &[], "git", &["-C", root, "rev-parse", "--verify", "-q", "origin/main"]).is_err() {
        return Ok(());
    }
    let count = |range: &str| -> u64 {
        run_env(Some(root), &[], "git", &["-C", root, "rev-list", "--count", range])
            .ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0)
    };
    let behind = count("HEAD..origin/main");
    if behind == 0 {
        return Ok(());
    }
    let ahead = count("origin/main..HEAD");
    let dirty = run_env(Some(root), &[], "git", &["-C", root, "status", "--porcelain"])
        .map(|s| !s.trim().is_empty()).unwrap_or(false);
    if ahead > 0 || dirty {
        return Err(format!(
            "canonical is {} behind origin/main but can't ff cleanly (ahead={}, dirty={}). Run 'werk-sync recover', then re-deploy.",
            behind, ahead, dirty
        ));
    }
    run_env(Some(root), &[], "git", &["-C", root, "merge", "--ff-only", "origin/main"])
        .map(|_| ())
        .map_err(|e| format!("ff to origin/main failed; run 'werk-sync recover': {}", e))
}

/// #2997 port — did the MCP daemon answer a real JSON-RPC initialize as a valid
/// server? The SSE body must carry `protocolVersion` inside a `result` (not an
/// error wrapper, not a generic 200 from the wrong handler). Pure half.
pub fn mcp_init_ready(body: &str) -> bool {
    body.lines().any(|l| {
        let l = l.trim();
        l.starts_with("data:") && l.contains("\"result\"") && l.contains("\"protocolVersion\"")
    })
}

fn smoke_timeout() -> Duration {
    Duration::from_secs(
        env::var("CHORUS_MCP_SMOKE_TIMEOUT_S").ok().and_then(|s| s.parse().ok()).unwrap_or(30),
    )
}

/// Poll a real MCP initialize until the daemon answers as a valid server (#2993/#2997).
fn wait_for_mcp_ready(url: &str) -> R<()> {
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"werk-deploy-smoke","version":"1.0"}}}"#;
    let deadline = Instant::now() + smoke_timeout();
    loop {
        if let Ok(out) = Command::new("curl")
            .args(["-s", "-m", "2", "-X", "POST", url,
                   "-H", "Accept: application/json, text/event-stream",
                   "-H", "Content-Type: application/json", "-d", body])
            .output()
        {
            if mcp_init_ready(&String::from_utf8_lossy(&out.stdout)) {
                return Ok(());
            }
        }
        if Instant::now() >= deadline {
            return Err(format!("MCP smoke at {} timed out after {:?}", url, smoke_timeout()));
        }
        sleep(Duration::from_millis(250));
    }
}

/// #3375 — is this health body healthy? Pure half of the TS health smoke.
/// chorus-api answers {"status":"healthy",...}; pulse and clearing (added to the
/// same smoke by #3352) answer {"status":"ok",...}. The old check required the
/// literal "healthy" string, so both new daemons false-negatived every deploy:
/// 30s of polling against a service answering in 0.17s, then a bare "timeout"
/// refusal (blocked #3366's land 3x, 2026-06-12). Anchored on the status KEY —
/// "ok"/"healthy" elsewhere in the body must not pass (the 6.401ms substring class).
pub fn health_body_ok(body: &str) -> bool {
    let b: String = body.chars().filter(|c| !c.is_whitespace()).collect();
    b.contains("\"status\":\"healthy\"") || b.contains("\"status\":\"ok\"")
}

/// #3375 AC3 — a health-smoke timeout refusal names what it polled, the last
/// body it saw, and the last curl exit — so connection-refused (curl 7) and
/// empty-200 (curl 0, no body) are distinguishable from the refusal alone.
/// "30s timeout" with no evidence made today's false-negative undiagnosable.
pub fn health_timeout_err(
    url: &str,
    timeout: Duration,
    last_body: Option<&str>,
    last_curl_exit: Option<i32>,
) -> String {
    let exit = match last_curl_exit {
        Some(c) => format!("last curl exit: {}", c),
        None => "curl never ran".to_string(),
    };
    let seen = match last_body {
        Some(b) if !b.trim().is_empty() => {
            let snippet: String = b.trim().chars().take(160).collect();
            format!("last body: {} ({})", snippet, exit)
        }
        _ => format!("no response body ({})", exit),
    };
    format!(
        "health smoke at {} timed out after {:?} — {}. If the body looks healthy but isn't matched, the predicate is wrong (werk-deploy health_body_ok), not the service.",
        url, timeout, seen
    )
}

/// Poll a TS daemon's health endpoint until it reports healthy (#2993 port;
/// #3375 shape-tolerant predicate + evidence-carrying refusal).
fn wait_for_api_healthy(url: &str) -> R<()> {
    let deadline = Instant::now() + smoke_timeout();
    let mut last_body: Option<String> = None;
    let mut last_exit: Option<i32> = None;
    loop {
        if let Ok(out) = Command::new("curl").args(["-s", "-m", "2", url]).output() {
            last_exit = out.status.code();
            let body = String::from_utf8_lossy(&out.stdout).to_string();
            if health_body_ok(&body) {
                return Ok(());
            }
            if !body.trim().is_empty() {
                last_body = Some(body);
            }
        }
        if Instant::now() >= deadline {
            return Err(health_timeout_err(url, smoke_timeout(), last_body.as_deref(), last_exit));
        }
        sleep(Duration::from_millis(250));
    }
}

/// #3232 port — a kickstarted daemon is only up once launchctl reports running
/// AND (for com.chorus.hooks) its socket is live. The socket is the liveness the
/// 2026-06-04 outage lacked: launchctl said loaded, the socket was gone, every
/// guard was offline team-wide. Timeout env-overridable for tests.
fn wait_for_service_up(svc: &str) -> R<()> {
    let timeout = Duration::from_secs(
        env::var("CHORUS_DEPLOY_LIVENESS_TIMEOUT_S").ok().and_then(|s| s.parse().ok()).unwrap_or(15),
    );
    wait_for_liveness(svc, timeout)?;
    if svc == "com.chorus.hooks" {
        let sock = env::var("CHORUS_HOOKS_SOCK").unwrap_or_else(|_| "/tmp/chorus-hooks.sock".to_string());
        let deadline = Instant::now() + timeout;
        while !Path::new(&sock).exists() {
            if Instant::now() >= deadline {
                return Err(format!("{} reports running but its socket {} never came up (the 2026-06-04 outage class)", svc, sock));
            }
            sleep(Duration::from_millis(250));
        }
    }
    Ok(())
}

/// The two TS daemons bash chorus-deploy special-cased: (dir_rel, launchd svc,
/// default smoke URL, smoke kind). They live OUTSIDE platform/services/ and are
/// npm-built in place (no dist copy) — the canonical deploy builds them FROM
/// canonical, mirroring the bash branches 1:1.
fn ts_daemon(name: &str) -> Option<(&'static str, &'static str, String, bool)> {
    match name {
        "chorus-api" => Some((
            "platform/api",
            "com.chorus.api",
            env::var("CHORUS_API_HEALTH_URL").unwrap_or_else(|_| "http://localhost:3340/api/chorus/health".to_string()),
            false, // health-check smoke
        )),
        "chorus-mcp" => Some((
            "platform/mcp-server",
            "com.chorus.mcp",
            env::var("CHORUS_MCP_DAEMON_SMOKE_URL").unwrap_or_else(|_| "http://localhost:3341/mcp".to_string()),
            true, // MCP-initialize smoke
        )),
        // #3352 — pulse + clearing were INVISIBLE to deploy discovery (Kade's specimen:
        // #3357's land reported deploy-success while the running pulse stayed stale;
        // the whole 2026-06-11 misdelivery fix needed a hand rebuild + kickstart).
        // Same native TS-daemon path as api/mcp: npm-built in place, kickstart, smoke.
        "pulse" => Some((
            "platform/pulse",
            "com.chorus.pulse",
            env::var("CHORUS_PULSE_HEALTH_URL").unwrap_or_else(|_| "http://localhost:3475/health".to_string()),
            false, // health-check smoke
        )),
        "clearing" => Some((
            "directing/clearing",
            "com.chorus.clearing",
            env::var("CHORUS_CLEARING_HEALTH_URL").unwrap_or_else(|_| "http://localhost:3470/health".to_string()),
            false, // health-check smoke
        )),
        _ => None,
    }
}

/// #3320 — the self-deploy case: deploying chorus-mcp FROM chorus-mcp. The inline
/// kickstart would kill the invoking daemon mid-call — the deploy completes but the
/// caller's MCP response drops (the transport-drop class). Fires ONLY for that exact
/// pair, and never for the detached continuation itself (no respawn loop). Pure half.
pub fn self_deploy_detach_needed(unit: &str, invoker: Option<&str>, already_detached: bool) -> bool {
    unit == "chorus-mcp" && invoker == Some("chorus-mcp") && !already_detached
}

/// #3320 — argv for the detached continuation: a crate-mode redeploy of the ONE unit
/// (the same standalone surface operators use), rollback flag preserved. Pure half.
pub fn detach_argv(unit: &str, rollback: bool) -> Vec<String> {
    let mut v = vec!["crate".to_string(), unit.to_string()];
    if rollback {
        v.push("--rollback".to_string());
    }
    v
}

/// #3320 — is this result the detach ack (not a completed deploy)? Callers must NOT
/// emit deploy.completed for an ack — the detached child emits the real one when it
/// finishes. House style: stringly result, marker-checked (like reason= refusals).
pub fn is_detached_ack(s: &str) -> bool {
    s.contains("deploy detached pid=")
}

/// #3320 — spawn the detached continuation and ack immediately. The child is its own
/// process group (survives the parent chorus-mcp's kickstart), runs the normal
/// crate-mode path with CHORUS_DETACHED=1 (so detection won't re-fire) and the SAME
/// trace id, and emits the usual deploy.completed — that spine event is the caller's
/// poll handle. Output goes to <home>/ops/logs/werk-deploy-detached.log. The npm build
/// in the child (seconds) is the grace window that lets this ack reach the caller
/// before the kickstart lands.
fn detach_self_deploy(
    home: &Path, role: &str, card: u64, trace: &str, name: &str, rollback: bool,
) -> R<String> {
    use std::os::unix::process::CommandExt;
    let exe = match env::var("WERK_DEPLOY_SELF_BIN") {
        Ok(p) if !p.trim().is_empty() => PathBuf::from(p),
        _ => env::current_exe().map_err(|e| format!("current_exe for detach failed: {}", e))?,
    };
    let log_dir = home.join("ops/logs");
    let _ = fs::create_dir_all(&log_dir);
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("werk-deploy-detached.log"))
        .map_err(|e| format!("detached log open failed: {}", e))?;
    let err_log = log.try_clone().map_err(|e| format!("detached log clone failed: {}", e))?;
    // #3323 — the detached child loses the user security session, so its gh keyring
    // auth 401s (broke werk-pull/werk-merge team-wide after every self-deploy). The
    // PARENT still has keychain access: capture `gh auth token` here and inject
    // GH_TOKEN into the child's env — scoped to process env, never written to disk
    // or logs. Capture failure is NON-FATAL but witnessed: a gh-less child is
    // strictly better than no deploy.
    let gh_token = Command::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|t| !t.is_empty());
    if gh_token.is_none() {
        jsonl(home, role, card, trace, "gh-token-capture.failed",
            &format!("{},\"detail\":\"parent could not capture gh auth token; detached child runs gh-less\"", fail_extra("gh-token-capture-failed")));
        emit_spine(home, "deploy.detach.warn", role, card, trace,
            &[("crate", name), ("reason", "gh-token-capture-failed"), ("failureClass", failure_class("gh-token-capture-failed"))]);
    }
    let mut cmd = Command::new(&exe);
    cmd.args(detach_argv(name, rollback))
        .env("CHORUS_DETACHED", "1")
        .env("CHORUS_TRACE_ID", trace)
        .env("DEPLOY_ROLE", role)
        .env("CHORUS_ROLE", role)
        .stdin(std::process::Stdio::null())
        .stdout(log)
        .stderr(err_log)
        .process_group(0);
    if let Some(t) = &gh_token {
        cmd.env("GH_TOKEN", t);
    }
    let child = cmd
        .spawn()
        .map_err(|e| format!("detached self-deploy spawn failed: {}", e))?;
    let pid = child.id().to_string();
    emit_spine(home, "deploy.detached", role, card, trace, &[("crate", name), ("pid", &pid)]);
    jsonl(home, role, card, trace, "deploy.detached",
        &format!(",\"name\":\"{}\",\"kind\":\"ts-daemon\",\"pid\":{},\"rollback\":{}", name, pid, rollback));
    Ok(format!(
        "{} deploy detached pid={} — survives its own kickstart; poll spine deploy.completed trace={}",
        name, pid, trace
    ))
}

/// #3317 — native canonical deploy of a TS daemon (chorus-api / chorus-mcp), the
/// bash branches ported: preserve dist→dist.prev, npm-build IN canonical, kickstart
/// (warn-only, like bash), smoke-gate deploy.completed, restore dist.prev on build
/// fail. `rollback=true` restores dist.prev + kickstart + smoke instead.
fn deploy_ts_daemon_canonical(
    home: &Path, role: &str, card: u64, trace: &str, name: &str, rollback_mode: bool,
) -> R<String> {
    // #3320 — mcp-from-mcp would kill its own caller at kickstart; hand off to the
    // detached continuation and ack now, while the transport is still alive.
    if self_deploy_detach_needed(
        name,
        env::var("CHORUS_INVOKER").ok().as_deref(),
        env::var("CHORUS_DETACHED").is_ok(),
    ) {
        return detach_self_deploy(home, role, card, trace, name, rollback_mode);
    }
    let (dir_rel, svc, smoke_url, is_mcp) =
        ts_daemon(name).ok_or_else(|| format!("{} is not a TS daemon", name))?;
    let root = canonical_root_path(home);
    let dir = format!("{}/{}", root, dir_rel);
    let dist = format!("{}/dist", dir);
    let prev = format!("{}.prev", dist);
    if !Path::new(&dir).is_dir() {
        return Err(died(home, role, card, trace, "dir-not-found", format!("{} dir not found at {}", name, dir)));
    }
    let kickstart = || run_env(None, &[], "launchctl", &["kickstart", "-k", &format!("gui/{}/{}", uid(), svc)]);
    let smoke = || if is_mcp { wait_for_mcp_ready(&smoke_url) } else { wait_for_api_healthy(&smoke_url) };

    if rollback_mode {
        if !Path::new(&prev).is_dir() {
            return Err(format!("no dist.prev to rollback to ({} not found) — no prior deploy to restore", prev));
        }
        let _ = fs::remove_dir_all(&dist);
        fs::rename(&prev, &dist).map_err(|e| format!("restore {} → {}: {}", prev, dist, e))?;
        let _ = kickstart(); // bash parity: warn-only
        smoke().map_err(|e| died(home, role, card, trace, "smoke-timeout-rollback", e))?;
        emit_spine(home, "deploy.rolled_back", role, card, trace, &[("crate", name)]);
        jsonl(home, role, card, trace, "deploy.rolled_back", &format!(",\"name\":\"{}\",\"kind\":\"ts-daemon\"", name));
        return Ok(format!("{} rolled back", name));
    }

    // preserve current dist for rollback, then build from canonical source.
    if Path::new(&dist).is_dir() {
        let _ = fs::remove_dir_all(&prev);
        fs::rename(&dist, &prev).map_err(|e| format!("preserve {} → {}: {}", dist, prev, e))?;
    }
    if let Err(e) = run_env(Some(&dir), &[], "npm", &["run", "build"]) {
        // restore disk to match the still-running prior code.
        if Path::new(&prev).is_dir() {
            let _ = fs::remove_dir_all(&dist);
            let _ = fs::rename(&prev, &dist);
        }
        emit_spine(home, "deploy.failed", role, card, trace, &[("crate", name), ("reason", "npm-build-fail"), ("failureClass", failure_class("npm-build-fail"))]);
        return Err(died(home, role, card, trace, "npm-build-fail", format!("npm run build failed for {}: {}", name, e)));
    }
    let _ = kickstart(); // bash parity: warn-only — the smoke is the gate
    if let Err(e) = smoke() {
        emit_spine(home, "deploy.failed", role, card, trace, &[("crate", name), ("reason", "smoke-timeout"), ("failureClass", failure_class("smoke-timeout"))]);
        return Err(died(home, role, card, trace, "smoke-timeout", format!("{} smoke failed after deploy: {}", name, e)));
    }
    jsonl(home, role, card, trace, "installed", &format!(",\"name\":\"{}\",\"kind\":\"ts-daemon\",\"target\":\"canonical\"", name));
    Ok(format!("{} deployed", name))
}

/// #3317 — native canonical deploy of ONE unit (the per-crate engine that replaces
/// `chorus-deploy --target canonical <crate>`). Dispatches: TS daemons → npm-build
/// path; Rust crates → the same install/verify/kickstart path the werk slot uses
/// (deploy_rust_service / deploy_cli_verb), rooted at CANONICAL (where werk-build
/// --target canonical just built). Emits the #3270 deploy.completed envelope.
fn deploy_crate_canonical(home: &Path, role: &str, card: u64, trace: &str, name: &str) -> R<String> {
    let start = Instant::now();
    let root = canonical_root_path(home);
    let artifact_class;
    if ts_daemon(name).is_some() {
        let out = deploy_ts_daemon_canonical(home, role, card, trace, name, false)?;
        // #3320 — a detach ack is NOT a completed deploy: the detached child emits
        // the real deploy.completed when it finishes. Return the ack as-is.
        if is_detached_ack(&out) {
            return Ok(out);
        }
        artifact_class = "ts-daemon";
    } else {
        let class = target_class_in(name, Path::new(&root))?;
        match &class {
            TargetClass::RustService { svc, bin } => {
                let built = file_cdhash(&format!("{}/platform/services/{}/target/release/{}", root, name, bin))
                    .unwrap_or_default();
                deploy_rust_service(home, &root, role, card, "canonical", trace, name, &built, svc, bin)?;
                artifact_class = "rust-daemon";
            }
            TargetClass::CliVerb { bin } => {
                let built = file_cdhash(&format!("{}/platform/services/{}/target/release/{}", root, name, bin))
                    .ok_or_else(|| format!(
                        "binary missing for {} — run werk-build --target canonical first ({}/platform/services/{}/target/release/{})",
                        name, root, name, bin
                    ))?;
                deploy_cli_verb(home, &root, role, card, "canonical", trace, name, &built, bin)?;
                artifact_class = "rust-cli";
            }
            other => {
                return Err(format!(
                    "canonical deploy for {:?} is not supported by the native engine yet (unit {}) — TS services/packages deploy via the in-flow path",
                    other, name
                ));
            }
        }
    }
    // #3270 — deploy.completed carries the ground-truth envelope: loaded cdhash,
    // live_main (is what's now live actually origin/main?), artifact_class.
    let commit = run_env(Some(&root), &[], "git", &["-C", &root, "rev-parse", "--short", "HEAD"])
        .map(|s| s.trim().to_string()).unwrap_or_else(|_| "unknown".to_string());
    let cdhash = match target_class_in(name, Path::new(&root)) {
        Ok(TargetClass::RustService { bin, .. }) | Ok(TargetClass::CliVerb { bin }) => {
            file_cdhash(&installed_path("canonical", role, &bin)).unwrap_or_default()
        }
        _ => String::new(),
    };
    let ms = start.elapsed().as_millis().to_string();
    emit_spine(home, "deploy.completed", role, card, trace, &[
        ("verb", "deploy"), ("step", "deploy"), ("outcome", "success"),
        ("artifact_class", artifact_class), ("commit", &commit), ("crate", name),
        ("cdhash", &cdhash), ("deploy_target", "canonical"), ("live_main", live_main(&root)),
        ("duration_ms", &ms),
    ]);
    Ok(name.to_string())
}

/// #3317 — `werk-deploy crate <name> [--rollback]`: the standalone crate-scoped
/// canonical deploy that replaces bash `chorus-deploy <crate> [--rollback]` for
/// operational callers (agent-state.sh deploy/rollback, ops by hand). ff-syncs
/// canonical first (#3181), resolves role from DEPLOY_ROLE/CHORUS_ROLE and card
/// from the latest commit subject (bash parity).
fn run_crate_mode(args: &[String]) -> R<String> {
    let name = args.iter().find(|a| !a.starts_with("--"))
        .ok_or_else(|| "usage: werk-deploy crate <name> [--rollback]".to_string())?
        .clone();
    let rollback_mode = args.iter().any(|a| a == "--rollback");
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);
    let root = canonical_root_path(&home);
    let role = env::var("DEPLOY_ROLE").or_else(|_| env::var("CHORUS_ROLE")).unwrap_or_else(|_| "system".to_string());
    let card = run_env(Some(&root), &[], "git", &["-C", &root, "log", "-1", "--pretty=%s"])
        .map(|s| card_from_subject(&s)).unwrap_or(0);
    let trace = resolve_trace(card);
    if !rollback_mode {
        canonical_ff_sync(&root)?;
    }
    if rollback_mode {
        if ts_daemon(&name).is_some() {
            return deploy_ts_daemon_canonical(&home, &role, card, &trace, &name, true);
        }
        // Rust crate: restore each binary via chorus-bin-install --rollback, then
        // kickstart + liveness when it's a service.
        let crate_dir = Path::new(&root).join(format!("platform/services/{}", name));
        let cbi = chorus_bin_install_cmd(&home, &root);
        for (b, _) in crate_binaries_with_service_in(&crate_dir, &name) {
            run_env(Some(&root), &[("CHORUS_ROLE", &role)], &cbi, &["--target", "canonical", "--rollback", &b])
                .map_err(|e| format!("rollback of {} failed: {}", b, e))?;
        }
        if let Ok(TargetClass::RustService { svc, .. }) = target_class_in(&name, Path::new(&root)) {
            run_env(None, &[], "launchctl", &["kickstart", "-k", &format!("gui/{}/{}", uid(), svc)])
                .map_err(|e| format!("kickstart {} after rollback failed: {}", svc, e))?;
            wait_for_service_up(&svc)?;
        }
        emit_spine(&home, "deploy.rolled_back", &role, card, &trace, &[("crate", &name)]);
        return Ok(format!("{} rolled back target=canonical", name));
    }
    jsonl(&home, &role, card, &trace, "deploy.started", &format!(",\"target\":\"canonical\",\"mode\":\"crate\",\"crate\":\"{}\"", name));
    let out = deploy_crate_canonical(&home, &role, card, &trace, &name)?;
    // #3320 — detach ack: the detached child writes the real deploy.completed.
    if is_detached_ack(&out) {
        return Ok(out);
    }
    // Witness the standalone crate deploy in the jsonl (the in-flow path writes its own
    // whole-deploy deploy.completed in deploy_canonical; this is the crate-mode analogue).
    jsonl(&home, &role, card, &trace, "deploy.completed", &format!(",\"target\":\"canonical\",\"mode\":\"crate\",\"deployed\":\"{}\"", name));
    Ok(format!("{} deployed target=canonical", name))
}

/// The card's changed SERVICE crate(s): names under `platform/services/<X>/` in the diff,
/// de-duplicated, order-stable. These are the only units a prod deploy needs to (re)build
/// from canonical + install — everything else on main is already built and deployed.
pub fn changed_service_crates(diff: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in diff.lines() {
        if let Some(rest) = line.trim().strip_prefix("platform/services/") {
            if let Some(name) = rest.split('/').next() {
                if !name.is_empty() && !out.iter().any(|c| c == name) {
                    out.push(name.to_string());
                }
            }
        }
    }
    out
}

/// #3243 — the card's changed TS SERVICES: chorus-mcp (`platform/mcp-server`) and chorus-api
/// (`platform/api`). These live OUTSIDE `platform/services/`, so `changed_service_crates`
/// misses them — the merged≠live hole that made #3239 (env-up forward) and #3241 (chorus_werk)
/// merge to main but stay un-deployed until a manual deploy. They are npm-built in place
/// (dist + kickstart), NOT by `werk-build` (target/release), so deploy_canonical routes
/// them to the native TS-daemon path (#3317) and skips the werk-build step.
pub fn changed_ts_services(diff: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |name: &str| {
        if !out.iter().any(|c| c == name) {
            out.push(name.to_string());
        }
    };
    for line in diff.lines() {
        let l = line.trim();
        if l.starts_with("platform/mcp-server/") {
            add("chorus-mcp");
        } else if l.starts_with("platform/api/") {
            add("chorus-api");
        } else if l.starts_with("platform/pulse/") {
            // #3352 — see ts_daemon: pulse/clearing join the discovery so a merged
            // change actually deploys (the merged-but-stale class, 3 specimens today).
            add("pulse");
        } else if l.starts_with("directing/clearing/") {
            add("clearing");
        }
    }
    out
}

/// #3222 — target=canonical: build the card's crate(s) from CANONICAL ff-synced to
/// origin/main (werk-build --target canonical --only), then install/verify/kickstart each
/// via the native engine (deploy_crate_canonical, #3317). The werk is read ONLY to
/// derive which crates changed (merge-base diff — survives the squash-merge: the merge-base
/// is the stable fork point even after origin/main moves ahead). NOTHING is built from the
/// werk: prod binaries are structurally a build of main (the merged≠live fix).
fn deploy_canonical(home: &Path, werk_s: &str, role: &str, card: u64, trace: &str, landed_commit: Option<&str>) -> R<String> {
    let _ = run_env(Some(werk_s), &[], "git", &["-C", werk_s, "fetch", "-q", "origin", "main"]);
    let base = run_env(Some(werk_s), &[], "git", &["-C", werk_s, "merge-base", "origin/main", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let range = if base.is_empty() { "origin/main..HEAD".to_string() } else { format!("{}..HEAD", base) };
    let diff = run_env(Some(werk_s), &[], "git", &["-C", werk_s, "diff", "--name-only", &range])
        .unwrap_or_default();
    let crates = changed_service_crates(&diff);
    // #3243 — TS services (chorus-mcp at platform/mcp-server, chorus-api at platform/api) live
    // OUTSIDE platform/services/, so changed_service_crates misses them. They are npm-built
    // in place by the native TS-daemon path (#3317), WITHOUT a werk-build step.
    let ts = changed_ts_services(&diff);

    if crates.is_empty() && ts.is_empty() {
        // Docs/config/graph-only card — no service to build for prod. Clean no-op so
        // the acp chain proceeds (mirrors werk-build's no-build-units case).
        jsonl(home, role, card, trace, "deploy.completed",
            ",\"target\":\"canonical\",\"deployed\":\"\",\"reason\":\"no-service-crates\"");
        return Ok("nothing to deploy (no service crates changed) target=canonical".to_string());
    }

    // Build the card's RUST crate(s) from canonical@origin/main — the one structural build
    // tool, crate-scoped so a prod deploy never rebuilds the whole tree (no full-repo tsc at
    // acp, no coupling the card's deploy to unrelated crates' health on main). TS services are
    // NOT built here — the native TS-daemon path npm-builds them from canonical below (#3317).
    let mut prod_hashes: Vec<(String, String)> = Vec::new();
    if !crates.is_empty() {
        let only = crates.join(",");
        let build_out = run_env(
            Some(werk_s),
            &[("CHORUS_TRACE_ID", trace), ("DEPLOY_ROLE", role), ("CHORUS_ROLE", role)],
            "werk-build",
            &[&card.to_string(), role, "--target", "canonical", "--only", &only],
        )
        .map_err(|e| died(home, role, card, trace, "canonical-build-fail",
            format!("werk-build --target canonical failed; nothing installed: {}", e)))?;
        prod_hashes = parse_build_summary(&build_out);
    }

    // #3316 — prove "what you demo is what ships". Prod builds from merged main (NOT a copy
    // of the demo binary); because build is a pure function of source, the prod cdhash MUST
    // equal the demo'd one. A divergence = source moved between demo and land (the integration
    // trap). Emitted, NOT yet a hard gate: a divergence is WARNED (deploy.cdhash.diverged) so
    // the live land path is never false-blocked while equality is confirmed across unit types
    // (follow-on: flip to refuse once proven). Rust crates only — TS dist-SHA determinism is
    // unproven, so TS is left for the follow-on rather than risk false-refusing TS lands.
    if !prod_hashes.is_empty() {
        let demo = fs::read_to_string(home.join("ops/logs/werk-deploy.jsonl"))
            .map(|c| demo_cdhashes(&c, card))
            .unwrap_or_default();
        if demo.is_empty() {
            jsonl(home, role, card, trace, "deploy.cdhash.unverified",
                ",\"reason\":\"no-demo-baseline\"");
        } else {
            let div = cdhash_divergences(&demo, &prod_hashes);
            if div.is_empty() {
                jsonl(home, role, card, trace, "deploy.cdhash.matched",
                    &format!(",\"crates\":\"{}\"", summary(&prod_hashes)));
                emit_spine(home, "deploy.cdhash.matched", role, card, trace,
                    &[("crates", &summary(&prod_hashes))]);
            } else {
                for (c, dh, ph) in &div {
                    jsonl(home, role, card, trace, "deploy.cdhash.diverged",
                        &format!(",\"crate\":\"{}\",\"demo\":\"{}\",\"prod\":\"{}\"", c, dh, ph));
                    emit_spine(home, "deploy.cdhash.diverged", role, card, trace,
                        &[("crate", c), ("demo", dh), ("prod", ph)]);
                }
            }
        }
    }

    // #3317 — install/verify/kickstart each NATIVELY (the engine that absorbed bash
    // chorus-deploy). Rust crates read the freshly-built canonical target/release
    // (where werk-build --target canonical just built); TS daemons npm-build from
    // canonical. #3181 ff is a no-op here (werk-build already ff-synced canonical;
    // a TS-only card has no werk-build step, so ff-sync canonical first).
    let home_s = canonical_root_path(home);
    if crates.is_empty() {
        canonical_ff_sync(&home_s)
            .map_err(|e| died(home, role, card, trace, "canonical-ff-fail", e))?;
    }
    let deployed: Vec<String> = crates.iter().chain(ts.iter()).cloned().collect();
    let mut labels: Vec<String> = Vec::new();
    for c in &deployed {
        let out = deploy_crate_canonical(home, role, card, trace, c)
            .map_err(|e| died(home, role, card, trace, "canonical-deploy-fail",
                format!("canonical deploy of {} failed: {}", c, e)))?;
        // #3320 — a detached unit is in flight, not deployed; label it honestly in the
        // whole-deploy envelope (its own deploy.completed comes from the child).
        labels.push(if is_detached_ack(&out) { format!("{}(detached)", c) } else { c.clone() });
    }

    let only = labels.join(",");

    // #3517 — the one-sha invariant GATE (AC5). The canonical deploy built origin/main HEAD;
    // verify it == the trigger's landedCommit so "what ran == the card's landed sha", end to end.
    // None = manual/--atomic recovery deploy (no pipeline sha to verify against → ungated).
    // Some(landed): commitVerified iff deployed-HEAD == landed AND landed non-empty.
    let root = canonical_root_path(home);
    let deployed_commit = run_env(Some(root.as_str()), &[], "git",
        &["-C", root.as_str(), "rev-parse", "origin/main"])
        .unwrap_or_default().trim().to_string();
    let commit_verified = match landed_commit {
        None => true, // ungated manual/recovery deploy — nothing to verify against
        Some(landed) => landed_commit_ok(&deployed_commit, landed),
    };
    jsonl(home, role, card, trace, "deploy.completed",
        &format!(",\"target\":\"canonical\",\"deployed\":\"{}\",\"landedCommit\":\"{}\",\"deployedCommit\":\"{}\",\"commitVerified\":{}",
            only, landed_commit.unwrap_or(""), deployed_commit, commit_verified));
    if !commit_verified {
        // Commit-DRIFT (deployed = valid newer-main != landedCommit) or EMPTY landedCommit (the
        // werk.yml capture failed) → RED the card, Err-ONLY (NO rollback). Principle (Kade, #3517):
        // ROLLBACK IS FOR A BROKEN RUNTIME, NOT FOR A VALID-BUT-NOT-MY-EXACT-SHA — degrading a valid
        // newer-main binary to an older one over a metadata/claim problem is strictly worse. The
        // cdhash-MISMATCH (broken install) case keeps its per-unit rollback above; this is distinct.
        // Self-heals: the idempotent re-trigger resolves landedCommit = current HEAD → green on re-run.
        return Err(format!(
            "one-sha gate RED: deployed origin/main HEAD '{}' != landedCommit '{}' (commit drift, or empty = werk.yml capture failed) — card not verified against its landed sha; runtime left valid, re-run to self-heal",
            deployed_commit, landed_commit.unwrap_or("")
        ));
    }
    Ok(format!("{} target=canonical", only))
}

fn summary(built: &[(String, String)]) -> String {
    built.iter().map(|(c, h)| format!("{}={}", c, h)).collect::<Vec<_>>().join(",")
}

/// #3092 — deploy a Rust SERVICE crate (chorus-hooks/inject/mcp).
/// Existing pre-#3092 behavior, extracted: chorus-bin-install → kickstart →
/// codesign cdhash verify running==built. The stale-build guard (cdhash-divergence
/// refuse) fires only on canonical (where there's a running binary to compare).
#[allow(clippy::too_many_arguments)]
/// codesign cdhash of a binary file (the linker's ad-hoc signature is enough to read
/// one). None if the file is absent / unsigned. #3179 — per-binary cdhash, computed
/// from the werk's built files, replacing reliance on the build summary's single
/// per-crate hash (which for chorus-hooks was the shim's).
fn file_cdhash(path: &str) -> Option<String> {
    run_env(None, &[], "codesign", &["-d", "--verbose=4", path])
        .ok()
        .and_then(|o| extract_running_cdhash(&o))
}

/// #3517 — current unix epoch (seconds). The install_epoch baseline for the inc2 running-proof.
fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// #3517 inc2 — did the daemon's LIVE process restart onto the just-installed file? Impure
/// (launchctl/ps/date), thin wrapper over the pure `running_verdict`. None = PID unresolvable
/// (→ Unknown → RED). Resolves an ABSOLUTE start_epoch via LC_ALL=C lstart→date-j (verified across
/// hooks/api/clearing; LC_ALL=C pins the %a/%b parse so it can't locale-drift), then compares
/// start_epoch >= install_epoch. macOS exposes no in-memory cdhash, so restart-after-install is the
/// actual proof that the running binary == the built one (path-codesign alone false-passes stale).
fn resolve_restarted(svc: &str, install_epoch: u64) -> Option<bool> {
    let print = run_env(None, &[], "launchctl", &["print", &format!("gui/{}/{}", uid(), svc)])
        .unwrap_or_default();
    let pid = print
        .lines()
        .find_map(|l| l.trim().strip_prefix("pid = ").map(|s| s.trim().to_string()))?;
    let lstart = run_env(None, &[("LC_ALL", "C")], "ps", &["-p", &pid, "-o", "lstart="]).unwrap_or_default();
    let lstart = lstart.trim();
    if lstart.is_empty() {
        return None;
    }
    let start_epoch: u64 = run_env(None, &[("LC_ALL", "C")], "date",
        &["-j", "-f", "%a %b %d %H:%M:%S %Y", lstart, "+%s"])
        .ok()
        .and_then(|s| s.trim().parse().ok())?;
    Some(start_epoch >= install_epoch)
}

// ADR-032 §1 testable-core: every input explicit (no hidden env/global reads) so the
// deploy path is exercisable against a temp repo with shimmed CLIs — hence the arg count.
#[allow(clippy::too_many_arguments)]
fn deploy_rust_service(
    home: &Path, werk_s: &str, role: &str, card: u64, target: &str, trace: &str,
    crate_name: &str, built_summary_cdhash: &str, svc: &str, bin: &str,
) -> R<()> {
    // #3179/#3317 — install EVERY binary the crate emits, discovered STRUCTURALLY from
    // the crate's own Cargo.toml + src/bin (no hardcoded list to drift). `werk_s` is the
    // SOURCE ROOT the built artifacts live under — the card's werk for in-werk deploys,
    // canonical for build-from-main canonical deploys (#3317).
    let crate_dir = Path::new(werk_s).join(format!("platform/services/{}", crate_name));
    let bins = crate_binaries_with_service_in(&crate_dir, crate_name);
    let built_path = |b: &str| format!("{}/platform/services/{}/target/release/{}", werk_s, crate_name, b);

    // AC2 stale-build guard + #3132 hash-gate (canonical only). Skip install+kickstart
    // only when EVERY binary is already installed == built AND source unchanged.
    if target == "canonical" {
        let all_match = bins.iter().all(|(b, _)| {
            let built = file_cdhash(&built_path(b));
            built.is_some() && built == file_cdhash(&installed_path(target, role, b))
        });
        if all_match {
            if crate_source_changed(werk_s, crate_name) {
                // All binaries match the installed cdhash but source changed this card →
                // the rebuild didn't take. Refuse rather than ship stale.
                jsonl(home, role, card, trace, "deploy.refused", &fail_extra("cdhash-divergence"));
                return Err(format!(
                    "cdhash-divergence: rebuild of {} matches the installed cdhash but its source changed this card — stale build, refusing (werk-build invariance suspect)",
                    crate_name
                ));
            }
            jsonl(home, role, card, trace, "deploy.skipped",
                &format!(",\"name\":\"{}\",\"kind\":\"rust-service\",\"reason\":\"unchanged\",\"cdhash\":\"{}\"", crate_name, built_summary_cdhash));
            return Ok(());
        }
    }

    let cbi = chorus_bin_install_cmd(home, werk_s);
    for (b, _) in &bins {
        if let Err(e) = run_env(
            Some(werk_s), &[("CHORUS_ROLE", role)], &cbi,
            &["--target", target, &built_path(b), b],
        ) {
            rollback(home, werk_s, role, card, trace, target, b, "install-fail");
            return Err(format!("install of {} failed; rolled back: {}", b, e));
        }
        jsonl(home, role, card, trace, "installed",
            &format!(",\"name\":\"{}\",\"bin\":\"{}\",\"kind\":\"rust-service\",\"target\":\"{}\"", crate_name, b, target));
    }

    // #3517 inc2 — timestamp the atomic-move complete. A live process whose start-epoch >= this
    // loaded the just-installed binary; captured BEFORE kickstart so a real restart is strictly after.
    let install_epoch = now_epoch();

    if target == "canonical" {
        if let Err(e) = run_env(None, &[], "launchctl", &["kickstart", "-k", &format!("gui/{}/{}", uid(), svc)]) {
            rollback(home, werk_s, role, card, trace, target, bin, "kickstart-fail");
            return Err(format!("kickstart {} failed; rolled back: {}", svc, e));
        }
        // #3317 (bash #3232 port) — verify the daemon actually came UP. `launchctl
        // kickstart` returns BEFORE the daemon binds, so a crash-on-start otherwise
        // ships green while the service is down (the 2026-06-04 chorus-hooks outage:
        // launchctl said loaded, the socket was gone). launchctl liveness for every
        // service; for com.chorus.hooks additionally require its socket to be live.
        if let Err(e) = wait_for_service_up(svc) {
            rollback(home, werk_s, role, card, trace, target, bin, "not-running");
            return Err(format!("{} did not come up after kickstart; rolled back: {}", svc, e));
        }
        // #3179 — verify the DAEMON (service) binary specifically: `bin` is the
        // is_service binary com.chorus.<svc> runs. Installed cdhash == its built
        // cdhash. Targeting the shim here was the false-green that left the daemon stale.
        let built = file_cdhash(&built_path(bin));
        let installed = file_cdhash(&installed_path(target, role, bin));
        match (&built, &installed) {
            (Some(b), Some(i)) if b == i => {
                // inc1 install-verify passed (installed == built file). #3517 inc2 — the RUNNING-proof:
                // also require the daemon RESTARTED onto it. installed==built ALONE false-passes a stale
                // daemon (codesign reads the file, not the live process — the 2026-06-04 outage). Bounded
                // reload ONCE; record restartedAfterInstall (the auditable catcher) on the spine.
                match running_verdict(true, b, i, resolve_restarted(svc, install_epoch)) {
                    RunVerdict::Ok => {
                        jsonl(home, role, card, trace, "verified",
                            &format!(",\"name\":\"{}\",\"bin\":\"{}\",\"cdhash\":\"{}\",\"installedCdhash\":\"{}\",\"equal\":true,\"restartedAfterInstall\":true", crate_name, bin, i, i));
                    }
                    RunVerdict::Stale => {
                        // started before the install (old inode) → force-reload ONCE, re-decide. No loop.
                        let _ = run_env(None, &[], "launchctl", &["kickstart", "-k", &format!("gui/{}/{}", uid(), svc)]);
                        let _ = wait_for_service_up(svc);
                        match running_verdict(true, b, i, resolve_restarted(svc, install_epoch)) {
                            RunVerdict::Ok => {
                                jsonl(home, role, card, trace, "verified",
                                    &format!(",\"name\":\"{}\",\"bin\":\"{}\",\"cdhash\":\"{}\",\"equal\":true,\"restartedAfterInstall\":true,\"reloaded\":true", crate_name, bin, i));
                            }
                            _ => {
                                // still stale after one reload = real failure. Err, NOT rollback: the file
                                // is good; restoring an OLDER binary can't fix a stale-RUNNING daemon.
                                jsonl(home, role, card, trace, "deploy.stale",
                                    &format!(",\"name\":\"{}\",\"bin\":\"{}\",\"installedCdhash\":\"{}\",\"equal\":true,\"restartedAfterInstall\":false", crate_name, bin, i));
                                return Err(format!(
                                    "daemon {} did not reload onto the built binary after kickstart -k (stale-running, 06-04 class) — RED; runtime left as-is (rollback can't fix stale-running)",
                                    svc
                                ));
                            }
                        }
                    }
                    RunVerdict::Unknown => {
                        // installed==built but the live PID/start-time is unresolvable → can't PROVE
                        // running==built. RED (unknown=RED). No rollback — the install itself is good.
                        jsonl(home, role, card, trace, "deploy.unverifiable",
                            &format!(",\"name\":\"{}\",\"bin\":\"{}\",\"installedCdhash\":\"{}\",\"equal\":true,\"restartedAfterInstall\":\"unknown\"", crate_name, bin, i));
                        return Err(format!(
                            "could not resolve {}'s running process to verify running==built (unknown=RED) — install good, runtime unverified",
                            svc
                        ));
                    }
                    // installed==built is guaranteed by the match guard → Mismatch can't arise here.
                    RunVerdict::Mismatch => unreachable!("installed==built guaranteed by the (Some(b),Some(i)) if b==i guard"),
                }
            }
            _ => {
                rollback(home, werk_s, role, card, trace, target, bin, "cdhash-mismatch");
                return Err(format!(
                    "running != built for daemon binary {} of {}: built={:?} installed={:?} — rolled back (stale-binary guard)",
                    bin, crate_name, built, installed
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
        let started = crate::demo_env::env_up(role, werk_s, &canonical_root, card, trace)
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
        return Err(died(home, role, card, trace, "dist-not-found", format!(
            "TS service deploy: werk dist not found at {} (werk-build should have produced it)",
            werk_dist
        )));
    }

    // #3132 HASH-GATE: if the live canonical dist already hashes to the just-built
    // identity, this service DID NOT CHANGE — skip the copy AND the kickstart. This is
    // the "don't restart everything every card" fix: build is global, but a unchanged
    // service is never bounced. The gate is keyed on byte-identity vs what's running,
    // computed fresh — it can't go stale and it can't be forgotten (unlike a path list).
    if let Ok(live_sha) = dist_sha(&canonical_dist) {
        if live_sha == built_dist_sha {
            jsonl(home, role, card, trace, "deploy.skipped",
                &format!(",\"name\":\"{}\",\"kind\":\"ts-service\",\"reason\":\"unchanged\",\"identity\":\"{}\"", svc_name, built_dist_sha));
            return Ok(());
        }
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
        jsonl(home, role, card, trace, "deploy.rolled_back", ",\"reason\":\"ts-install-fail\"");
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
        jsonl(home, role, card, trace, "deploy.rolled_back", ",\"reason\":\"ts-identity-mismatch-pre-kickstart\"");
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
        jsonl(home, role, card, trace, "deploy.rolled_back", ",\"reason\":\"ts-kickstart-fail\"");
        return Err(format!("kickstart {} failed; restored prev dist: {}", svc, e));
    }

    // #3132 — VERIFY the restart took. If the service self-declared a health URL,
    // smoke it (proves it SERVES the new code — the strong proof, e.g. chorus-api).
    // Otherwise fall back to launchd LIVENESS (proves the process came up and isn't
    // crash-looping). A MISSING health declaration NEVER fails the deploy — it only
    // downgrades the proof. (Kade's #3132 floor: optional metadata must not gate the
    // structural deploy, or "not declared" becomes the new "not on the list → stale".)
    let verify = if smoke_url.is_empty() {
        jsonl(home, role, card, trace, "deploy.verify.liveness", &format!(",\"name\":\"{}\",\"svc\":\"{}\"", svc_name, svc));
        wait_for_liveness(svc, Duration::from_secs(15))
    } else {
        jsonl(home, role, card, trace, "deploy.verify.smoke", &format!(",\"name\":\"{}\",\"url\":\"{}\"", svc_name, smoke_url));
        wait_for_health(smoke_url, Duration::from_secs(30))
    };
    if let Err(e) = verify {
        let _ = fs::remove_dir_all(&canonical_dist);
        if Path::new(&canonical_dist_prev).is_dir() {
            let _ = fs::rename(&canonical_dist_prev, &canonical_dist);
        }
        let _ = run_env(None, &[], "launchctl", &["kickstart", "-k", &format!("gui/{}/{}", uid(), svc)]);
        jsonl(home, role, card, trace, "deploy.rolled_back", ",\"reason\":\"ts-verify-fail\"");
        return Err(format!("post-restart verify for {} failed; restored prev dist: {}", svc_name, e));
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
/// #3376 — role slots owed a refresh when a verb lands canonical. The #3101
/// wrapper routes CHORUS_ROLE to $WERK_<ROLE>_BIN/<bin> FIRST; without refresh
/// a role keeps running last week's verb after every land (the merged≠live
/// class one layer down — kade-bin/werk-demo predated #3352/#3365 live on
/// 2026-06-12 while canonical was current). Refresh every role slot holding
/// the binary EXCEPT a role with a live werk containing this crate's source —
/// that slot is an ACTIVE demo variant and is owned by the demo (#3101).
pub fn slots_to_refresh(werk_base: &str, crate_name: &str, bin: &str) -> Vec<String> {
    slots_to_refresh_with(werk_base, crate_name, bin, &werk_diff_touches_crate)
}

/// Pure-decidable core: `is_variant(werk_dir, crate)` answers "is this live werk
/// actually MODIFYING this crate?" — injected so tests can model full checkouts
/// honestly (the first cut checked crate-dir-EXISTS, which is true for EVERY
/// crate in EVERY werk since werks are full checkouts: the exception swallowed
/// the rule and the refresh was a production no-op — caught by cold-eyes before
/// land, 2026-06-12).
pub fn slots_to_refresh_with(
    werk_base: &str,
    crate_name: &str,
    bin: &str,
    is_variant: &dyn Fn(&str, &str) -> bool,
) -> Vec<String> {
    let mut out = Vec::new();
    for role in ["wren", "silas", "kade"] {
        let slot = Path::new(werk_base).join(format!("{}-bin", role)).join(bin);
        if !slot.is_file() {
            continue;
        }
        let has_live_variant = fs::read_dir(werk_base)
            .ok()
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| {
                        let n = e.file_name().to_string_lossy().to_string();
                        n.starts_with(&format!("{}-", role)) && !n.ends_with("-bin")
                    })
                    .any(|e| is_variant(&e.path().to_string_lossy(), crate_name))
            })
            .unwrap_or(false);
        if !has_live_variant {
            out.push(role.to_string());
        }
    }
    out
}

/// A werk is a VARIANT of a crate iff its branch diff vs origin/main touches
/// `platform/services/<crate>/` — modification, not presence. Unreadable werk /
/// git failure → NOT a variant (refresh proceeds): a true variant's own next
/// pipeline run re-installs its slot via target=werk anyway, so the cost of a
/// wrong refresh is one re-deploy; the cost of wrongly protecting is the exact
/// stale-slot class this card exists to kill.
pub fn werk_diff_touches_crate(werk_dir: &str, crate_name: &str) -> bool {
    let needle = format!("platform/services/{}/", crate_name);
    std::process::Command::new("git")
        .args(["-C", werk_dir, "diff", "--name-only", "origin/main...HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|out| out.lines().any(|l| l.trim().starts_with(&needle)))
        .unwrap_or(false)
}

/// Execute the #3376 refresh after a verified canonical install: copy the
/// canonical `-bin` content over each owed slot (atomic via tmp+rename), emit
/// slot.refreshed per slot. Best-effort per slot — a failed slot refresh is
/// LOUD on the witness but does not fail the deploy (canonical is the truth;
/// the wrapper's fall-through still serves it if a slot copy is lost).
fn refresh_role_slots(home: &Path, role: &str, card: u64, trace: &str, crate_name: &str, bin: &str, canonical_path: &str) {
    let werk_base = env::var("CHORUS_WERK_BASE").unwrap_or_else(|_| {
        format!("{}/CascadeProjects/chorus-werk", env::var("HOME").unwrap_or_default())
    });
    for slot_role in slots_to_refresh(&werk_base, crate_name, bin) {
        let slot = format!("{}/{}-bin/{}", werk_base, slot_role, bin);
        let tmp = format!("{}.refresh-tmp", slot);
        let ok = fs::copy(canonical_path, &tmp).is_ok()
            && run_env(None, &[], "chmod", &["+x", &tmp]).is_ok()
            && fs::rename(&tmp, &slot).is_ok();
        if ok {
            jsonl(home, role, card, trace, "slot.refreshed",
                  &format!(",\"bin\":\"{}\",\"slot_role\":\"{}\"", bin, slot_role));
        } else {
            let _ = fs::remove_file(&tmp);
            // A failed refresh must NOT leave the STALE slot executable — the
            // wrapper would keep routing to last week's semantics (silent-drift
            // reborn). Retire it aside so fall-through serves CANONICAL: degrade
            // to current, never to stale. (Cold-eyes probe 3, 2026-06-12.)
            if fs::rename(&slot, format!("{}.stale", slot)).is_ok() {
                jsonl(home, role, card, trace, "slot.retired_stale",
                      &format!(",\"bin\":\"{}\",\"slot_role\":\"{}\"", bin, slot_role));
            } else {
                jsonl(home, role, card, trace, "slot.refresh_failed",
                      &format!(",\"bin\":\"{}\",\"slot_role\":\"{}\"", bin, slot_role));
            }
        }
    }
}

/// Test seam for refresh_role_slots (env-driven werk base; fs side effects).
pub fn refresh_role_slots_for_test(home: &Path, role: &str, card: u64, trace: &str, crate_name: &str, bin: &str, canonical_path: &str) {
    refresh_role_slots(home, role, card, trace, crate_name, bin, canonical_path)
}

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
        &chorus_bin_install_cmd(home, werk_s),
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
            return Err(died(home, role, card, trace, "wrapper-write",
                format!("deploy_cli_verb: write wrapper {}: {}", wrapper_path, e)));
        }
        // chmod +x via std (Unix permissions; mode 0o755).
        let _ = run_env(None, &[], "chmod", &["+x", &wrapper_path]);
        jsonl(home, role, card, trace, "wrapper-installed", &format!(",\"name\":\"{}\",\"wrapper\":\"{}\",\"bin\":\"{}\"", crate_name, wrapper_path, bin_path));

        // #3376 — canonical landed: refresh every role slot that holds this verb
        // (except live demo variants). Without this, the wrapper above keeps
        // routing roles to whatever sha their slot last saw — the merged≠live
        // class one layer down, proven live 2026-06-12.
        refresh_role_slots(home, role, card, trace, crate_name, bin, &bin_path);
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
        return Err(died(home, role, card, trace, "dist-not-found",
            format!("shared-lib deploy: werk dist not found at {} (werk-build should have produced it)", werk_lib_dist)));
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
        jsonl(home, role, card, trace, "deploy.rolled_back", ",\"reason\":\"sharedlib-install-fail\"");
        return Err(format!("install of {} dist failed; rolled back: {}", lib, e));
    }
    // identity verify: installed lib dist == built H.
    let installed_lib_sha = dist_sha(&canonical_lib_dist).unwrap_or_default();
    if installed_lib_sha != built_identity {
        restore(&moved);
        jsonl(home, role, card, trace, "deploy.rolled_back", ",\"reason\":\"sharedlib-identity-mismatch\"");
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
            jsonl(home, role, card, trace, "deploy.rolled_back", ",\"reason\":\"consumer-dist-missing\"");
            return Err(format!("consumer {} werk dist not found at {} (werk-build cascade should have produced it)", c.name, werk_consumer_dist));
        }
        if let Err(e) = install_dist(&werk_consumer_dist, &canonical_consumer_dist, &mut moved) {
            restore(&moved);
            jsonl(home, role, card, trace, "deploy.rolled_back", ",\"reason\":\"consumer-install-fail\"");
            return Err(format!("install of consumer {} dist failed; rolled back: {}", c.name, e));
        }
        jsonl(home, role, card, trace, "installed", &format!(",\"name\":\"{}\",\"kind\":\"consumer\",\"target\":\"{}\"", c.name, target));
    }

    // 3. AC4 — anti-stale verify: each consumer must RESOLVE the merged lib identity.
    for c in &consumers {
        let resolved_sdk_dist = format!("{}/{}/node_modules/{}/dist", canonical_root, c.dir_rel, lib);
        if !Path::new(&resolved_sdk_dist).exists() {
            restore(&moved);
            jsonl(home, role, card, trace, "deploy.rolled_back", ",\"reason\":\"consumer-cannot-resolve-lib\"");
            return Err(format!(
                "anti-stale: consumer {} cannot resolve {} (no {}); it would run a missing/old lib — rolled back",
                c.name, lib, resolved_sdk_dist
            ));
        }
        let resolved_sha = dist_sha(&resolved_sdk_dist).unwrap_or_default();
        if resolved_sha != built_identity {
            restore(&moved);
            jsonl(home, role, card, trace, "deploy.rolled_back", ",\"reason\":\"consumer-resolves-stale-lib\"");
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

/// #3132 — launchd LIVENESS floor: after a kickstart, poll `launchctl print
/// gui/<uid>/<svc>` until the job reports a running pid, or fail. This is the
/// universal "did the restart take" check for a service that did NOT self-declare a
/// health URL — it proves the process came up and isn't spawn-throttled/crash-looping,
/// without claiming it serves correctly (that's what a health URL would add). Weaker
/// than smoke, but it never blocks a deploy for a missing declaration.
fn wait_for_liveness(svc: &str, timeout: Duration) -> R<()> {
    let label = format!("gui/{}/{}", uid(), svc);
    let start = Instant::now();
    let mut last = String::new();
    loop {
        if let Ok(out) = run_env(None, &[], "launchctl", &["print", &label]) {
            // A live job prints `state = running` and a `pid = <n>`. A crash-looping
            // one prints `state = spawn scheduled`/`waiting` with no pid, or a
            // nonzero `last exit code`. Require a pid + running to pass.
            let running = out.contains("state = running");
            let has_pid = out.lines().any(|l| l.trim_start().starts_with("pid ="));
            if running && has_pid {
                return Ok(());
            }
            last = out;
        }
        if start.elapsed() >= timeout {
            let tail: String = last.lines().take(6).collect::<Vec<_>>().join(" | ");
            return Err(format!("{} not running within {:?} (launchctl: {})", svc, timeout, tail));
        }
        sleep(Duration::from_millis(500));
    }
}

/// #3132 — deploy a plain TS PACKAGE (build script, no LaunchAgent, not a shared lib):
/// a CLI/tool like `cards` or `clearing`. Copy its dist to canonical (preserve .prev),
/// identity-verify installed == built, NO kickstart (nothing to restart). Hash-gated:
/// an unchanged package is skipped (no copy). This closes the stale gap for CLIs that
/// changed on their own (not via a shared-lib cascade) — they were previously
/// unreachable by the deploy verb and silently ran old code.
#[allow(clippy::too_many_arguments)]
fn deploy_ts_package(
    home: &Path, werk_s: &str, role: &str, card: u64, target: &str, trace: &str,
    name: &str, built_dist_sha: &str, dist_dir_rel: &str,
) -> R<()> {
    if target == "werk" {
        // The role's session runs the package from its own werk dist (already built).
        jsonl(home, role, card, trace, "deploy.tspackage.werk-noop", &format!(",\"name\":\"{}\"", name));
        return Ok(());
    }
    let canonical_root = canonical_root_path(home);
    let canonical_dist = format!("{}/{}", canonical_root, dist_dir_rel);
    let werk_dist = format!("{}/{}", werk_s, dist_dir_rel);
    if !Path::new(&werk_dist).is_dir() {
        return Err(died(home, role, card, trace, "dist-not-found",
            format!("TS package deploy: werk dist not found at {} (werk-build should have produced it)", werk_dist)));
    }
    // HASH-GATE: unchanged → skip the copy.
    if let Ok(live_sha) = dist_sha(&canonical_dist) {
        if live_sha == built_dist_sha {
            jsonl(home, role, card, trace, "deploy.skipped",
                &format!(",\"name\":\"{}\",\"kind\":\"ts-package\",\"reason\":\"unchanged\",\"identity\":\"{}\"", name, built_dist_sha));
            return Ok(());
        }
    }
    let mut moved: Vec<(String, String)> = Vec::new();
    if let Err(e) = install_dist(&werk_dist, &canonical_dist, &mut moved) {
        for (canonical, prev) in moved.iter().rev() {
            let _ = fs::remove_dir_all(canonical);
            if Path::new(prev).is_dir() { let _ = fs::rename(prev, canonical); }
        }
        jsonl(home, role, card, trace, "deploy.rolled_back", ",\"reason\":\"tspackage-install-fail\"");
        return Err(format!("install of {} dist failed; rolled back: {}", name, e));
    }
    let installed_sha = dist_sha(&canonical_dist).unwrap_or_default();
    if installed_sha != built_dist_sha {
        for (canonical, prev) in moved.iter().rev() {
            let _ = fs::remove_dir_all(canonical);
            if Path::new(prev).is_dir() { let _ = fs::rename(prev, canonical); }
        }
        jsonl(home, role, card, trace, "deploy.rolled_back", ",\"reason\":\"tspackage-identity-mismatch\"");
        return Err(format!("installed {} dist sha != built: built={} installed={} — rolled back", name, built_dist_sha, installed_sha));
    }
    jsonl(home, role, card, trace, "installed", &format!(",\"name\":\"{}\",\"kind\":\"ts-package\",\"target\":\"{}\",\"identity\":\"{}\"", name, target, installed_sha));
    Ok(())
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
    jsonl(home, role, card, trace, "deploy.rolled_back", &format!(",\"reason\":\"{}\",\"target\":\"{}\"", reason, target));
    let _ = run_env(Some(werk_s), &[("CHORUS_ROLE", role)], &chorus_bin_install_cmd(home, werk_s), &["--target", target, "--rollback", bin]);
    if target == "canonical" {
        let svc = service_for_crate(bin);
        let _ = run_env(None, &[], "launchctl", &["kickstart", "-k", &format!("gui/{}/{}", uid(), svc)]);
    }
}

#[cfg(test)]
mod running_verdict_tests {
    // #3528 — the running-proof verdict logic (#3517 inc2, the 2026-06-04 stale-daemon
    // catcher) was PURE-by-design but never unit-tested — its only coverage was the
    // macOS-launchd e2e tests, which can't run on the Linux CI runner (BSD date -v/-j).
    // These pure tests exercise all 5 branches cross-platform (no shell), so the safety
    // logic stays covered on every CI push; the e2e tests verify the macOS shelling
    // where the runtime actually lives.
    use super::{running_verdict, RunVerdict};

    #[test]
    fn one_shot_cli_is_ok_without_a_live_process() {
        // class_is_daemon=false → install-verify already gated it, no running process to prove.
        assert_eq!(running_verdict(false, "AAA", "AAA", None), RunVerdict::Ok);
        assert_eq!(running_verdict(false, "AAA", "BBB", Some(false)), RunVerdict::Ok);
    }

    #[test]
    fn installed_not_equal_built_is_mismatch_regardless_of_process() {
        // a broken install reds before the process state even matters.
        assert_eq!(running_verdict(true, "BUILT", "STALE_INSTALL", Some(true)), RunVerdict::Mismatch);
        assert_eq!(running_verdict(true, "BUILT", "STALE_INSTALL", None), RunVerdict::Mismatch);
    }

    #[test]
    fn daemon_restarted_onto_built_is_ok() {
        assert_eq!(running_verdict(true, "SAME", "SAME", Some(true)), RunVerdict::Ok);
    }

    #[test]
    fn daemon_did_not_restart_is_stale_06_04() {
        // installed==built but the process never restarted onto it → old inode → Stale.
        assert_eq!(running_verdict(true, "SAME", "SAME", Some(false)), RunVerdict::Stale);
    }

    #[test]
    fn unresolvable_pid_is_unknown_red() {
        // PID/start-time unresolvable → Unknown (the contract's unknown=RED; never silent-pass).
        assert_eq!(running_verdict(true, "SAME", "SAME", None), RunVerdict::Unknown);
    }
}
