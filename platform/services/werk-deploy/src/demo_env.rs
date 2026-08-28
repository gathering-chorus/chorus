//! #3092 — per-role demo environment lifecycle.
//!
//! Encapsulates env start / deploy / stop for the per-role variant services
//! (chorus-api, chorus-mcp). Per Jeff's design framing: the env is the unit,
//! not the per-service plumbing. chorus-werk remove calls env_stop;
//! werk-deploy --target werk routes to env_deploy.
//!
//! Three operations, one source of truth for which services belong in a role's
//! demo env + their per-role ports + their plist shapes:
//!   env_start  — provision (generate plists, launchctl bootstrap, write markers)
//!   env_deploy — refresh (rebuild dist + restart variants + smoke)
//!   env_stop   — destroy (bootout, remove markers, clean role-bin slot)
//!
//! Mirrors chorus-mcp's #3016 launchd pattern; extends it to chorus-api and
//! formalizes the lifecycle. Process-isolated per role; state (DB, Fuseki,
//! Loki, Vikunja, log files) is shared with canonical by design.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

pub type R<T> = Result<T, String>;

/// Kind of service in the demo env — affects plist generation + smoke shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvServiceKind {
    /// TypeScript service (chorus-api). Plist runs `node dist/server.js`.
    /// PORT env drives the listening port. Health smoke via HTTP GET.
    TsService,
    /// Rust service (chorus-mcp). Plist runs the compiled binary directly.
    /// PORT env drives the listening port. MCP smoke via initialize handshake.
    RustService,
}

/// One service in the env. Holds everything env_start/deploy/stop needs to
/// generate the right plist + smoke + teardown for it.
#[derive(Debug, Clone)]
pub struct EnvService {
    /// Service name as used by werk-build summary + target_class
    /// (e.g., "chorus-api", "chorus-mcp").
    pub name: String,
    /// Service kind drives plist shape + smoke.
    pub kind: EnvServiceKind,
    /// Per-role port — silas/kade/wren ordering (matches chorus-mcp #3016).
    pub silas_port: u16,
    pub kade_port: u16,
    pub wren_port: u16,
    /// Service source dir relative to repo root (e.g., "platform/api").
    pub source_dir_rel: String,
    /// Path to the LaunchAgent program — either an absolute path to a binary
    /// or "node <abs path to entry>" form. Filled at plist generation time.
    pub program_args_template: ProgramArgsTemplate,
    /// Env var name the service reads for its listening port.
    pub port_env: String,
    /// Health-smoke URL path (relative to http://localhost:<port>).
    pub smoke_path: String,
    /// How to smoke this service — GET (most HTTP services) or POST with an
    /// MCP initialize body (chorus-mcp uses JSON-RPC; GET returns 406).
    pub smoke_kind: SmokeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmokeKind {
    /// curl -f GET, expect 200.
    HttpGet,
    /// POST a JSON-RPC initialize body, expect 200 + result.protocolVersion.
    /// Mirrors v1 chorus-deploy's wait_for_mcp_ready_at.
    McpInitialize,
}

#[derive(Debug, Clone)]
pub enum ProgramArgsTemplate {
    /// Run `node <werk>/<source_dir_rel>/dist/<entry>` from the werk.
    Node { entry: String },
    /// Run `<werk>/<source_dir_rel>/target/release/<binary>` from the werk.
    Rust { binary: String },
    /// #4022 — a deploy-werk artifact (`<CHORUS_WERK_BASE>/<role>-bin/<binary>`)
    /// run with fixed args plus `--port <role port>`. athena-make takes its port
    /// as an argument, not an env, and the werk pipeline installs the built
    /// binary into the role's bin slot, not the crate's target dir.
    WerkBin { binary: String, args: Vec<String> },
}

/// #4022 — where deploy-werk installs a werk's built binaries.
pub fn werk_bin_dir(role: &str) -> String {
    let base = std::env::var("CHORUS_WERK_BASE")
        .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus-werk".to_string());
    format!("{}/{}-bin", base, role)
}

/// #4022 — the chorus-api variant's `/owl` proxy must reach the WERK's
/// athena-make, not production's: an athena-make change is otherwise invisible
/// in the demo env (found 2026-08-28 — the presented variant proxied /owl to
/// prod :3360, which still 502'd on the very route the card fixed).
/// #4022 — where a nightly run FROM a werk writes its log: nightly-suites.sh
/// isolates a werk run to `/tmp/nightly-<werk basename>.log` (#3722) so it never
/// touches the 03:00 log. The api variant's /test-run report must read that
/// same file, or the demo shows production's last nightly, never the card's.
pub fn werk_nightly_log_path(werk_root: &str) -> String {
    let base = Path::new(werk_root).file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| werk_root.to_string());
    format!("/tmp/nightly-{}.log", base)
}

pub fn owl_upstream_for(role: &str) -> R<String> {
    let athena = env_services().into_iter().find(|s| s.name == "athena-make")
        .ok_or_else(|| "env: athena-make is not an env service".to_string())?;
    Ok(format!("http://127.0.0.1:{}", athena.port_for(role)?))
}

/// The canonical role list — silas, kade, wren. Adding a fourth role would
/// extend this list + the per-role port fields on EnvService.
pub fn known_roles() -> &'static [&'static str] {
    &["silas", "kade", "wren"]
}

/// The canonical service list for the demo env. Today: chorus-api + chorus-mcp.
/// hooks/inject named in #3092 as next-slice (TCC grants + per-event invocation
/// dispatch); not in env start/deploy/stop yet.
pub fn env_services() -> Vec<EnvService> {
    vec![
        EnvService {
            name: "chorus-api".to_string(),
            kind: EnvServiceKind::TsService,
            silas_port: 3343,
            kade_port: 3344,
            wren_port: 3345,
            source_dir_rel: "platform/api".to_string(),
            program_args_template: ProgramArgsTemplate::Node {
                entry: "dist/server.js".to_string(),
            },
            port_env: "CHORUS_API_PORT".to_string(),
            smoke_path: "/api/chorus/health".to_string(),
            smoke_kind: SmokeKind::HttpGet,
        },
        EnvService {
            name: "chorus-mcp".to_string(),
            // chorus-mcp is TypeScript living at platform/mcp-server (NOT
            // platform/services/chorus-mcp — caught on the maiden voyage
            // 2026-05-26). Source layout pre-dates the platform/services/
            // convention. The TsService kind reflects truth.
            kind: EnvServiceKind::TsService,
            silas_port: 3351,
            kade_port: 3352,
            wren_port: 3353,
            source_dir_rel: "platform/mcp-server".to_string(),
            program_args_template: ProgramArgsTemplate::Node {
                entry: "dist/main.js".to_string(),
            },
            port_env: "CHORUS_MCP_PORT".to_string(),
            smoke_path: "/mcp".to_string(),
            smoke_kind: SmokeKind::McpInitialize,
        },
        // #4022 — the werk's own athena-make, so the api variant's /owl proxy
        // (and every Athena page through it) shows THIS card's model API.
        EnvService {
            name: "athena-make".to_string(),
            kind: EnvServiceKind::RustService,
            silas_port: 3363,
            kade_port: 3364,
            wren_port: 3365,
            source_dir_rel: "platform/services/athena-make".to_string(),
            program_args_template: ProgramArgsTemplate::WerkBin {
                binary: "athena-make".to_string(),
                args: vec!["serve".to_string()],
            },
            port_env: "ATHENA_MAKE_PORT".to_string(),
            smoke_path: "/health".to_string(),
            smoke_kind: SmokeKind::HttpGet,
        },
    ]
}

impl EnvService {
    /// Per-role port lookup. Returns Err on unknown role so a typo surfaces
    /// rather than silent default.
    pub fn port_for(&self, role: &str) -> R<u16> {
        match role {
            "silas" => Ok(self.silas_port),
            "kade" => Ok(self.kade_port),
            "wren" => Ok(self.wren_port),
            other => Err(format!("env: unknown role '{}' (known: silas/kade/wren)", other)),
        }
    }

    /// LaunchAgent label for this service + role.
    pub fn label(&self, role: &str) -> String {
        format!("com.chorus.{}.werk.{}", strip_chorus_prefix(&self.name), role)
    }

    /// Marker directory inside the canonical repo root for this service —
    /// chorus-env-setup.sh reads <marker_dir>/active to route the session.
    pub fn marker_dir(&self, canonical_root: &str) -> String {
        format!("{}/.werk-{}", canonical_root, strip_chorus_prefix(&self.name))
    }

    /// Per-werk daemon-log path (launchd stdout/stderr — kept per-werk so
    /// boot/crash logs from variants don't tangle with canonical chorus-api.log
    /// or with each other).
    pub fn daemon_log_path(&self, canonical_root: &str) -> String {
        format!("{}/daemon.log", self.marker_dir(canonical_root))
    }
}

/// Strip the "chorus-" prefix so labels read com.chorus.api.werk.silas
/// (not com.chorus.chorus-api.werk.silas).
fn strip_chorus_prefix(name: &str) -> &str {
    name.strip_prefix("chorus-").unwrap_or(name)
}

// --- subprocess helpers (mirror lib.rs run_env so env.rs can be tested
//     independently without pulling the larger module surface in tests) ---

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
        return Err(format!(
            "{} {}: {}",
            cmd,
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    ))
}

fn uid() -> u32 {
    std::env::var("UID").ok().and_then(|s| s.parse().ok()).unwrap_or(501)
}

/// Generate the per-role plist text for a service. Pure (no IO); tested
/// directly. The template intentionally avoids KeepAlive=true — these are
/// demo variants tied to a card; KeepAlive would resurrect them after the
/// role's session ends, defeating teardown.
pub fn generate_plist(
    svc: &EnvService,
    role: &str,
    werk_root: &str,
    port: u16,
    extra_env: &[(&str, &str)],
) -> String {
    let label = svc.label(role);
    let working_dir = format!("{}/{}", werk_root, svc.source_dir_rel);
    // daemon.log lives under the CANONICAL root, not the werk — so it survives
    // env_down (which tears down the werk via chorus-werk remove) and so
    // post-mortem logs remain after the card is acp'd. Path mirrors the
    // marker_dir convention (<canonical>/.werk-<svc>/daemon.log).
    let daemon_log = format!(
        "{}/.werk-{}/daemon.log",
        std::env::var("CHORUS_HOME").unwrap_or_else(|_| werk_root.to_string()),
        strip_chorus_prefix(&svc.name)
    );

    let program_args = match &svc.program_args_template {
        ProgramArgsTemplate::Node { entry } => {
            // Use NVM-managed node since chorus-api requires v20+ (#3085 ABI
            // mismatch lesson — homebrew node v23 crashes better-sqlite3 ABI
            // built for v20). Hardcode the v20.11.1 path matching v1's
            // chorus-api-wrapper.sh.
            let node_bin = "/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin/node";
            vec![node_bin.to_string(), format!("{}/{}", working_dir, entry)]
        }
        ProgramArgsTemplate::Rust { binary } => {
            vec![format!("{}/target/release/{}", working_dir, binary)]
        }
        ProgramArgsTemplate::WerkBin { binary, args } => {
            let mut v = vec![format!("{}/{}", werk_bin_dir(role), binary)];
            v.extend(args.iter().cloned());
            v.push("--port".to_string());
            v.push(port.to_string());
            v
        }
    };

    let program_args_xml: String = program_args
        .iter()
        .map(|a| format!("    <string>{}</string>", xml_escape(a)))
        .collect::<Vec<_>>()
        .join("\n");

    // Env vars: the port env (so the service binds to the role's port), plus
    // any extras the caller wants (e.g., CHORUS_API_SCHEDULED_JOBS=off for
    // werk-api variants — hole 2 of Wren's review).
    let mut env_pairs = vec![
        (svc.port_env.as_str(), port.to_string()),
        ("CHORUS_ROLE", role.to_string()),
        ("CHORUS_API_ENV", "werk".to_string()),
        ("CHORUS_ROOT", werk_root.to_string()),
    ];
    for (k, v) in extra_env {
        env_pairs.push((k, v.to_string()));
    }
    let env_xml: String = env_pairs
        .iter()
        .map(|(k, v)| {
            format!(
                "    <key>{}</key><string>{}</string>",
                xml_escape(k),
                xml_escape(v)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{label}</string>
  <key>ProgramArguments</key>
  <array>
{program_args_xml}
  </array>
  <key>WorkingDirectory</key><string>{working_dir}</string>
  <key>EnvironmentVariables</key>
  <dict>
{env_xml}
  </dict>
  <key>RunAtLoad</key><true/>
  <key>StandardOutPath</key><string>{daemon_log}</string>
  <key>StandardErrorPath</key><string>{daemon_log}</string>
</dict>
</plist>
"#
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Where the role's werk-root sits — same shape chorus-mcp #3016 uses
/// (the werk root the role's card is on). For env_start without a specific
/// card, prefer the role's bin slot's parent (the werk-base) so plists point
/// at a stable location even without a specific card werk. For env_deploy
/// with a card, the werk_root IS the card's werk.
pub fn werk_root_for(role: &str, card: Option<u64>, werk_base: &str) -> R<String> {
    // #3239 — env-up MUST target the card under test. A missing card used to fall back to
    // the role's FIRST werk dir, so /demo silently stood up an ARBITRARY/stale werk (proven
    // live: env-up for kade/3236 ran npm build in kade-3224, a Done card's stale werk; every
    // env.up event logged card_id:0). REFUSE instead of guessing — the caller forwards the
    // card_id; no card means a bug upstream, not a werk to pick.
    match card {
        Some(c) => Ok(format!("{}/{}-{}", werk_base, role, c)),
        None => Err(format!(
            "env-up requires a card_id — refusing to guess the werk for role '{}' (the first-werk fallback stood up arbitrary/stale werks; #3239). Pass card_id.",
            role
        )),
    }
}

/// Poll a service URL until it responds correctly for its smoke kind.
/// HttpGet: 200 from a GET. McpInitialize: 200 from a POST JSON-RPC initialize
/// (mirrors v1 chorus-deploy.sh's wait_for_mcp_ready_at — /mcp returns 406 on
/// GET because the protocol is POST-only).
fn wait_for_smoke(url: &str, kind: &SmokeKind, timeout: Duration) -> R<()> {
    let start = Instant::now();
    let init_body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"werk-deploy-smoke","version":"1.0"}}}"#;
    loop {
        let out = match kind {
            SmokeKind::HttpGet => Command::new("curl")
                .args([
                    "-s", "-f", "-o", "/dev/null", "-w", "%{http_code}",
                    "--max-time", "5", url,
                ])
                .output(),
            SmokeKind::McpInitialize => Command::new("curl")
                .args([
                    "-s", "-f", "-o", "/dev/null", "-w", "%{http_code}",
                    "--max-time", "5",
                    "-X", "POST",
                    "-H", "Content-Type: application/json",
                    "-H", "Accept: application/json, text/event-stream",
                    "-d", init_body,
                    url,
                ])
                .output(),
        };
        if let Ok(o) = out {
            if o.status.success() {
                let code = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if code == "200" {
                    return Ok(());
                }
            }
        }
        if start.elapsed() >= timeout {
            return Err(format!("smoke {} timed out after {:?}", url, timeout));
        }
        sleep(Duration::from_millis(500));
    }
}

// --- the two lifecycle verbs (env up / env down) ---
//
// Builds are cheap on this codebase (chorus-api ~2s, chorus-mcp ~1s cold), so
// the build/bootstrap/smoke phases collapse cleanly into one verb (env_up).
// No separate "start vs deploy" boundary needed — re-running env_up is the
// refresh path. env_down is the destroy path. Two verbs, one lifecycle.

/// Build dist for one service inside the werk. TS services run `npm run build`;
/// future Rust-built TS services would extend this. Cheap (~2s); env_up calls
/// this per service before bootstrapping the variant.
fn build_service_dist(svc: &EnvService, werk_root: &str, role: &str) -> R<()> {
    let svc_dir = format!("{}/{}", werk_root, svc.source_dir_rel);
    if !Path::new(&svc_dir).is_dir() {
        return Err(format!("env_up: service dir not found at {} (werk-pull first?)", svc_dir));
    }
    match svc.program_args_template {
        ProgramArgsTemplate::Node { .. } => {
            run_env(Some(&svc_dir), &[], "npm", &["run", "build"])
                .map(|_| ())
                .map_err(|e| format!("env_up: npm run build in {} failed: {}", svc_dir, e))
        }
        ProgramArgsTemplate::Rust { binary: _ } => {
            // Reserved for future actual-Rust services. Today's chorus-mcp is TS
            // (Node template) even though it lives under platform/services/.
            run_env(Some(&svc_dir), &[], "cargo", &["build", "--release", "--quiet"])
                .map(|_| ())
                .map_err(|e| format!("env_up: cargo build in {} failed: {}", svc_dir, e))
        }
        ProgramArgsTemplate::WerkBin { ref binary, .. } => {
            // #4022 — deploy-werk already built + installed the binary into the
            // role's bin slot; env_up only refuses loudly if it is not there.
            let bin = format!("{}/{}", werk_bin_dir(role), binary);
            if Path::new(&bin).is_file() { Ok(()) } else {
                Err(format!("env_up: {} not found at {} (deploy-werk installs it; run werk-deploy first)", svc.name, bin))
            }
        }
    }
}

/// Stand up the role's demo environment: build dist for each service in the
/// werk, generate plists, bootstrap launchd, smoke, write markers. Idempotent
/// — re-running refreshes against current werk source (this is also the
/// "deploy a change to demo" path; no separate verb needed).
///
/// Returns a summary like `env_up role=silas chorus-api=:3343 chorus-mcp=:3351`.
pub fn env_up(role: &str, werk_root: &str, canonical_root: &str, card: u64, trace: &str) -> R<String> {
    let home_p = Path::new(canonical_root);
    let mut summary = Vec::new();
    for svc in env_services() {
        // Phase 1: build dist for this service in the werk. ~2s for TS.
        // Surfacing per-service so a failure points at exactly which service
        // failed to build, not "env_up failed."
        build_service_dist(&svc, werk_root, role)?;

        // Phase 2: generate plist + bootstrap launchd unit.
        let port = svc.port_for(role)?;
        let marker_dir = svc.marker_dir(canonical_root);
        fs::create_dir_all(&marker_dir)
            .map_err(|e| format!("env_up: mkdir {}: {}", marker_dir, e))?;

        let plist_path = format!(
            "{}/Library/LaunchAgents/{}.plist",
            std::env::var("HOME").unwrap_or_default(),
            svc.label(role)
        );

        // #3381: per-werk isolated stores. The 2026-06-12 wedge root was every
        // variant opening PRODUCTION's ~/.chorus/index.db + lance (WAL locks,
        // three api procs on one store). #3379 landed the seams — chorus-api
        // reads CHORUS_DB_PATH / CHORUS_LANCE_DIR with a prod-default fallback;
        // this env_up now PASSES them, pointing at a per-werk dir the variant
        // owns. Provisioned here, torn down in env_stop. AC3: a variant boot
        // makes ZERO opens of the prod store (lsof-verified).
        let demo_store_dir = format!("{}/.chorus-demo", werk_root);
        let demo_db_path = format!("{}/index.db", demo_store_dir);
        let demo_lance_dir = format!("{}/lance", demo_store_dir);
        fs::create_dir_all(&demo_lance_dir)
            .map_err(|e| format!("env_up: mkdir {}: {}", demo_lance_dir, e))?;

        // #3381 D1(b): seed the demo db from prod (schema + bounded slice) so the
        // variant boots against a REAL store, not an empty file — chorus-api only
        // self-ensures rcas+traces, so an unseeded db throws on the first read of
        // any other table. Kade's design call: the seed is generated by a versioned
        // sampler, and the SAME seed feeds runner tests. This provision-time read of
        // prod is NOT the running variant opening prod (AC3) — the booted variant
        // opens only the demo db. chorus-api owns the db; mcp doesn't need it.
        if svc.name == "chorus-api" {
            let seed_script = format!("{}/platform/scripts/demo-store-seed.sh", werk_root);
            let prod_db = format!(
                "{}/.chorus/index.db",
                std::env::var("HOME").unwrap_or_default()
            );
            run_env(None, &[], "bash", &[&seed_script, &demo_db_path, &prod_db])
                .map_err(|e| format!("env_up: demo-store-seed failed: {}", e))?;
        }

        // Service-specific extra env. chorus-api has scheduled jobs
        // (boardCache, healthCache, reindex worker, crawler-sweep, watchdog)
        // that race on shared SQLite/Fuseki/spine — default OFF in werk-api
        // (Wren hole 2). chorus-mcp doesn't have those; keep extras empty.
        // Add new service-specific gates here, not by kind.
        let owl_upstream = owl_upstream_for(role)?;
        let nightly_log = werk_nightly_log_path(werk_root);
        let css_issuer = css_issuer_for_variant();
        let chorus_home = std::env::var("CHORUS_HOME")
            .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string());
        let extra_env: Vec<(&str, &str)> = match svc.name.as_str() {
            "chorus-api" => vec![
                ("CHORUS_API_SCHEDULED_JOBS", "off"),
                ("CHORUS_DB_PATH", demo_db_path.as_str()),
                ("CHORUS_LANCE_DIR", demo_lance_dir.as_str()),
                // #4022 — /owl proxies to the werk's athena-make, not prod's.
                ("OWL_UPSTREAM", owl_upstream.as_str()),
                // #4022 — /test-run reads the WERK's nightly log, not prod's.
                ("NIGHTLY_LOG_PATH", nightly_log.as_str()),
            ],
            // #4022 — the athena variant verifies write tokens against CSS's
            // JWKS, and the issuer URL reaches the binary ONLY through env
            // (CSS_ISSUER; prod gets it from athena-make-launch.sh). Without it
            // the variant defaults to http://localhost:3001/, CSS answers 500
            // for that identifier, ES256 verifies fail closed, and every
            // nightly writeback to the demo store 401s (2026-08-28 14:37:
            // 0 of 7,289 stored, first_fail_http=401 — the same token was
            // accepted by prod). Same for CHORUS_HOME: the model-resolved
            // allow-set and the identity scripts live under it.
            "athena-make" => vec![
                ("CSS_ISSUER", css_issuer.as_str()),
                ("CHORUS_HOME", chorus_home.as_str()),
            ],
            _ => vec![],
        };

        let plist = generate_plist(&svc, role, werk_root, port, &extra_env);
        fs::write(&plist_path, &plist)
            .map_err(|e| format!("env_up: write {}: {}", plist_path, e))?;

        let label = svc.label(role);
        let domain = format!("gui/{}", uid());
        let unit = format!("{}/{}", domain, label);
        // Bootout any prior instance for idempotency. Then POLL until the unit
        // is actually out of the domain — bootout returns 0 before launchd has
        // fully evicted the unit, and a too-fast bootstrap re-fires before the
        // domain is clean (caught on the maiden voyage 2026-05-26: bootstrap
        // returned silently with no unit loaded).
        let _ = run_env(None, &[], "launchctl", &["bootout", &unit]);
        let still_in_domain = |u: &str| -> bool {
            run_env(None, &[], "launchctl", &["print", u]).is_ok()
        };
        let evict_start = Instant::now();
        while still_in_domain(&unit) {
            if evict_start.elapsed() >= Duration::from_secs(5) {
                break;
            }
            sleep(Duration::from_millis(200));
        }

        // Bootstrap fresh + verify it's actually loaded. If bootstrap returns
        // an error OR the unit isn't in the domain after a brief poll, retry
        // once via kickstart (which can reload a partially-loaded plist).
        let _ = run_env(None, &[], "launchctl", &["bootstrap", &domain, &plist_path]);
        let load_start = Instant::now();
        while !still_in_domain(&unit) {
            if load_start.elapsed() >= Duration::from_secs(5) {
                return Err(format!(
                    "env_up: {} did not load into {} after bootstrap (plist at {}); \
                     check daemon log at {}",
                    label, domain, plist_path, svc.daemon_log_path(canonical_root)
                ));
            }
            sleep(Duration::from_millis(200));
        }

        // Phase 3: smoke. Both services advertise a known endpoint.
        let url = format!("http://localhost:{}{}", port, svc.smoke_path);
        let port_s = port.to_string();
        if let Err(e) = wait_for_smoke(&url, &svc.smoke_kind, Duration::from_secs(30)) {
            // #3215: a smoke fail is the per-service truth Borg needs — emit
            // result=fail on the spine BEFORE the terminal Err so the trace
            // shows which variant didn't come up, not a silent env.up.failed.
            crate::emit_spine(home_p, "env.up.smoked", role, card, trace,
                &[("svc", &svc.name), ("port", &port_s), ("result", "fail")]);
            return Err(format!(
                "env_up: {} smoke failed at {} — {} (plist at {}, daemon log {})",
                svc.name, url, e, plist_path, svc.daemon_log_path(canonical_root)
            ));
        }
        // #3215: per-service smoke success on the spine — env.up.smoked{svc,port,result}.
        crate::emit_spine(home_p, "env.up.smoked", role, card, trace,
            &[("svc", &svc.name), ("port", &port_s), ("result", "ok")]);

        // Phase 4: markers — chorus-env-setup.sh reads .werk-<svc>/active to
        // route the session.
        let active = format!("{}/active", marker_dir);
        let port_file = format!("{}/port", marker_dir);
        let label_file = format!("{}/label", marker_dir);
        fs::write(&active, "")
            .map_err(|e| format!("env_up: write {}: {}", active, e))?;
        fs::write(&port_file, port.to_string())
            .map_err(|e| format!("env_up: write {}: {}", port_file, e))?;
        fs::write(&label_file, &label)
            .map_err(|e| format!("env_up: write {}: {}", label_file, e))?;

        summary.push(format!("{}=:{}", svc.name, port));
    }
    Ok(format!("env_up role={} {}", role, summary.join(" ")))
}

/// Tear down the role's demo environment: bootout all variants, verify they
/// actually exited, remove plists + markers. Keeps marker_dir + daemon.log
/// around for post-mortem (only removes the activation surface).
///
/// Includes a post-bootout verify loop (lesson from the maiden voyage where
/// bootout returned 0 but launchctl list briefly showed the unit lingering).
pub fn env_down(role: &str, canonical_root: &str, card: u64, trace: &str) -> R<String> {
    let home_p = Path::new(canonical_root);
    let domain = format!("gui/{}", uid());
    let mut stopped = Vec::new();
    for svc in env_services() {
        let label = svc.label(role);
        let unit = format!("{}/{}", domain, label);

        // Bootout (terminates + removes from domain). Retry once if the unit
        // is still present after a short pause — handles the lag between
        // launchctl returning and the unit actually being torn down.
        let _ = run_env(None, &[], "launchctl", &["bootout", &unit]);
        let still_present = |unit: &str| -> bool {
            run_env(None, &[], "launchctl", &["print", unit]).is_ok()
        };
        if still_present(&unit) {
            sleep(Duration::from_millis(500));
            let _ = run_env(None, &[], "launchctl", &["bootout", &unit]);
        }

        let plist_path = format!(
            "{}/Library/LaunchAgents/{}.plist",
            std::env::var("HOME").unwrap_or_default(),
            label
        );
        let _ = fs::remove_file(&plist_path);
        let marker_dir = svc.marker_dir(canonical_root);
        let _ = fs::remove_file(format!("{}/active", marker_dir));
        let _ = fs::remove_file(format!("{}/port", marker_dir));
        let _ = fs::remove_file(format!("{}/label", marker_dir));
        // #3215: per-variant teardown on the spine — env.down.stopped{svc}.
        // Borg pairs this against env.up.smoked: an env.up.smoked with no
        // matching env.down.stopped is a LEAK, visible as a gap not a silence.
        crate::emit_spine(home_p, "env.down.stopped", role, card, trace,
            &[("svc", &svc.name), ("label", &label)]);
        stopped.push(label);
    }
    Ok(format!("env_down role={} stopped={}", role, stopped.join(",")))
}

// --- unit tests for the pure helpers (no IO, no subprocess) ---

/// #4022 — the CSS issuer the athena variant must verify against: the
/// builder's env if set, else the same default `athena-make-launch.sh` gives
/// prod. Never the bare-binary default (`http://localhost:3001/`), which CSS
/// rejects as "outside the configured identifier".
pub fn css_issuer_for_variant() -> String {
    std::env::var("CSS_ISSUER")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "https://id.lightlifeurbangardens.com/".to_string())
}

#[cfg(test)]
mod tests {
    /// #4022 — the athena variant carries the CSS issuer (and CHORUS_HOME) the
    /// way prod's launch script does; the bare-binary default is the 401.
    #[test]
    fn athena_variant_plist_carries_the_css_issuer_never_the_bare_default() {
        let athena = env_services().into_iter().find(|s| s.name == "athena-make").unwrap();
        let issuer = css_issuer_for_variant();
        assert!(issuer.starts_with("https://"), "issuer must be the real CSS, got {}", issuer);
        let plist = generate_plist(&athena, "kade", "/werk/kade-4022", 3364,
            &[("CSS_ISSUER", issuer.as_str()), ("CHORUS_HOME", "/Users/x/chorus")]);
        assert!(plist.contains(&format!("<key>CSS_ISSUER</key><string>{}</string>", issuer)), "{}", plist);
        assert!(plist.contains("<key>CHORUS_HOME</key><string>/Users/x/chorus</string>"), "{}", plist);
        assert!(!plist.contains("localhost:3001"), "the bare default is the 401: {}", plist);
        // negative proof (#3734): the plist shape WITHOUT the pair is exactly what
        // was deployed on 2026-08-28 — make sure this test can see that state.
        let bare = generate_plist(&athena, "kade", "/werk/kade-4022", 3364, &[]);
        assert!(!bare.contains("CSS_ISSUER"), "a bare plist must be distinguishable: {}", bare);
    }

    use super::*;

    #[test]
    fn known_roles_lists_three_with_stable_order() {
        assert_eq!(known_roles(), &["silas", "kade", "wren"]);
    }

    #[test]
    fn env_services_includes_api_and_mcp() {
        let svcs = env_services();
        let names: Vec<&str> = svcs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"chorus-api"), "expected chorus-api in env services");
        assert!(names.contains(&"chorus-mcp"), "expected chorus-mcp in env services");
        assert!(names.contains(&"athena-make"), "expected athena-make in env services (#4022)");
    }

    /// #4022 — the api variant's /owl proxy points at the SAME role's athena
    /// variant; the negative half: never at another role's port, never at prod.
    #[test]
    fn api_variant_owl_upstream_is_the_roles_athena_variant() {
        assert_eq!(owl_upstream_for("kade").unwrap(), "http://127.0.0.1:3364");
        assert_eq!(owl_upstream_for("silas").unwrap(), "http://127.0.0.1:3363");
        assert!(owl_upstream_for("ghost").is_err());
        let api = &env_services()[0];
        let up = owl_upstream_for("wren").unwrap();
        let plist = generate_plist(api, "wren", "/werk/wren-1", 3345, &[("OWL_UPSTREAM", up.as_str())]);
        assert!(plist.contains("<key>OWL_UPSTREAM</key><string>http://127.0.0.1:3365</string>"), "{}", plist);
        assert!(!plist.contains("3360") && !plist.contains("3364"), "wren's variant must not reach prod or kade's athena: {}", plist);
    }

    /// #4022 — the api variant's /test-run reads the werk's isolated nightly log
    /// (the path nightly-suites.sh uses for a WERK RUN), never the 03:00 log.
    #[test]
    fn api_variant_reads_the_werks_nightly_log_not_prods() {
        assert_eq!(werk_nightly_log_path("/x/chorus-werk/kade-4022"), "/tmp/nightly-kade-4022.log");
        let api = &env_services()[0];
        let p = werk_nightly_log_path("/x/chorus-werk/silas-9");
        let plist = generate_plist(api, "silas", "/x/chorus-werk/silas-9", 3343, &[("NIGHTLY_LOG_PATH", p.as_str())]);
        assert!(plist.contains("<key>NIGHTLY_LOG_PATH</key><string>/tmp/nightly-silas-9.log</string>"), "{}", plist);
        assert!(!plist.contains("Library/Logs/Chorus"), "never production's nightly log: {}", plist);
    }

    /// #4022 — athena-make runs from the role's deploy-werk bin slot with
    /// `serve --port <role port>`; the crate's target dir is never the program.
    #[test]
    fn athena_variant_runs_from_werk_bin_with_serve_and_port() {
        std::env::set_var("CHORUS_WERK_BASE", "/wb");
        let athena = env_services().into_iter().find(|s| s.name == "athena-make").unwrap();
        let plist = generate_plist(&athena, "kade", "/werk/kade-4022", athena.port_for("kade").unwrap(), &[]);
        std::env::remove_var("CHORUS_WERK_BASE");
        assert!(plist.contains("<string>/wb/kade-bin/athena-make</string>"), "{}", plist);
        assert!(plist.contains("<string>serve</string>"), "{}", plist);
        assert!(plist.contains("<string>--port</string>") && plist.contains("<string>3364</string>"), "{}", plist);
        assert!(!plist.contains("target/release"), "never the crate build dir: {}", plist);
        assert!(plist.contains("com.chorus.athena-make.werk.kade"), "{}", plist);
    }

    #[test]
    fn port_for_returns_role_specific_port() {
        let api = &env_services()[0];
        assert_eq!(api.port_for("silas").unwrap(), 3343);
        assert_eq!(api.port_for("kade").unwrap(), 3344);
        assert_eq!(api.port_for("wren").unwrap(), 3345);
        assert!(api.port_for("ghost").is_err());
    }

    #[test]
    fn label_uses_com_chorus_svc_werk_role_shape() {
        let api = &env_services()[0];
        assert_eq!(api.label("silas"), "com.chorus.api.werk.silas");
        assert_eq!(api.label("wren"), "com.chorus.api.werk.wren");
    }

    #[test]
    fn marker_dir_lives_under_canonical_root_with_stripped_prefix() {
        let api = &env_services()[0];
        assert_eq!(api.marker_dir("/x/canonical"), "/x/canonical/.werk-api");
        let mcp = &env_services()[1];
        assert_eq!(mcp.marker_dir("/x/canonical"), "/x/canonical/.werk-mcp");
    }

    #[test]
    fn generate_plist_includes_port_role_env_and_paths() {
        let api = &env_services()[0];
        let plist = generate_plist(api, "silas", "/werk/silas-3092", 3343, &[("X", "y")]);
        assert!(plist.contains("com.chorus.api.werk.silas"), "label");
        assert!(plist.contains("3343"), "port");
        assert!(plist.contains("CHORUS_API_PORT"), "port env name");
        assert!(plist.contains("CHORUS_ROLE"), "role env");
        assert!(plist.contains("CHORUS_API_ENV"), "env=werk marker env");
        assert!(plist.contains("/werk/silas-3092/platform/api"), "WorkingDirectory");
        assert!(plist.contains("dist/server.js"), "program arg entry");
        assert!(plist.contains("X") && plist.contains("y"), "extra env preserved");
    }

    #[test]
    fn xml_escape_protects_against_meta() {
        // Plist values shouldn't break parsing on stray < > & in working dir
        // or env values.
        assert_eq!(xml_escape("a&b"), "a&amp;b");
        assert_eq!(xml_escape("<x>"), "&lt;x&gt;");
    }
}
