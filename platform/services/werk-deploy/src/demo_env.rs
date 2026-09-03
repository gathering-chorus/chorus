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
        // #4075 — the werk's own Clearing, pointed at the werk's chorus-api, so a
        // card that changes the room (tiles, pane, messages) is SEEN in demo.
        // Jeff, 2026-09-02: "im tired of clearing not being available in demo" /
        // "so no demo card fail". Before this, /clearing on a variant 302'd to
        // prod :3470 and every Clearing change was only visible after land.
        EnvService {
            name: "clearing".to_string(),
            kind: EnvServiceKind::TsService,
            silas_port: 3481,
            kade_port: 3482,
            wren_port: 3483,
            source_dir_rel: "directing/clearing".to_string(),
            program_args_template: ProgramArgsTemplate::Node {
                entry: "dist/server.js".to_string(),
            },
            port_env: "COMMAND_CHANNEL_PORT".to_string(),
            smoke_path: "/health".to_string(),
            smoke_kind: SmokeKind::HttpGet,
        },
    ]
}

/// #4075 — the port one env service listens on for a role, by name. The
/// Clearing needs its api's port; the api needs the Clearing's (for /clearing).
pub fn env_port_for(service: &str, role: &str) -> R<u16> {
    env_services()
        .iter()
        .find(|s| s.name == service)
        .ok_or_else(|| format!("env: no service named '{}'", service))?
        .port_for(role)
}

/// #4075 — the Clearing's HTTPS listener (mic) sits ten above its HTTP port.
pub fn clearing_https_port(http_port: u16) -> u16 {
    http_port + 10
}

/// #4075 — the env a variant Clearing runs with. Pure, so the two properties
/// the card exists for are testable: it reads THIS role's api (never :3340),
/// and its writable files (spine, message store) sit under the werk's
/// .chorus-demo, never the prod paths (#3615 membrane).
pub fn clearing_extra_env(
    role: &str,
    demo_store_dir: &str,
    css_issuer: &str,
    variant_path: &str,
) -> R<Vec<(String, String)>> {
    let api_url = format!("http://localhost:{}", env_port_for("chorus-api", role)?);
    let https = clearing_https_port(env_port_for("clearing", role)?).to_string();
    let signin_url = std::env::var("CHORUS_SIGNIN_URL")
        .unwrap_or_else(|_| "https://chorus.lightlifeurbangardens.com/_auth/".to_string());
    let s = |k: &str, v: String| (k.to_string(), v);
    Ok(vec![
        s("CHORUS_API_URL", api_url.clone()),
        s("CHORUS_API_BASE", api_url.clone()),
        s("PULSE_URL", api_url),
        s("CLEARING_HTTPS_PORT", https),
        s("CHORUS_LOG_FILE", format!("{}/chorus.log", demo_store_dir)),
        s("CLEARING_MSG_FILE", format!("{}/bridge-messages.json", demo_store_dir)),
        s("CHORUS_SIGNIN_URL", signin_url),
        s("CSS_ISSUER", css_issuer.to_string()),
        s("CHORUS_CLEARING_REQUIRE_DPOP", "1".to_string()),
        s("PATH", variant_path.to_string()),
    ])
}

/// #4075 — the prod surfaces a variant Clearing must never be handed. If any
/// value in its env names one, env_up REFUSES (fail-closed) rather than start a
/// room whose posts would land in prod. Silas's gate ask, 2026-09-02: "a message
/// posted in the variant Clearing must land in the variant store, never prod on
/// 3475 or 3340."
pub const CLEARING_PROD_SURFACES: &[&str] = &[
    "localhost:3340",            // prod chorus-api
    "localhost:3475",            // prod pulse / jeff-input
    "localhost:3470",            // prod Clearing
    "/tmp/bridge-messages.json", // prod message store
    "/.chorus/chorus.log",       // prod spine
];

pub fn clearing_env_prod_leak(env: &[(String, String)]) -> Option<String> {
    for (k, v) in env {
        if let Some(hit) = CLEARING_PROD_SURFACES.iter().find(|s| v.contains(*s)) {
            return Some(format!("{}={} names prod surface {}", k, v, hit));
        }
    }
    None
}

/// #4075 — every (service, role) port in the env is distinct; a collision
/// would make one variant's smoke pass on another's process.
pub fn env_ports_collide(services: &[EnvService]) -> Option<(String, u16)> {
    let mut seen = std::collections::HashMap::new();
    for s in services {
        for p in [s.silas_port, s.kade_port, s.wren_port] {
            if let Some(prev) = seen.insert(p, s.name.clone()) {
                return Some((format!("{} vs {}", prev, s.name), p));
            }
        }
    }
    None
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
    // The Fuseki admin credential every variant needs to WRITE to its own store.
    // Without it a reload answers 401 and the service turns that into a 500 —
    // which reads as the variant being broken rather than unauthenticated.
    // launchd gives a plist no inherited environment, so it has to be written in.
    if let Some((user, pw)) = fuseki_admin_creds() {
        env_pairs.push(("FUSEKI_ADMIN_USER", user));
        env_pairs.push(("FUSEKI_ADMIN_PASSWORD", pw));
    }
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
/// #4047 — create the werk's in-memory dataset (idempotent: an existing one is
/// left alone) and load the WERK's model into it, so the variant serves the
/// shapes and claims of the branch under demo rather than prod's. Best-effort
/// by design: a store that cannot be prepared must not block env-up, but it
/// says so loudly instead of silently falling back to prod's data.
/// #4047 follow-on (Silas, 2026-09-02): Fuseki's admin endpoint requires
/// basic auth, and both the create and the drop posted anonymously — 401, then
/// a model seed against a dataset that was never made, which surfaced as a
/// confusing 405. The credential is the same one every bash writer uses via
/// fuseki-auth.sh; read it from the environment rather than inventing a path.
fn fuseki_admin_creds() -> Option<(String, String)> {
    let from_env = std::env::var("FUSEKI_ADMIN_PASSWORD").ok().filter(|p| !p.is_empty());
    let (user, pw) = match from_env {
        Some(pw) => (std::env::var("FUSEKI_ADMIN_USER").unwrap_or_else(|_| "admin".to_string()), pw),
        None => {
            // The pipeline runs under act/launchd, where nothing exports this —
            // which is exactly where the 401 showed up. fuseki-auth.sh has read
            // the credential from this file since #3611; do the same rather than
            // require every caller to source a shell script first. Extract the
            // two keys only, and never log the value.
            let path = std::env::var("FUSEKI_WRITE_ENV").unwrap_or_else(|_| {
                format!("{}/.gathering/data/fuseki-write.env",
                        std::env::var("HOME").unwrap_or_default())
            });
            let body = std::fs::read_to_string(path).ok()?;
            let pick = |k: &str| body.lines()
                .find_map(|l| l.strip_prefix(&format!("{}=", k)))
                .map(|v| v.trim().to_string());
            let pw = pick("FUSEKI_ADMIN_PASSWORD").filter(|p| !p.is_empty())?;
            (pick("FUSEKI_ADMIN_USER").unwrap_or_else(|| "admin".to_string()), pw)
        }
    };
    Some((user, pw))
}

fn fuseki_admin_auth() -> Vec<String> {
    match fuseki_admin_creds() {
        Some((user, pw)) => vec!["-u".to_string(), format!("{}:{}", user, pw)],
        None => Vec::new(),
    }
}

fn prepare_werk_store(role: &str, werk_root: &str) -> String {
    let ds = werk_dataset_name(role);
    let base = werk_fuseki_for(role);
    let base = base.trim_end_matches(&format!("/{}", ds)).to_string();
    let admin = format!("{}/$/datasets", base);
    let auth = fuseki_admin_auth();
    let out = Command::new("curl")
        .args(&auth)
        .args(["-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "POST", &admin,
               "--data", &format!("dbName={}&dbType=mem", ds)])
        .output();
    let mut created = match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        Err(e) => format!("curl-failed:{}", e),
    };
    // #4096 — a 409 means a store from an earlier round is still here (a failed
    // env-up never reaches env-down). Rows it holds were written by that round's
    // code and data — loom sat there owned by role-jeff across four rounds and
    // every later PUT was refused against it. A demo store is fresh or it is
    // not a demo: drop it and create it again.
    if created == "409" {
        let _ = Command::new("curl")
            .args(&auth)
            .args(["-s", "-o", "/dev/null", "-X", "DELETE", &format!("{}/{}", admin, ds)])
            .output();
        created = match Command::new("curl")
            .args(&auth)
            .args(["-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "POST", &admin,
                   "--data", &format!("dbName={}&dbType=mem", ds)])
            .output()
        {
            Ok(o) => format!("{} (recreated fresh)", String::from_utf8_lossy(&o.stdout).trim()),
            Err(e) => format!("curl-failed:{}", e),
        };
    }
    // 200 = created, 409 = already there; both are usable. Anything else means
    // there is no dataset, and seeding into one that does not exist answers 405
    // — a code that sends the reader looking at the wrong thing. Say it here.
    if !(created.starts_with("200") || created == "409") {
        return format!(
            "store={} create_http={} — dataset NOT created (admin auth missing or refused); \
no model seed attempted",
            ds, created
        );
    }
    let deploy = format!("{}/platform/scripts/athena-deploy-model.sh", werk_root);
    let seeded = if Path::new(&deploy).exists() {
        let st = Command::new("bash")
            .arg(&deploy)
            .env("FUSEKI_GSP", format!("{}/{}/data", base, ds))
            .env("FUSEKI_QUERY", format!("{}/{}/query", base, ds))
            .env("FUSEKI_UPDATE", format!("{}/{}/update", base, ds))
            .env("CHORUS_ROOT", werk_root)
            .status();
        match st { Ok(s) if s.success() => "model-seeded".to_string(),
                   Ok(s) => format!("model-seed-FAILED rc={}", s.code().unwrap_or(-1)),
                   Err(e) => format!("model-seed-FAILED {}", e) }
    } else { "model-seed-SKIPPED (no deploy script in werk)".to_string() };
    // #4047 — the TBox alone is not a demo: instances (pipelines, roles, value
    // streams) come from the instance-seed-manifest. #4096 — they are POSTED
    // through the variant athena-make AFTER it boots (see post_werk_rows), not
    // loaded here around it: the file loader was the second door.
    format!("store={} create_http={} {}", ds, created, seeded)
}

/// #4047 — drop the werk's dataset at env-down so no per-card store outlives
/// its demo. In-memory, so the drop is the whole cleanup.
fn drop_werk_store(role: &str) -> String {
    let ds = werk_dataset_name(role);
    let base = werk_fuseki_for(role);
    let base = base.trim_end_matches(&format!("/{}", ds)).to_string();
    let out = Command::new("curl")
        .args(&fuseki_admin_auth())
        .args(["-s", "-o", "/dev/null", "-w", "%{http_code}", "-X", "DELETE",
               &format!("{}/$/datasets/{}", base, ds)])
        .output();
    match out { Ok(o) => format!("store_dropped={} http={}", ds, String::from_utf8_lossy(&o.stdout).trim()),
                Err(e) => format!("store_drop_failed={}", e) }
}

pub fn env_up(role: &str, werk_root: &str, canonical_root: &str, card: u64, trace: &str) -> R<String> {
    let home_p = Path::new(canonical_root);
    let mut summary = Vec::new();
    // #4047 — the werk's own store, prepared BEFORE the services boot so
    // athena-make finds a seeded dataset on first query.
    summary.push(prepare_werk_store(role, werk_root));
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
        let jwks_url = jwks_url_for_variant();
        let model_bin = format!("{}/athena-model", werk_bin_dir(role));
        let werk_fuseki = werk_fuseki_for(role);
        let variant_path = variant_path_for(role);
        let chorus_home = std::env::var("CHORUS_HOME")
            .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string());
        let clearing_port_s = env_port_for("clearing", role)?.to_string();
        let clearing_owned = clearing_extra_env(role, &demo_store_dir, &css_issuer, &variant_path)?;
        if svc.name == "clearing" {
            if let Some(leak) = clearing_env_prod_leak(&clearing_owned) {
                return Err(format!("env_up: refusing to start the variant Clearing — {}", leak));
            }
        }
        let extra_env: Vec<(&str, &str)> = match svc.name.as_str() {
            "chorus-api" => vec![
                ("CLEARING_PORT", clearing_port_s.as_str()),
                ("CHORUS_API_SCHEDULED_JOBS", "off"),
                ("CHORUS_DB_PATH", demo_db_path.as_str()),
                ("CHORUS_LANCE_DIR", demo_lance_dir.as_str()),
                // #4022 — /owl proxies to the werk's athena-make, not prod's.
                ("OWL_UPSTREAM", owl_upstream.as_str()),
                // #4022 — /test-run reads the WERK's nightly log, not prod's.
                ("NIGHTLY_LOG_PATH", nightly_log.as_str()),
                // Silas, 2026-09-02, proving #4058: the variant booted without
                // these three, so an authz demo could not fail — the security
                // envelope was pass-through (chorus-sdk walked straight through
                // reload), the api's own athena sparql/update still pointed at
                // PROD's store while only athena-make got the werk one, and the
                // reload's Fuseki update answered 401 → 500 with no admin
                // credential. A demo env that cannot refuse proves nothing.
                ("CHORUS_SECURITY_ENVELOPE_ENABLE", "1"),
                ("CHORUS_FUSEKI", werk_fuseki.as_str()),
                // Silas, 2026-09-02: with the envelope on, the variant refused
                // EVERYONE with authn-missing — it had no identity verifier at
                // all. Prod's chorus-api carries both of these; the variant
                // carried neither, so "refuses everything" looked like the
                // envelope working when it was the door having no key reader.
                ("CSS_ISSUER", css_issuer.as_str()),
                ("CHORUS_JWKS_URL", jwks_url.as_str()),
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
            // ...and the DAL: every write shells to `athena-model`, resolved via
            // PATH (launchd's default PATH has no chorus bin) — the presented
            // variant answered `dal-spawn: No such file or directory` → 502 on
            // every batch. The variant now names its DAL explicitly (the same
            // deploy-werk bin slot its own binary runs from) and carries prod's
            // PATH shape so subprocesses (curl, jq, node) resolve the same way.
            // #4047 — CHORUS_FUSEKI points the variant at the WERK's own
            // dataset (athena-make/lib.rs:41 reads exactly this var, defaulting
            // to prod's /pods). This is what makes a model change demonstrable
            // before it lands.
            "athena-make" => vec![
                ("CSS_ISSUER", css_issuer.as_str()),
                ("CHORUS_HOME", chorus_home.as_str()),
                ("CHORUS_MODEL_BIN", model_bin.as_str()),
                ("CHORUS_FUSEKI", werk_fuseki.as_str()),
                ("PATH", variant_path.as_str()),
            ],
            // #4075 — the room reads the werk's api (tiles, pulse, pane) and
            // writes ONLY werk-local files: its spine and message store live
            // under the werk's .chorus-demo, never /tmp/bridge-messages.json or
            // ~/.chorus/chorus.log (#3615 membrane). Sign-in is the real one.
            "clearing" => clearing_owned.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect(),
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
    // #4096 — the rows go through the door, as each row's owner, once the
    // variant athena-make answers. A refusal here is the demo's refusal.
    summary.push(post_werk_rows(role, werk_root)?);
    Ok(format!("env_up role={} {}", role, summary.join(" ")))
}

/// #4096 — `athena-model seed --post` against the werk's athena-make: every
/// manifest row created or replaced through POST/PUT, signed by its owner
/// (Jeff: "each owner in turn"). Fails closed: a variant whose rows did not
/// post is not a demo.
fn post_werk_rows(role: &str, werk_root: &str) -> R<String> {
    let api = owl_upstream_for(role)?;
    // the variant may still be finishing its boot; give it a moment to answer
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let up = Command::new("curl").args(["-sf", "--max-time", "3", "-o", "/dev/null", &format!("{}/health", api)]).status()
            .map(|s| s.success()).unwrap_or(false);
        if up { break; }
        if std::time::Instant::now() > deadline {
            return Err(format!("env_up: variant athena-make at {} did not answer /health within 30s — rows not posted", api));
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    // the role's identity is still needed for the ownerless kinds, which
    // `--unowned load` sends through the file loader (said out loud each run)
    let token = Command::new(format!("{}/platform/scripts/chorus-identity-token", werk_root))
        .arg(role)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if token.is_empty() {
        return Err("env_up: no identity token for the role — rows not posted".to_string());
    }
    let out = Command::new(format!("{}/athena-model", werk_bin_dir(role)))
        .args(["seed", "--post", "--api", &api, "--unowned", "load"])
        .env("CHORUS_ROOT", werk_root)
        .env("CHORUS_FUSEKI", werk_fuseki_for(role))
        .env("CHORUS_IDENTITY_TOKEN", &token)
        .output()
        .map_err(|e| format!("env_up: athena-model seed --post: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "env_up: rows did NOT post through {} — {}",
            api,
            String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("").trim()
        ));
    }
    Ok(format!("rows-posted ({})", String::from_utf8_lossy(&out.stdout).lines().last().unwrap_or("").trim()))
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
    // #4047 — the werk's store dies with its env; nothing per-card outlives the demo.
    let dropped = drop_werk_store(role);
    Ok(format!("env_down role={} stopped={} {}", role, stopped.join(","), dropped))
}

// --- unit tests for the pure helpers (no IO, no subprocess) ---

/// #4022 — the CSS issuer the athena variant must verify against: the
/// builder's env if set, else the same default `athena-make-launch.sh` gives
/// prod. Never the bare-binary default (`http://localhost:3001/`), which CSS
/// rejects as "outside the configured identifier".
/// #4022 — PATH for a werk variant: its own bin slot first, then prod's shape
/// (`~/.chorus/bin`, homebrew, system). Subprocesses a variant spawns (the DAL,
/// curl, jq, node) resolve exactly as they do under prod's plist.
pub fn variant_path_for(role: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
    format!("{}:{}/.chorus/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin", werk_bin_dir(role), home)
}

/// #4047 — the werk's OWN Fuseki dataset. Until this, the demo variant ran its
/// own athena-make but pointed it at prod's `/pods`, so a card that CHANGES THE
/// MODEL could never be demonstrated: the variant served yesterday's shapes and
/// the only way to see the change was to land it. That is the gap Jeff's
/// 2026-08-26 rule ("no go if you cannot show it working in demo") kept hitting.
/// One in-memory dataset per role, created at env-up and dropped at env-down —
/// isolated by construction, so a model deploy into it can never touch prod.
pub fn werk_dataset_name(role: &str) -> String {
    format!("werk-{}", role)
}

pub fn werk_fuseki_for(role: &str) -> String {
    let base = std::env::var("CHORUS_FUSEKI_BASE")
        .unwrap_or_else(|_| "http://localhost:3030".to_string());
    format!("{}/{}", base.trim_end_matches('/'), werk_dataset_name(role))
}

/// The local JWKS the verifier fetches. Same default chorus-env-setup.sh uses;
/// the issuer is the LOGICAL public origin (behind Cloudflare) and the JWKS is
/// fetched LOCALLY — fetching the public one from the box is 1010-blocked.
pub fn jwks_url_for_variant() -> String {
    std::env::var("CHORUS_JWKS_URL")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "http://localhost:3001/.oidc/jwks".to_string())
}

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

    /// #4022 — the variant names its DAL and carries a PATH; the bare plist
    /// (what was presented at 15:40 and 502'd every batch) has neither.
    #[test]
    fn athena_variant_names_its_dal_and_path_from_its_own_bin_slot() {
        let athena = env_services().into_iter().find(|s| s.name == "athena-make").unwrap();
        let model_bin = format!("{}/athena-model", werk_bin_dir("kade"));
        let path = variant_path_for("kade");
        assert!(path.starts_with(&werk_bin_dir("kade")), "{}", path);
        assert!(path.contains("/.chorus/bin:") && path.ends_with("/usr/bin:/bin"), "{}", path);
        let plist = generate_plist(&athena, "kade", "/werk/kade-4022", 3364,
            &[("CHORUS_MODEL_BIN", model_bin.as_str()), ("PATH", path.as_str())]);
        assert!(plist.contains(&format!("<key>CHORUS_MODEL_BIN</key><string>{}</string>", model_bin)), "{}", plist);
        assert!(plist.contains("<key>PATH</key><string>"), "{}", plist);
        assert!(!model_bin.contains("target/release"), "the DAL comes from the bin slot, never a build dir: {}", model_bin);
        let bare = generate_plist(&athena, "kade", "/werk/kade-4022", 3364, &[]);
        assert!(!bare.contains("CHORUS_MODEL_BIN") && !bare.contains("<key>PATH</key>"), "{}", bare);
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

#[cfg(test)]
mod store_4047 {
    use super::*;

    /// #4047 — the variant must point at the WERK's dataset, never prod's
    /// /pods. This is the whole reason a model change can be demoed.
    #[test]
    fn werk_fuseki_is_per_role_and_never_pods() {
        let u = werk_fuseki_for("kade");
        assert!(u.ends_with("/werk-kade"), "{}", u);
        assert!(!u.contains("/pods"), "a werk store must never be prod's dataset: {}", u);
        assert_ne!(werk_fuseki_for("kade"), werk_fuseki_for("silas"),
            "two roles demoing at once must not share a store");
    }

    /// #3734 negative proof: if the athena-make env block ever loses
    /// CHORUS_FUSEKI, the variant silently reads prod again — exactly the
    /// failure this card exists to end. The plist must carry it.
    #[test]
    fn plist_carries_chorus_fuseki_or_the_variant_reads_prod() {
        let f = werk_fuseki_for("kade");
        let with = generate_plist(
            &env_services().into_iter().find(|s| s.name == "athena-make").unwrap(),
            "kade", "/tmp/werk", 3364,
            &[("CHORUS_FUSEKI", f.as_str())]);
        assert!(with.contains("<key>CHORUS_FUSEKI</key>"), "{}", with);
        assert!(with.contains(&f), "{}", with);
        // the violation: same plist without the var — the guard must see it missing
        let without = generate_plist(
            &env_services().into_iter().find(|s| s.name == "athena-make").unwrap(),
            "kade", "/tmp/werk", 3364, &[]);
        assert!(!without.contains("CHORUS_FUSEKI"),
            "fixture is bogus if the var appears without being passed");
    }

    /// Silas, 2026-09-02: the chorus-api variant booted without the security
    /// envelope and without its own store, so an authz demo could not fail —
    /// the envelope was pass-through and the api read PROD's graph while only
    /// athena-make got the werk one. A demo env that cannot refuse proves
    /// nothing, so both belong on the plist.
    #[test]
    fn api_variant_plist_carries_the_envelope_flag_and_the_werk_store() {
        let svc = env_services().into_iter().find(|s| s.name == "chorus-api").unwrap();
        let f = werk_fuseki_for("kade");
        let with = generate_plist(&svc, "kade", "/tmp/werk", 3343,
            &[("CHORUS_SECURITY_ENVELOPE_ENABLE", "1"), ("CHORUS_FUSEKI", f.as_str())]);
        assert!(with.contains("<key>CHORUS_SECURITY_ENVELOPE_ENABLE</key>"), "{}", with);
        assert!(with.contains("<key>CHORUS_FUSEKI</key>"), "{}", with);
        assert!(with.contains(&f), "{}", with);

        // NEGATIVE PROOF (#3734): the same plist with neither passed. If this
        // still contained them the assertions above would pass for the wrong
        // reason — the shape of the #3725 trap.
        let without = generate_plist(&svc, "kade", "/tmp/werk", 3343, &[]);
        assert!(!without.contains("CHORUS_SECURITY_ENVELOPE_ENABLE"),
            "fixture is bogus if the flag appears without being passed");
        assert!(!without.contains("CHORUS_FUSEKI"),
            "fixture is bogus if the store appears without being passed");
    }

    /// Silas, 2026-09-02: envelope ON + no verifier env = refuses everyone with
    /// authn-missing, which reads as the envelope working. The issuer is the
    /// LOGICAL public origin; the JWKS is fetched locally (the public one is
    /// 1010-blocked from the box), so they are two different hosts on purpose.
    #[test]
    fn api_variant_plist_carries_both_halves_of_the_identity_verifier() {
        let svc = env_services().into_iter().find(|s| s.name == "chorus-api").unwrap();
        let issuer = css_issuer_for_variant();
        let jwks = jwks_url_for_variant();
        assert!(issuer.starts_with("https://"), "issuer must be the real CSS, got {}", issuer);
        assert!(jwks.contains("/.oidc/jwks"), "jwks must be the local key set, got {}", jwks);
        assert_ne!(issuer, jwks, "issuer and jwks are different hosts by design");
        let with = generate_plist(&svc, "kade", "/tmp/werk", 3343,
            &[("CSS_ISSUER", issuer.as_str()), ("CHORUS_JWKS_URL", jwks.as_str())]);
        assert!(with.contains("<key>CSS_ISSUER</key>"), "{}", with);
        assert!(with.contains("<key>CHORUS_JWKS_URL</key>"), "{}", with);

        // NEGATIVE PROOF (#3734): neither passed — the state that produced
        // authn-missing for every caller.
        let without = generate_plist(&svc, "kade", "/tmp/werk", 3343, &[]);
        assert!(!without.contains("CSS_ISSUER"), "fixture bogus: issuer appears unpassed");
        assert!(!without.contains("CHORUS_JWKS_URL"), "fixture bogus: jwks appears unpassed");
    }
}
