mod hooks;
mod session_cache;
mod state;
mod types;

use axum::{
    extract::{DefaultBodyLimit, State},
    routing::{get, post},
    Json, Router,
};
use state::AppState;
use std::path::Path;
use tokio::net::UnixListener;
use tracing::{info, trace};
use types::{HookInput, HookResponse};

const SOCKET_PATH: &str = "/tmp/chorus-hooks.sock";
const HOOK_LOG: &str = "/Users/jeffbridwell/Library/Logs/Gathering/hooks.log";
const HOOK_LOG_MAX: u64 = 10 * 1024 * 1024; // 10MB rotation

/// Enriched pulse log — module, duration, full reason, session_id (#1859)
fn log_decision(hook: &str, tool: &str, role: &str, module: &str, decision: &str, duration_ms: u64, session_id: &str, reason: &str) {
    use std::io::Write;
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
    let sid = if session_id.len() > 8 { &session_id[..8] } else { session_id };
    let line = format!("{} | {:15} | {:6} | {:5} | {:20} | {:5} | {:4}ms | {} | {}\n",
        ts, hook, tool, role, module, decision, duration_ms, sid, reason);

    // Rotate if over 10MB
    if let Ok(meta) = std::fs::metadata(HOOK_LOG) {
        if meta.len() > HOOK_LOG_MAX {
            let rotated = format!("{}.1", HOOK_LOG);
            let _ = std::fs::rename(HOOK_LOG, &rotated);
        }
    }

    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(HOOK_LOG) {
        let _ = f.write_all(line.as_bytes());
    }
}

/// Backward-compat wrapper for simple entry logging
fn log_hook(hook: &str, tool: &str, role: &str, decision: &str, detail: &str) {
    log_decision(hook, tool, role, "-", decision, 0, "-", detail);
}

/// Circuit-breaker wrapper for async hook calls (#1858)
/// Returns HookResponse::allow() if module is disabled or times out.
async fn guarded_hook<F, Fut>(state: &AppState, module: &str, f: F) -> HookResponse
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = HookResponse>,
{
    // Check circuit breaker
    if state.is_hook_disabled(module).await {
        return HookResponse::allow();
    }

    // Run with 2-second timeout
    match tokio::time::timeout(std::time::Duration::from_secs(2), f()).await {
        Ok(response) => response,
        Err(_) => {
            // Timeout — record failure
            let broke = state.record_hook_failure(module).await;
            let msg = format!("TIMEOUT after 2s");
            log_decision("circuit_breaker", "-", "-", module, if broke { "BREAK" } else { "TIMEOUT" }, 2000, "-", &msg);
            if broke {
                // Notify Bridge
                let _ = std::process::Command::new("curl")
                    .args(["-s", "-X", "POST", "http://localhost:3470/api/message",
                           "-H", "Content-Type: application/json",
                           "-d", &format!(r#"{{"from":"system","text":"[ALERT] Hook circuit-breaker: {} disabled after 3 failures in 5min"}}"#, module),
                           "--connect-timeout", "2"])
                    .output();
            }
            HookResponse::allow() // fail open
        }
    }
}

/// Circuit-breaker wrapper for sync hook calls (#1858)
async fn guarded_hook_sync<F>(state: &AppState, module: &str, f: F) -> HookResponse
where
    F: FnOnce() -> HookResponse,
{
    if state.is_hook_disabled(module).await {
        return HookResponse::allow();
    }
    // Sync hooks can't timeout with tokio — just run them and catch panics
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    match result {
        Ok(response) => response,
        Err(_) => {
            let broke = state.record_hook_failure(module).await;
            log_decision("circuit_breaker", "-", "-", module, if broke { "BREAK" } else { "PANIC" }, 0, "-", "hook panicked");
            if broke {
                let _ = std::process::Command::new("curl")
                    .args(["-s", "-X", "POST", "http://localhost:3470/api/message",
                           "-H", "Content-Type: application/json",
                           "-d", &format!(r#"{{"from":"system","text":"[ALERT] Hook circuit-breaker: {} disabled after 3 panics in 5min"}}"#, module),
                           "--connect-timeout", "2"])
                    .output();
            }
            HookResponse::allow()
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("chorus_hooks=info")
        .with_target(false)
        .init();

    // Clean up old socket
    if Path::new(SOCKET_PATH).exists() {
        let _ = std::fs::remove_file(SOCKET_PATH);
    }

    let state = AppState::new();

    let app = Router::new()
        .route("/health", get(health))
        .route("/pre-tool-use", post(pre_tool_use))
        .route("/post-tool-use", post(post_tool_use))
        .route("/user-prompt-submit", post(user_prompt_submit))
        .route("/stop", post(stop_hook))
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024)) // 16MB — tool_response can be large
        .with_state(state);

    let listener = UnixListener::bind(SOCKET_PATH).expect("Failed to bind unix socket");

    // Set permissions so all users can connect
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(SOCKET_PATH, std::fs::Permissions::from_mode(0o777));
    }

    info!("chorus-hooks listening on {}", SOCKET_PATH);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Server failed");
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl+C handler");
    info!("Shutting down...");
    let _ = std::fs::remove_file(SOCKET_PATH);
}

async fn health() -> &'static str {
    "ok"
}

/// PreToolUse — dispatches to all relevant hook handlers
async fn pre_tool_use(
    State(state): State<AppState>,
    Json(input): Json<HookInput>,
) -> Json<HookResponse> {
    let start = std::time::Instant::now();
    let (module, result) = pre_tool_use_inner(&state, &input).await;
    let duration_ms = start.elapsed().as_millis() as u64;
    let tool = input.tool_name_str();
    let role = input.role();
    let session_id = input.session_id.as_deref().unwrap_or("-");
    let decision = if result.exit_code != 0 { "BLOCK" }
        else if result.stdout.as_ref().map(|s| s.contains("deny")).unwrap_or(false) { "DENY" }
        else if result.stderr.is_some() { "WARN" }
        else { "allow" };
    // Full reason for blocks, truncated for allows
    let reason = if decision == "allow" {
        tool.to_string()
    } else {
        result.stderr.as_deref()
            .or(result.stdout.as_deref())
            .unwrap_or("")
            .lines().next().unwrap_or("")
            .to_string()
    };
    log_decision("pre_tool_use", tool, role.as_str(), &module, decision, duration_ms, session_id, &reason);
    Json(result)
}

async fn pre_tool_use_inner(
    state: &AppState,
    input: &HookInput,
) -> (String, HookResponse) {
    let tool = input.tool_name_str().to_string();
    let role = input.role();
    let mut last_module = String::from("none");
    let detail_str = match tool.as_str() {
        "Bash" => input.get_tool_input_str("command").chars().take(80).collect::<String>(),
        "Read" | "Write" | "Edit" => input.get_tool_input_str("file_path").chars().take(80).collect::<String>(),
        "Grep" | "Glob" => input.get_tool_input_str("pattern").chars().take(80).collect::<String>(),
        "Skill" => input.get_tool_input_str("skill"),
        _ => String::new(),
    };
    log_hook("pre_tool_use", &tool, role.as_str(), "enter", &detail_str);
    trace!(hook = "pre_tool_use", phase = "receive", %tool, role = role.as_str(), "dispatching");

    // Session init gate (all tools for Write/Edit/Bash/Read)
    let gate_result = hooks::session_init_gate::check(&input, &state).await;
    if let Some(ref stdout) = gate_result.stdout {
        if stdout.contains("\"deny\"") {
            return ("session_init_gate".into(), gate_result);
        }
    }

    // Clock sync — update /tmp/wall-clock.txt on every tool call (#1559)
    hooks::clock_sync::tick(&input).await;

    // Set interaction pattern from card type (#1911)
    let role_str = role.as_str();
    if state.get_interaction_pattern(role_str).await == "unknown" {
        let card_type = crate::types::card_type_for_role(role_str);
        if card_type != "unknown" {
            state.set_interaction_pattern(role_str, &card_type).await;
        }
    }

    // Tool telemetry (always, never blocks — fire-and-forget)
    let state_clone = state.clone();
    let input_clone = input.clone();
    tokio::spawn(async move {
        hooks::tool_telemetry::pre_tool_use(&input_clone, &state_clone).await;
    });

    match tool.as_str() {
        "Bash" => {
            // sparql guard
            last_module = "sparql_guard".into(); let r = hooks::sparql_guard::check(&input).await;
            if r.stderr.is_some() {
                return (last_module.clone(), r);
            }

            // bedroom NFS guard
            last_module = "bedroom_nfs_guard".into(); let r = hooks::bedroom_nfs_guard::check(&input);
            if r.stdout.is_some() {
                return (last_module.clone(), r);
            }

            // infra guardrails (engineer only)
            last_module = "infra_guardrails".into(); let r = hooks::infra_guardrails::check(&input).await;
            if r.stdout.is_some() {
                return (last_module.clone(), r);
            }

            // Nudge blast radius (#1658) — warn if target role is WIP
            last_module = "nudge_blast_radius".into(); let r = hooks::nudge_blast_radius::check(&input).await;
            if r.stderr.is_some() {
                // Don't block — just surface the warning
                return (last_module.clone(), r);
            }

            // CSC guard (#1685) — block /tmp/ artifact writes, warn outside /Volumes/Gathering/
            last_module = "csc_guard".into(); let r = hooks::csc_guard::check(&input);
            if r.stdout.is_some() || r.stderr.is_some() {
                return (last_module.clone(), r);
            }

            // TDD gate (#1814) — block demo/done without test evidence
            last_module = "tdd_gate".into(); let r = hooks::tdd_gate::check(&input, &state);
            if r.stdout.is_some() || r.exit_code != 0 {
                return (last_module.clone(), r);
            }

            // Demo gate (#1814) — block done without demo evidence
            last_module = "demo_gate".into(); let r = hooks::demo_gate::check(&input, &state);
            if r.stdout.is_some() || r.exit_code != 0 {
                return (last_module.clone(), r);
            }

            // Batch progress (#1656) — detect run_in_background in PreToolUse
            // PostToolUse doesn't fire until background task completes
            let r = hooks::batch_progress::check_pre_bg(&input);
            if r.stderr.is_some() {
                // Don't block — stderr warning passes through
            }
        }
        "Grep" | "Glob" => {
            last_module = "search_hierarchy".into(); let r = hooks::search_hierarchy::check(&input, &state).await;
            if r.stdout.is_some() || r.exit_code != 0 {
                return (last_module.clone(), r);
            }
        }
        "Write" | "Edit" => {
            // Sensitive paths — block writes to .env, credentials, SSH keys
            last_module = "sensitive_paths".into(); let r = hooks::sensitive_paths::check(&input).await;
            if r.stdout.is_some() {
                return (last_module.clone(), r);
            }
            last_module = "write_scrubber".into(); let r = hooks::write_scrubber::check(&input).await;
            if r.stdout.is_some() {
                return (last_module.clone(), r);
            }
            last_module = "story_write_gate".into(); let r = hooks::story_write_gate::check(&input).await;
            if r.stderr.is_some() || r.exit_code != 0 {
                return (last_module.clone(), r);
            }
            // Memory-and-research gate (#1811) — block code writes without prior checks
            last_module = "memory_gate".into(); let r = hooks::memory_gate::check(&input, &state);
            if r.stdout.is_some() || r.exit_code != 0 {
                return (last_module.clone(), r);
            }

            // Log-first gate (#1879) — block fix writes without log inspection
            last_module = "log_first_gate".into(); let r = hooks::log_first_gate::check(&input, &state);
            if r.stdout.is_some() || r.exit_code != 0 {
                return (last_module.clone(), r);
            }

            // TDD gate — block production code edits without test file edit first
            last_module = "tdd_gate".into(); let r = hooks::tdd_gate::check(&input, &state);
            if r.stdout.is_some() || r.exit_code != 0 {
                return (last_module.clone(), r);
            }

            // Pair gate (#1814) — block code edits without active pair
            last_module = "pair_gate".into(); let r = hooks::pair_gate::check(&input, &state);
            if r.stdout.is_some() || r.exit_code != 0 {
                return (last_module.clone(), r);
            }

            // ICD pre-read gate (#1684) — warn on data domain writes without context read
            hooks::icd_pre_read::check(&input, &state).await;
        }
        "Read" => {
            last_module = "sensitive_paths".into(); let r = hooks::sensitive_paths::check(&input).await;
            if r.stdout.is_some() {
                return (last_module.clone(), r);
            }
            // ICD pre-read (#1684) — set flag when domain context is read
            hooks::icd_pre_read::check(&input, &state).await;
        }
        "AskUserQuestion" => {
            last_module = "autonomy_guard".into(); let r = hooks::autonomy_guard::check(&input, &state).await;
            if r.stdout.is_some() || r.exit_code != 0 {
                return (last_module.clone(), r);
            }
        }
        "Skill" => {
            // TDD gate (#1814) — block demo/done without test evidence
            last_module = "tdd_gate".into(); let r = hooks::tdd_gate::check(&input, &state);
            if r.stdout.is_some() || r.exit_code != 0 {
                return (last_module.clone(), r);
            }
            // Demo gate (#1814) — block done without demo evidence
            last_module = "demo_gate".into(); let r = hooks::demo_gate::check(&input, &state);
            if r.stdout.is_some() || r.exit_code != 0 {
                return (last_module.clone(), r);
            }
            // Quality gate (#1717) — agent review of AC before demo
            last_module = "quality_gate".into(); let r = hooks::quality_gate::pre_demo_check(&input).await;
            if r.stdout.is_some() || r.exit_code != 0 {
                return (last_module.clone(), r);
            }
            // Demo preflight gate (#1657)
            last_module = "demo_preflight".into(); let r = hooks::demo_preflight::check(&input).await;
            if r.stdout.is_some() || r.exit_code != 0 {
                return (last_module.clone(), r);
            }
            // Accept gate (#1671)
            last_module = "accept_gate".into(); let r = hooks::accept_gate::check(&input).await;
            if r.stdout.is_some() || r.exit_code != 0 {
                return (last_module.clone(), r);
            }
            // NiFi discipline (#1686) — warn on bash wrapper pattern
            if let Some(msg) = hooks::nifi_discipline::check(&input, &state).await {
                eprintln!("{}", msg);
            }
            // Pair enforcement (#1673) — nudge target to load /pair
            last_module = "pair_enforcement".into(); let r = hooks::pair_enforcement::check(&input).await;
            if r.stderr.is_some() {
                // Don't block — just notify
                return (last_module.clone(), r);
            }
        }
        _ => {}
    }

    trace!(hook = "pre_tool_use", phase = "respond", %tool, role = role.as_str(), "allow (no guard triggered)");
    ("none".into(), HookResponse::allow())
}

/// PostToolUse — telemetry + handoff logger + observer
async fn post_tool_use(
    State(state): State<AppState>,
    Json(input): Json<HookInput>,
) -> Json<HookResponse> {
    let tool = input.tool_name_str().to_string();

    // Clock sync on every tool call (#1849) — keeps /tmp/wall-clock.txt fresh
    hooks::clock_sync::post_tick(&input).await;

    // Stop-on-error gate (#1841) — block next action when previous errored
    let r = hooks::stop_on_error::check(&input, &state).await;
    if r.exit_code != 0 {
        return Json(r);
    }

    match tool.as_str() {
        "Bash" => {
            hooks::tool_telemetry::post_tool_use_bash(&input, &state).await;
            // Context synthesis tracker — log Chorus searches and git lookups
            hooks::memory_gate::post_check(&input);
            // Proving stage events (#1765) — detect test runs and smoke checks
            let cmd = input.get_tool_input_str("command");
            if cmd.contains("cargo test") || cmd.contains("npx jest") || cmd.contains("npm test") {
                let role_name = input.role().as_str().to_string();
                let test_type = if cmd.contains("cargo") { "rust" } else { "typescript" }.to_string();
                tokio::spawn(async move {
                    crate::state::chorus_log("build.test.completed", &role_name, &[("type", &test_type)]).await;
                });
            }
            if cmd.contains("smoke-check") {
                let role_name = input.role().as_str().to_string();
                tokio::spawn(async move {
                    crate::state::chorus_log("smoke.check.completed", &role_name, &[]).await;
                });
            }
            // Batch progress monitor (#1656) — detect background jobs, emit progress
            let r = hooks::batch_progress::check(&input).await;
            if r.stderr.is_some() {
                // Pass stderr through but don't block
                return Json(r);
            }
        }
        "Read" => {
            // Context synthesis tracker — log memory/decision file reads
            hooks::memory_gate::post_check(&input);
            // Designing stage events (#1765) — domain context + architecture reads
            let read_path = input.get_tool_input_str("file_path");
            if read_path.contains("domain-context") {
                let role_name = input.role().as_str().to_string();
                let domain = read_path.rsplit('/').next().unwrap_or("").replace("domain-context-", "").replace(".md", "");
                let domain_owned = domain.to_string();
                tokio::spawn(async move {
                    crate::state::chorus_log("domain.context.read", &role_name, &[("domain", &domain_owned)]).await;
                });
            }
            // Capturing stage events (#1765) — seed processing
            if read_path.contains("/seeds/") || read_path.contains("seed-") {
                let role_name = input.role().as_str().to_string();
                let artifact = read_path.rsplit('/').next().unwrap_or("").to_string();
                tokio::spawn(async move {
                    crate::state::chorus_log("seed.reviewed", &role_name, &[("artifact", &artifact)]).await;
                });
            }
            if read_path.contains("/adr/") || read_path.contains("system-architecture") || read_path.contains("infrastructure-constraints") {
                let role_name = input.role().as_str().to_string();
                let artifact = read_path.rsplit('/').next().unwrap_or("").to_string();
                tokio::spawn(async move {
                    crate::state::chorus_log("architecture.artifact.read", &role_name, &[("artifact", &artifact)]).await;
                });
            }
        }
        "Write" | "Edit" => {
            hooks::handoff_logger::check(&input, &state).await;
            // Quality gate (#1717) — lightweight post-edit check
            hooks::quality_gate::post_edit_check(&input);
            // ICD write gate — Athena pattern: validate + reload + lint on ICD file writes
            hooks::icd_write_gate::check(&input, &state).await;
            // Artifact creation pulse (#1907) — detect new docs in artifacts/ dirs
            let file_path = input.get_tool_input_str("file_path");
            if file_path.contains("/artifacts/") && tool == "Write" {
                let fname = file_path.rsplit('/').next().unwrap_or(&file_path).to_string();
                let role_name = format!("{:?}", input.role()).to_lowercase();
                // Emit spine event
                crate::state::chorus_log(
                    "artifact.created", &role_name,
                    &[("file", &fname), ("path", &file_path)],
                ).await;
                // Auto-brief relevant roles — all roles except author
                for target in &["wren", "silas", "kade"] {
                    if *target == role_name { continue; }
                    let msg = format!("[artifact] {} created {} — new design doc", role_name, fname);
                    let _ = std::process::Command::new("bash")
                        .args([
                            "/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/nudge.sh",
                            target, &msg, "--level", "info", "--from", &role_name,
                        ])
                        .spawn(); // fire-and-forget, don't block
                }
            }
        }
        _ => {}
    }

    // Demo provenance (#1670) — auto-generate brief after /demo
    if tool == "Skill" {
        let r = hooks::demo_provenance::check(&input).await;
        if r.stderr.is_some() {
            // Pass the notification through
            return Json(r);
        }
    }

    // Persistent observer — digest every meaningful tool call
    let state_clone = state.clone();
    let input_clone = input.clone();
    tokio::spawn(async move {
        hooks::observer::observe(&input_clone, &state_clone).await;
    });

    // L3: nudge drain happens on UserPromptSubmit (werk-init.sh --scan), not PostToolUse
    // PostToolUse stderr only surfaces on exit code 2, which signals error — wrong hook for drain

    Json(HookResponse::allow())
}


/// UserPromptSubmit — clock sync + context injection + autonomy guard + JDI detection
async fn user_prompt_submit(
    State(state): State<AppState>,
    Json(input): Json<HookInput>,
) -> Json<HookResponse> {
    // Clock sync on every prompt (#1559, #1849)
    let clock_result = hooks::clock_sync::tick(&input).await;

    // Input classifier (#1659) — statement vs command detection
    let classifier_result = hooks::input_classifier::check(&input).await;
    if classifier_result.stderr.is_some() {
        // Classification signal — pass through (doesn't block)
        // But still run JDI + autonomy guard
    }

    // Context injection (#1838) — search Chorus + memory, inject before role thinks
    let context_result = hooks::context_inject::check(&input).await;

    // JDI detector — fire-and-forget (#1598)
    let state_clone = state.clone();
    let input_clone = input.clone();
    tokio::spawn(async move {
        hooks::jdi_detector::check(&input_clone, &state_clone).await;
    });

    // E2E responder (#1936) — fire-and-forget, detect [e2e-test] nudges, post ack to Clearing
    let input_e2e = input.clone();
    tokio::task::spawn_blocking(move || {
        hooks::e2e_responder::check(&input_e2e);
    });

    // Merge all stderr signals: classifier + context injection + autonomy guard
    let guard_result = hooks::autonomy_guard::check(&input, &state).await;

    let mut stderr_parts: Vec<String> = Vec::new();
    if let Some(ref msg) = clock_result.stderr {
        stderr_parts.push(msg.clone());
    }
    if let Some(ref msg) = classifier_result.stderr {
        stderr_parts.push(msg.clone());
    }
    if let Some(ref msg) = context_result.stderr {
        stderr_parts.push(msg.clone());
    }
    if let Some(ref msg) = guard_result.stderr {
        stderr_parts.push(msg.clone());
    }

    let merged_stderr = if stderr_parts.is_empty() {
        None
    } else {
        Some(stderr_parts.join("\n"))
    };

    Json(HookResponse {
        stdout: guard_result.stdout,
        stderr: merged_stderr,
        exit_code: guard_result.exit_code,
    })
}

/// Stop hook — autonomy guard (permission-seeking scan)
async fn stop_hook(
    State(state): State<AppState>,
    Json(input): Json<HookInput>,
) -> Json<HookResponse> {
    Json(hooks::autonomy_guard::check(&input, &state).await)
}
