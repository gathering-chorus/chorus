//! werk-build — `/werk/build` v2 logic (card #3061).
//!
//! Self-contained: std only; calls `git`, `gh`, and `build-signed.sh` as
//! subprocesses. No dependency on any other chorus code (ADR-032 §1 blueprint;
//! mirrors werk-pull #3045).
//!
//! Recompile + sign a card's crate IN THE WERK and emit its cdhash.
//! - NO install, no system mutation, no rollback — build is non-mutating
//!   (BUILD_SKIP_INSTALL=1). All install/slot logic lives in werk-deploy (#3062);
//!   build is slot-agnostic (the cdhash is the contract between them).
//! - Build-invariance: same source commit -> same cdhash (the deploy/verify gate).
//! - Refuses if the `<role>/<card>` werk/branch doesn't exist; never builds canonical.
//! - JSONL witness per step: best-effort, NEVER affects the operation.

use std::collections::BTreeSet;
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
const LOCK_EX_NB: i32 = 0x02 | 0x04; // LOCK_EX | LOCK_NB
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

/// Shared trace per ADR-032 §3: CHORUS_TRACE_ID env -> /tmp/<card>-trace file ->
/// mint-and-persist (write the file so downstream verbs inherit). The file is the
/// cross-process carrier (verbs are separate processes). Unlike werk-pull's
/// fresh-mint (its #3063 drift), this threads one trace across pull->...->accept.
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

pub fn jsonl_line(ts: u128, event: &str, role: &str, card: u64, trace: &str, extra: &str) -> String {
    format!(
        "{{\"ts\":{},\"event\":\"{}\",\"role\":\"{}\",\"card_id\":{},\"trace_id\":\"{}\"{}}}\n",
        ts, event, role, card, trace, extra
    )
}

/// #3092/#3126 — what kind of thing was changed, and what's its identifier. A Rust
/// crate (build-signed.sh, cdhash), a TS service (`npm run build`, sha256-of-dist),
/// or a shared library (`npm run build` + cascade-rebuild every graph-discovered
/// consumer, identity = sha256-of-dist). All flow through the same `<name>=<identity>`
/// summary contract that werk-deploy parses (ADR-032 §1, §5 amended: cdhash and
/// sha256(dist) are both "stable identity hashes").
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum BuildUnit {
    /// Shared library at platform/<name>/ (chorus-sdk) — `npm run build` its dist,
    /// then cascade a rebuild of every consumer that imports it (#3126). Sorts FIRST
    /// so it builds before consumers (a consumer's tsc needs the lib's fresh .d.ts).
    SharedLib(String),
    /// Rust crate at platform/services/<name>/ — cargo+codesign+cdhash.
    RustCrate(String),
    /// TS service at platform/api/ etc. — `npm run build` in service dir, hash dist.
    TsService(String),
}

impl BuildUnit {
    /// Stable display name used in the summary (`<name>=<identity>`).
    pub fn name(&self) -> &str {
        match self {
            BuildUnit::SharedLib(n) | BuildUnit::RustCrate(n) | BuildUnit::TsService(n) => {
                n.as_str()
            }
        }
    }
}

/// #3132 — recursively collect paths named `filename` under `root`, skipping
/// node_modules / .git / target (build + VCS noise). std-only walk so it's
/// testable against a temp dir without a git repo. Sorted for stable output.
fn find_files(root: &Path, filename: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name();
            let name = name.to_string_lossy();
            if p.is_dir() {
                if name == "node_modules" || name == ".git" || name == "target" {
                    continue;
                }
                stack.push(p);
            } else if name == filename {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// #3132 — does this package.json declare a `build` script? std-only, shape-tolerant:
/// a `build` token appearing after the `scripts` token. (Sufficient for this repo's
/// package.json layout, where the scripts object precedes deps; a dep literally named
/// "build" would be a false positive, but none exists and a global build is harmless
/// anyway — worst case is building a unit with no build step, which `npm run build`
/// itself would reject loudly.)
pub fn has_build_script(pkg_json: &str) -> bool {
    let tokens = quoted_tokens(pkg_json);
    let Some(scripts_at) = tokens.iter().position(|t| t == "scripts") else { return false };
    tokens[scripts_at + 1..].iter().any(|t| t == "build")
}

/// #3132 — STRUCTURAL enumeration: every buildable unit in the werk, discovered by
/// what it IS (a Rust crate / a TS package with a build script), NOT by what a diff
/// touched. This is the fix for the merged-but-stale class (#3126/#3130/#3133): the
/// old `discover_build_units(diff)` only built units whose path matched a hardcoded
/// rule (`ts_service_for_path` / `shared_lib_for_path`), so a service not on the list
/// built NOTHING → merged green → ran stale. There is no list here: if it's a crate or
/// a build-script package, it's a unit. Building everything is cheap — the toolchains
/// no-op unchanged units (~10s TS, 0.07s cargo for an unchanged crate set) — and
/// werk-deploy gates the actual copy+restart on hash-difference, so unchanged units
/// are never bounced. Build-all (nothing missed) + restart-only-what-differs.
///
/// SharedLib vs TsService is itself structural: a TS package that is the target of a
/// `file:` dependency from another package IS a library (its consumers must be
/// cascade-rebuilt on deploy); one that isn't is a service/CLI.
pub fn discover_build_units_in_tree(werk_root: &Path) -> Vec<BuildUnit> {
    let mut units: BTreeSet<BuildUnit> = BTreeSet::new();

    // Rust crates: platform/services/<name>/Cargo.toml.
    let services = werk_root.join("platform/services");
    if let Ok(entries) = fs::read_dir(&services) {
        for e in entries.flatten() {
            if e.path().join("Cargo.toml").is_file() {
                if let Some(n) = e.file_name().to_str() {
                    units.insert(BuildUnit::RustCrate(n.to_string()));
                }
            }
        }
    }

    // Shared-lib dirs = canonicalized targets of every `file:` dep in any tracked
    // package.json. Discovered from the dependency graph, never hardcoded.
    let pkg_files = find_files(werk_root, "package.json");
    let mut sharedlib_dirs: BTreeSet<PathBuf> = BTreeSet::new();
    for pj in &pkg_files {
        let Ok(content) = fs::read_to_string(pj) else { continue };
        let pkg_dir = pj.parent().unwrap_or(werk_root);
        for (_dep, target) in extract_file_deps(&content) {
            if let Ok(canon) = fs::canonicalize(pkg_dir.join(&target)) {
                sharedlib_dirs.insert(canon);
            }
        }
    }

    // TS packages with a build script → SharedLib (a file:-dep target) or TsService.
    for pj in &pkg_files {
        let Ok(content) = fs::read_to_string(pj) else { continue };
        if !has_build_script(&content) {
            continue;
        }
        let Some(name) = pkg_name(&content) else { continue };
        let pkg_dir = pj.parent().unwrap_or(werk_root);
        let is_lib = fs::canonicalize(pkg_dir)
            .map(|c| sharedlib_dirs.contains(&c))
            .unwrap_or(false);
        if is_lib {
            units.insert(BuildUnit::SharedLib(name));
        } else {
            units.insert(BuildUnit::TsService(name));
        }
    }

    units.into_iter().collect()
}

/// #3126 — pure: extract `(dep_name, file_target)` for every `"x": "file:<path>"`
/// dependency in a package.json. This is the primitive of graph-driven consumer
/// discovery: a consumer of a shared lib declares it as a `file:` dep, so scanning
/// every package.json for file-deps whose target resolves to the lib yields the
/// bundler set WITHOUT a hardcoded list that rots. JSON-shape tolerant (no serde:
/// std-only per ADR-032 §1) — keys on the literal `"file:` value prefix.
pub fn extract_file_deps(pkg_json: &str) -> Vec<(String, String)> {
    // Tokenize into ordered double-quoted strings — robust against nesting/newlines
    // (the comma/colon-split approach broke on the `dependencies: { ... }` object).
    // A file dep is the token PAIR ["<name>", "file:<target>"]: in JSON the value
    // string immediately follows its key string, so tokens[i-1] is the dep name.
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

/// Collect every double-quoted string token, in order. std-only JSON-lite scan.
fn quoted_tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_str = false;
    let mut cur = String::new();
    let mut prev = '\0';
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

/// #3126 — pure: read the `"name"` field of a package.json (the consumer's deploy
/// identity in the build summary). std-only, shape-tolerant.
pub fn pkg_name(pkg_json: &str) -> Option<String> {
    let tokens = quoted_tokens(pkg_json);
    // the top-level "name" key's value is the token immediately after the first
    // "name" token. (Sufficient: package.json's top-level name precedes any nested
    // "name" in deps, which are version-range values not bare "name" keys.)
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

// --- side-effecting helpers ---

fn jsonl(home: &Path, role: &str, card: u64, trace: &str, event: &str, extra: &str) {
    let p = home.join("ops/logs/werk-build.jsonl");
    if let Some(d) = p.parent() {
        let _ = fs::create_dir_all(d);
    }
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let line = jsonl_line(ts, event, role, card, trace, extra);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&p) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Run a CLI, capture stdout; any non-zero exit is a typed error.
fn run(cmd: &str, args: &[&str]) -> R<String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("{} failed to start: {}", cmd, e))?;
    if !out.status.success() {
        return Err(format!("{} {}: {}", cmd, args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Like `run`, with an explicit working dir + extra env (for BUILD_SKIP_INSTALL
/// and so `gh`/`build-signed.sh` resolve the repo + crate from the werk).
fn run_in_env(dir: &str, envs: &[(&str, &str)], cmd: &str, args: &[&str]) -> R<String> {
    let mut c = Command::new(cmd);
    c.args(args).current_dir(dir);
    for (k, v) in envs {
        c.env(k, v);
    }
    let out = c.output().map_err(|e| format!("{} failed to start: {}", cmd, e))?;
    if !out.status.success() {
        return Err(format!(
            "{} {}: {}",
            cmd,
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    // build-signed.sh prints cdhash on stdout; callers may need stderr too.
    Ok(format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    ))
}

/// flock guard — auto-releases on drop (and on process exit/crash, kernel-level).
pub struct FlockGuard(std::fs::File);
impl Drop for FlockGuard {
    fn drop(&mut self) {
        unsafe { flock(self.0.as_raw_fd(), LOCK_UN) };
    }
}

/// Lock the WERK's build (not canonical) — two concurrent cargo builds on one
/// target dir race. Build never touches canonical, so the lock lives in the werk.
pub fn lock(werk: &Path, timeout: Duration) -> R<FlockGuard> {
    let p = werk.join(".git-build.lock");
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
            return Err("another build holds the werk lock (timed out after 60s)".to_string());
        }
        sleep(Duration::from_millis(100));
    }
}

fn path(p: &Path) -> R<&str> {
    p.to_str().ok_or_else(|| format!("non-utf8 path: {}", p.display()))
}

/// Extract the cdhash from build-signed.sh output (`build-signed: cdhash=<hash>`).
pub fn extract_cdhash(output: &str) -> Option<String> {
    for line in output.lines() {
        if let Some(rest) = line.trim().strip_prefix("build-signed: cdhash=") {
            let h = rest.trim();
            if !h.is_empty() {
                return Some(h.to_string());
            }
        }
    }
    None
}

// #3092 — `detect_crates` (Rust-only diff scan) retired; replaced by the
// pure `discover_build_units` helper above + a direct git-diff call in `build`.
// One diff query, dispatched through the BuildUnit enum.

/// (identifier, binary-name) for a crate. Mirrors build-signed.sh `resolve_crate`
/// for the signed crates; derives a sensible default otherwise. Needed because the
/// 1-arg `build-signed.sh <crate>` form resolves crate_dir against build-signed.sh's
/// OWN ($ROOT) canonical root — so it builds CANONICAL, not the werk. werk-build must
/// use the 3-arg `<crate-dir> <identifier> <binary>` form with the WERK's crate dir,
/// or "build the card's changes before merge" is defeated (it'd build unchanged main).
pub fn crate_spec(crate_name: &str) -> (String, String) {
    match crate_name {
        "chorus-hooks" => ("com.chorus.hook-shim".to_string(), "chorus-hook-shim".to_string()),
        "chorus-inject" => ("com.chorus.inject".to_string(), "chorus-inject".to_string()),
        other => {
            // convention: binary == crate name; identifier == com.chorus.<name w/o chorus- prefix>.
            let suffix = other.strip_prefix("chorus-").unwrap_or(other);
            (format!("com.chorus.{}", suffix), other.to_string())
        }
    }
}

/// Build + sign ONE crate IN THE WERK, build-only, return its cdhash.
/// 3-arg build-signed.sh form against the werk's crate dir (not $ROOT/canonical);
/// BUILD_SKIP_INSTALL=1 (the flag exists per #2774) — no install, no mutation.
fn build_crate(werk_s: &str, crate_name: &str) -> R<String> {
    let crate_dir = format!("{}/platform/services/{}", werk_s, crate_name);
    let (identifier, binary) = crate_spec(crate_name);
    let out = run_in_env(
        werk_s,
        &[("BUILD_SKIP_INSTALL", "1")],
        "build-signed.sh",
        &[&crate_dir, &identifier, &binary],
    )?;
    extract_cdhash(&out)
        .ok_or_else(|| format!("build of {} produced no cdhash (build-signed.sh output had no 'cdhash=' line)", crate_name))
}

/// #3132 — resolve a TS package's source dir inside the werk by STRUCTURE: find the
/// package.json whose `"name"` matches. No hardcoded name→dir map (the executor-side
/// twin of the old discovery allowlist — the live run of #3132 caught that fixing only
/// discovery left this stale). Errs only if the named package can't be found at all.
fn pkg_dir_for_name(werk_s: &str, name: &str) -> R<String> {
    let root = Path::new(werk_s);
    for pj in find_files(root, "package.json") {
        if let Ok(content) = fs::read_to_string(&pj) {
            if pkg_name(&content).as_deref() == Some(name) {
                let dir = pj.parent().unwrap_or(root);
                return path(dir).map(|s| s.to_string());
            }
        }
    }
    Err(format!("TS package '{}' not found in werk (no package.json with that name)", name))
}

fn ts_service_dir(werk_s: &str, service: &str) -> R<String> {
    pkg_dir_for_name(werk_s, service)
}

/// #3092 — build ONE TS service IN THE WERK, return its identity hash
/// (sha256 of the dist tree). Determinism PROVEN externally (2026-05-26: three
/// back-to-back tsc builds → byte-identical sha) so same source → same hash,
/// build-invariance holds the same way cdhash does for Rust crates.
///
/// Mirrors v1 chorus-deploy.sh's chorus-api branch (`npm run build`) — same
/// command, different caller. The hash uses `find | sort | shasum | shasum`
/// (the same scheme the determinism check used), keying on file CONTENT and
/// file PATHS so a rename or addition shifts the hash.
fn build_ts_service(werk_s: &str, service: &str) -> R<String> {
    let service_dir = ts_service_dir(werk_s, service)?;
    if !Path::new(&service_dir).is_dir() {
        return Err(format!("TS service dir not found at {}", service_dir));
    }
    // npm run build in the service dir (mirrors v1 chorus-deploy.sh #2831 path).
    run_in_env(&service_dir, &[], "npm", &["run", "build"])?;
    let dist_dir = format!("{}/dist", service_dir);
    if !Path::new(&dist_dir).is_dir() {
        return Err(format!("expected dist/ after `npm run build` at {}", dist_dir));
    }
    // sha256 of dist tree, RELATIVE PATHS so the hash is location-independent:
    // werk-build hashes the werk's dist; werk-deploy hashes the canonical dist
    // after cp. Both MUST produce the same hash for byte-identical content even
    // though the dirs sit at different absolute paths. (Maiden-voyage bug #3092
    // 2026-05-26: `find $abs_dir -type f` bakes the absolute path into each
    // shasum line, so faithfully-identical content at different paths produced
    // different outer hashes. Use `cd $dir && find . -type f` so the shasum
    // lines all start with "./relpath" — location-independent.)
    let cmd = format!(
        "cd {} && find . -type f | LC_ALL=C sort | xargs shasum -a 256 | shasum -a 256 | cut -d' ' -f1",
        dist_dir
    );
    let out = run("sh", &["-c", &cmd])?;
    let sha = out.trim();
    if sha.is_empty() || sha.len() < 32 {
        return Err(format!("dist hash invalid ({:?}) after build of {}", sha, service));
    }
    Ok(sha.to_string())
}

/// #3126 — sha256 of a dist tree, RELATIVE PATHS so the hash is location-independent
/// (werk-build hashes the werk's dist; werk-deploy hashes canonical's after cp —
/// both MUST agree for byte-identical content at different absolute paths). Same
/// scheme as `build_ts_service` and werk-deploy's identity verify.
fn dist_sha(dist_dir: &str) -> R<String> {
    if !Path::new(dist_dir).is_dir() {
        return Err(format!("dist not found at {}", dist_dir));
    }
    let cmd = format!(
        "cd {} && find . -type f | LC_ALL=C sort | xargs shasum -a 256 | shasum -a 256 | cut -d' ' -f1",
        dist_dir
    );
    let out = run("sh", &["-c", &cmd])?;
    let sha = out.trim();
    if sha.is_empty() || sha.len() < 32 {
        return Err(format!("dist hash invalid ({:?}) for {}", sha, dist_dir));
    }
    Ok(sha.to_string())
}

/// #3132 — source dir of a shared lib inside the werk, resolved structurally by
/// package name (mirrors ts_service_dir; no hardcoded chorus-sdk rule).
fn shared_lib_dir(werk_s: &str, lib: &str) -> R<String> {
    pkg_dir_for_name(werk_s, lib)
}

/// A graph-discovered consumer of a shared lib: its package `name` (deploy identity)
/// and its directory relative to the repo root (where dist lives, where to build).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Consumer {
    pub name: String,
    pub dir_rel: String,
}

/// #3126 — DISCOVER every consumer of `lib` from the dependency graph: walk all
/// package.json files in the werk (git-tracked, so node_modules is excluded), and
/// for each that declares a `file:` dep resolving to the lib's directory, record it.
/// The bundler set is therefore DISCOVERED, never a hardcoded list that rots — the
/// exact failure #3092 left open. Returns consumers sorted by name (stable summary).
pub fn discover_consumers(werk_s: &str, lib: &str) -> R<Vec<Consumer>> {
    let lib_dir = shared_lib_dir(werk_s, lib)?;
    let lib_canon = fs::canonicalize(&lib_dir)
        .map_err(|e| format!("cannot canonicalize lib dir {}: {}", lib_dir, e))?;
    // git-tracked package.json files only (excludes node_modules, which is untracked).
    let listing = run("git", &["-C", werk_s, "ls-files", "*package.json", "**/package.json"])?;
    let mut consumers: BTreeSet<(String, String)> = BTreeSet::new();
    for rel in listing.lines() {
        let rel = rel.trim();
        if rel.is_empty() {
            continue;
        }
        let abs = format!("{}/{}", werk_s, rel);
        let Ok(content) = fs::read_to_string(&abs) else { continue };
        // skip the lib's own package.json.
        let pkg_dir = Path::new(&abs).parent().map(|p| p.to_path_buf());
        for (_dep, target) in extract_file_deps(&content) {
            let Some(ref dir) = pkg_dir else { continue };
            let resolved = dir.join(&target);
            if let Ok(rc) = fs::canonicalize(&resolved) {
                if rc == lib_canon {
                    let name = pkg_name(&content).unwrap_or_else(|| {
                        // fall back to the dir name if "name" is absent.
                        dir.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
                    });
                    let dir_rel = Path::new(rel).parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if !name.is_empty() && !dir_rel.is_empty() {
                        consumers.insert((name, dir_rel));
                    }
                }
            }
        }
    }
    Ok(consumers.into_iter().map(|(name, dir_rel)| Consumer { name, dir_rel }).collect())
}

/// #3126 — build ONE shared lib IN THE WERK and CASCADE-rebuild its consumers.
/// (1) `npm run build` the lib → its dist; identity = sha256(dist).
/// (2) DISCOVER consumers from the graph; for each, `npm install` (refresh the
///     `file:` link so a bundler/linker sees the new dist) + `npm run build`
///     (re-bundle/re-transpile against it). A consumer that can't build against the
///     new lib is a real breaking change — it fails the build LOUDLY (the whole
///     point: no silent stale prod).
/// Returns the LIB's identity hash — the contract werk-deploy keys the anti-stale
/// verify on. Consumer dists are produced here (in the werk) for werk-deploy to copy.
fn build_shared_lib(home: &Path, role: &str, card: u64, trace: &str, werk_s: &str, lib: &str) -> R<String> {
    let lib_dir = shared_lib_dir(werk_s, lib)?;
    if !Path::new(&lib_dir).is_dir() {
        return Err(format!("shared lib dir not found at {}", lib_dir));
    }
    run_in_env(&lib_dir, &[], "npm", &["run", "build"])?;
    let lib_dist = format!("{}/dist", lib_dir);
    let identity = dist_sha(&lib_dist)?;
    jsonl(home, role, card, trace, "sharedlib.built", &format!(",\"lib\":\"{}\",\"identity\":\"{}\"", lib, identity));

    let consumers = discover_consumers(werk_s, lib)?;
    jsonl(
        home, role, card, trace, "sharedlib.consumers.discovered",
        &format!(",\"lib\":\"{}\",\"count\":{},\"consumers\":\"{}\"", lib,
            consumers.len(),
            consumers.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join("|")),
    );
    if consumers.is_empty() {
        // A shared lib with zero consumers is suspicious (why is it shared?) but not
        // an error — log it loudly so a real "consumers vanished from the graph"
        // regression is visible rather than silent.
        jsonl(home, role, card, trace, "sharedlib.consumers.none", &format!(",\"lib\":\"{}\"", lib));
    }
    for c in &consumers {
        let cdir = format!("{}/{}", werk_s, c.dir_rel);
        jsonl(home, role, card, trace, "consumer.rebuild.started", &format!(",\"name\":\"{}\",\"dir\":\"{}\"", c.name, c.dir_rel));
        // refresh the file: link, then rebuild against the new lib dist.
        run_in_env(&cdir, &[], "npm", &["install", "--no-audit", "--no-fund"])
            .map_err(|e| format!("consumer {} npm install failed (cascade): {}", c.name, e))?;
        run_in_env(&cdir, &[], "npm", &["run", "build"])
            .map_err(|e| format!("consumer {} rebuild failed against new {} — breaking change, refusing: {}", c.name, lib, e))?;
        jsonl(home, role, card, trace, "consumer.rebuild.completed", &format!(",\"name\":\"{}\"", c.name));
    }
    Ok(identity)
}

/// gh: set chorus/build/<card> success on the werk HEAD carrying the cdhash, and
/// carry prior chorus/*/<card> statuses forward onto this SHA (ADR-032 §5).
/// Best-effort: gh failure does not fail a successful build (build is non-mutating;
/// the cdhash is already produced — gh is observability of the process state).
fn register_gh(werk_s: &str, card: u64, role: &str, trace: &str, cdhash: &str) {
    let sha = match run("git", &["-C", werk_s, "rev-parse", "HEAD"]) {
        Ok(s) => s.trim().to_string(),
        Err(_) => return,
    };
    let endpoint = format!("repos/{{owner}}/{{repo}}/statuses/{}", sha);
    let desc = format!("role={} trace={} cdhash={} status=built", role, trace, cdhash);
    let _ = run_in_env(
        werk_s,
        &[],
        "gh",
        &[
            "api",
            &endpoint,
            "-f",
            "state=success",
            "-f",
            &format!("context=chorus/build/{}", card),
            "-f",
            &format!("description={}", desc),
        ],
    );
    // carry-forward: re-apply any chorus/*/<card> statuses from origin/main's HEAD
    // (where pull set its status pre-commit) onto this new SHA, so the whole chain
    // reads on one commit. Best-effort, never fatal.
    carry_forward(werk_s, card, &sha);
}

/// Re-apply prior chorus/*/<card> commit-statuses onto `sha` (ADR-032 §5).
fn carry_forward(werk_s: &str, card: u64, sha: &str) {
    let base = match run("git", &["-C", werk_s, "rev-parse", "origin/main"]) {
        Ok(s) => s.trim().to_string(),
        Err(_) => return,
    };
    if base == sha {
        return;
    }
    let listing = run_in_env(
        werk_s,
        &[],
        "gh",
        &["api", &format!("repos/{{owner}}/{{repo}}/commits/{}/statuses", base)],
    )
    .unwrap_or_default();
    let needle = format!("/{}", card);
    // crude scan for our card's chorus/* contexts already present on base.
    for ctx_line in listing.split('"') {
        if ctx_line.starts_with("chorus/") && ctx_line.ends_with(&needle) && ctx_line != format!("chorus/build/{}", card) {
            let endpoint = format!("repos/{{owner}}/{{repo}}/statuses/{}", sha);
            let _ = run_in_env(
                werk_s,
                &[],
                "gh",
                &[
                    "api",
                    &endpoint,
                    "-f",
                    "state=success",
                    "-f",
                    &format!("context={}", ctx_line),
                    "-f",
                    "description=carried-forward",
                ],
            );
        }
    }
}

/// Entry: parse `werk-build <card> <role>` (role falls back to $DEPLOY_ROLE).
pub fn run_build() -> R<String> {
    let card_arg = env::args().nth(1).ok_or_else(|| "usage: werk-build <card> <role>".to_string())?;
    let card: u64 = card_arg.parse().map_err(|_| format!("card id is not a number: {}", card_arg))?;
    let role = env::args()
        .nth(2)
        .or_else(|| env::var("DEPLOY_ROLE").ok())
        .ok_or_else(|| "usage: werk-build <card> <role> (or set DEPLOY_ROLE)".to_string())?;
    let werk_base =
        PathBuf::from(env::var("CHORUS_WERK_BASE").map_err(|_| "CHORUS_WERK_BASE not set".to_string())?);
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);
    build(card, &role, &home, &werk_base)
}

/// The whole verb, all inputs explicit so it is testable against a real temp repo
/// (deps injected via PATH: real `git`, shimmed `build-signed.sh`/`gh`).
/// Returns a comma-joined `crate=cdhash` summary.
pub fn build(card: u64, role: &str, home: &Path, werk_base: &Path) -> R<String> {
    let trace = resolve_trace(card);
    let branch = branch_name(role, card);
    let werk = werk_base.join(format!("{}-{}", role, card));
    let werk_s = path(&werk)?.to_string();

    jsonl(home, role, card, &trace, "build.started", "");

    // no-werk-refuse guard (ADR-032 §4): never build canonical.
    if !werk.is_dir() {
        jsonl(home, role, card, &trace, "build.refused", ",\"reason\":\"no-werk\"");
        return Err(format!("no werk at {} — pull #{} first (build never touches canonical)", werk.display(), card));
    }
    let cur = run("git", &["-C", &werk_s, "rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
    if cur.trim() != branch {
        jsonl(home, role, card, &trace, "build.refused", ",\"reason\":\"branch-mismatch\"");
        return Err(format!("werk {} is on '{}', not '{}'", werk.display(), cur.trim(), branch));
    }

    // #3132 — STRUCTURAL enumeration: build EVERY unit in the werk (Rust crate + TS
    // package with a build script), NOT a diff-filtered subset. The diff-driven
    // `discover_build_units` was a second incrementality layer on top of the one
    // cargo/tsc already do natively — it saved ~nothing and was the source of
    // merged-but-stale prod (a unit not matching a hardcoded path rule built NOTHING).
    // Building everything is cheap (toolchains no-op unchanged units) and can't miss a
    // unit; werk-deploy gates the actual copy+restart on hash-difference so nothing
    // unchanged is bounced. Build-all + restart-only-what-differs.
    let units = discover_build_units_in_tree(&werk);
    if units.is_empty() {
        // Degenerate werk with no buildable units at all (no crates, no build-script
        // packages). Keep the clean no-op so the demo/acp chain proceeds (#3107).
        jsonl(home, role, card, &trace, "build.skipped", ",\"reason\":\"no-build-units\"");
        jsonl(home, role, card, &trace, "build.completed", ",\"built\":\"\"");
        return Ok(String::new());
    }

    // one lock around all builds (cargo can't race the target dir; npm builds also
    // serialize naturally per-service, but the lock is the cross-unit guarantee).
    let _lock = lock(&werk, Duration::from_secs(120))?;
    jsonl(home, role, card, &trace, "lock.acquired", "");

    let mut summary: Vec<String> = Vec::new();
    for unit in &units {
        let (kind_str, name) = match unit {
            BuildUnit::SharedLib(n) => ("sharedlib", n.as_str()),
            BuildUnit::RustCrate(n) => ("rust", n.as_str()),
            BuildUnit::TsService(n) => ("ts", n.as_str()),
        };
        jsonl(
            home,
            role,
            card,
            &trace,
            "unit.build.started",
            &format!(",\"kind\":\"{}\",\"name\":\"{}\"", kind_str, name),
        );
        // Dispatch by unit kind. Each returns the unit's stable identity hash
        // (cdhash for Rust, sha256-of-dist for TS — same contract, different forms,
        // per ADR-032 §1/§5 widened: "stable identity hash").
        let identity = match unit {
            BuildUnit::SharedLib(l) => build_shared_lib(home, role, card, &trace, &werk_s, l)?,
            BuildUnit::RustCrate(c) => build_crate(&werk_s, c)?,
            BuildUnit::TsService(s) => build_ts_service(&werk_s, s)?,
        };
        jsonl(
            home,
            role,
            card,
            &trace,
            "unit.build.completed",
            &format!(
                ",\"kind\":\"{}\",\"name\":\"{}\",\"identity\":\"{}\"",
                kind_str, name, identity
            ),
        );
        // gh status per unit (last write wins for the chorus/build/<card> context;
        // the summary line carries every name=identity pair for werk-deploy).
        register_gh(&werk_s, card, role, &trace, &identity);
        summary.push(format!("{}={}", name, identity));
    }

    let joined = summary.join(",");
    jsonl(home, role, card, &trace, "build.completed", &format!(",\"built\":\"{}\"", joined));
    Ok(joined)
}
