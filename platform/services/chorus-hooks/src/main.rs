mod hooks;
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
    let tool = input.tool_name_str().to_string();
    let role = input.role();
    trace!(hook = "pre_tool_use", phase = "receive", %tool, role = role.as_str(), "dispatching");

    // Session init gate (all tools for Write/Edit/Bash/Read)
    let gate_result = hooks::session_init_gate::check(&input, &state).await;
    if let Some(ref stdout) = gate_result.stdout {
        if stdout.contains("\"deny\"") {
            return Json(gate_result);
        }
    }

    // Clock sync — update /tmp/wall-clock.txt on every tool call (#1559)
    hooks::clock_sync::tick(&input).await;

    // Tool telemetry (always, never blocks — fire-and-forget)
    let state_clone = state.clone();
    let input_clone = input.clone();
    tokio::spawn(async move {
        hooks::tool_telemetry::pre_tool_use(&input_clone, &state_clone).await;
    });

    match tool.as_str() {
        "Bash" => {
            // app-state guard
            let r = hooks::app_state_guard::check(&input);
            if r.stdout.is_some() {
                return Json(r);
            }

            // sparql guard
            let r = hooks::sparql_guard::check(&input).await;
            if r.stderr.is_some() {
                return Json(r);
            }

            // bedroom NFS guard
            let r = hooks::bedroom_nfs_guard::check(&input);
            if r.stdout.is_some() {
                return Json(r);
            }

            // infra guardrails (engineer only)
            let r = hooks::infra_guardrails::check(&input).await;
            if r.stdout.is_some() {
                return Json(r);
            }

            // Nudge blast radius (#1658) — warn if target role is WIP
            let r = hooks::nudge_blast_radius::check(&input).await;
            if r.stderr.is_some() {
                // Don't block — just surface the warning
                return Json(r);
            }

            // CSC guard (#1685) — block /tmp/ artifact writes, warn outside /Volumes/Gathering/
            let r = hooks::csc_guard::check(&input);
            if r.stdout.is_some() || r.stderr.is_some() {
                return Json(r);
            }

            // Batch progress (#1656) — detect run_in_background in PreToolUse
            // PostToolUse doesn't fire until background task completes
            let r = hooks::batch_progress::check_pre_bg(&input);
            if r.stderr.is_some() {
                // Don't block — stderr warning passes through
            }
        }
        "Grep" | "Glob" => {
            let r = hooks::search_hierarchy::check(&input, &state).await;
            if r.stdout.is_some() || r.exit_code != 0 {
                return Json(r);
            }
        }
        "Write" | "Edit" => {
            let r = hooks::write_scrubber::check(&input).await;
            if r.stdout.is_some() {
                return Json(r);
            }
            let r = hooks::story_write_gate::check(&input).await;
            if r.stderr.is_some() || r.exit_code != 0 {
                return Json(r);
            }
            // Memory-and-research gate (#1811) — block code writes without prior checks
            let r = hooks::memory_gate::check(&input);
            if r.stdout.is_some() || r.exit_code != 0 {
                return Json(r);
            }

            // ICD pre-read gate (#1684) — warn on data domain writes without context read
            hooks::icd_pre_read::check(&input, &state).await;
        }
        "Read" => {
            let r = hooks::sensitive_paths::check(&input).await;
            if r.stdout.is_some() {
                return Json(r);
            }
            // ICD pre-read (#1684) — set flag when domain context is read
            hooks::icd_pre_read::check(&input, &state).await;
        }
        "AskUserQuestion" => {
            let r = hooks::autonomy_guard::check(&input, &state).await;
            if r.stdout.is_some() || r.exit_code != 0 {
                return Json(r);
            }
        }
        "Skill" => {
            // Demo preflight gate (#1657)
            let r = hooks::demo_preflight::check(&input).await;
            if r.stdout.is_some() || r.exit_code != 0 {
                return Json(r);
            }
            // Accept gate (#1671)
            let r = hooks::accept_gate::check(&input).await;
            if r.stdout.is_some() || r.exit_code != 0 {
                return Json(r);
            }
            // NiFi discipline (#1686) — warn on bash wrapper pattern
            if let Some(msg) = hooks::nifi_discipline::check(&input, &state).await {
                eprintln!("{}", msg);
            }
            // Pair enforcement (#1673) — nudge target to load /pair
            let r = hooks::pair_enforcement::check(&input).await;
            if r.stderr.is_some() {
                // Don't block — just notify
                return Json(r);
            }
        }
        _ => {}
    }

    trace!(hook = "pre_tool_use", phase = "respond", %tool, role = role.as_str(), "allow (no guard triggered)");
    Json(HookResponse::allow())
}

/// PostToolUse — telemetry + handoff logger + observer
async fn post_tool_use(
    State(state): State<AppState>,
    Json(input): Json<HookInput>,
) -> Json<HookResponse> {
    let tool = input.tool_name_str().to_string();

    match tool.as_str() {
        "Bash" => {
            hooks::tool_telemetry::post_tool_use_bash(&input, &state).await;
            // Batch progress monitor (#1656) — detect background jobs, emit progress
            let r = hooks::batch_progress::check(&input).await;
            if r.stderr.is_some() {
                // Pass stderr through but don't block
                return Json(r);
            }
        }
        "Write" | "Edit" => {
            hooks::handoff_logger::check(&input, &state).await;
            // ICD write gate — Athena pattern: validate + reload + lint on ICD file writes
            hooks::icd_write_gate::check(&input, &state).await;
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


/// UserPromptSubmit — clock sync + autonomy guard + JDI detection
async fn user_prompt_submit(
    State(state): State<AppState>,
    Json(input): Json<HookInput>,
) -> Json<HookResponse> {
    // Clock sync on every prompt (#1559)
    hooks::clock_sync::tick(&input).await;

    // Input classifier (#1659) — statement vs command detection
    let classifier_result = hooks::input_classifier::check(&input).await;
    if classifier_result.stderr.is_some() {
        // Classification signal — pass through (doesn't block)
        // But still run JDI + autonomy guard
    }

    // JDI detector — fire-and-forget (#1598)
    let state_clone = state.clone();
    let input_clone = input.clone();
    tokio::spawn(async move {
        hooks::jdi_detector::check(&input_clone, &state_clone).await;
    });

    // If classifier emitted a signal, merge it with autonomy guard result
    let guard_result = hooks::autonomy_guard::check(&input, &state).await;
    if let Some(ref classifier_msg) = classifier_result.stderr {
        // Classifier has a message — combine with guard result
        let merged_stderr = match guard_result.stderr {
            Some(ref guard_msg) => Some(format!("{}\n{}", classifier_msg, guard_msg)),
            None => Some(classifier_msg.clone()),
        };
        return Json(HookResponse {
            stdout: guard_result.stdout,
            stderr: merged_stderr,
            exit_code: guard_result.exit_code,
        });
    }

    Json(guard_result)
}

/// Stop hook — autonomy guard (permission-seeking scan)
async fn stop_hook(
    State(state): State<AppState>,
    Json(input): Json<HookInput>,
) -> Json<HookResponse> {
    Json(hooks::autonomy_guard::check(&input, &state).await)
}
