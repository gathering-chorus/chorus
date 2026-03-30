use crate::state::{append_log, chorus_log, AppState};
use crate::types::{HookInput, HookResponse, Role};
use chrono::Utc;
use std::path::PathBuf;

pub async fn check(input: &HookInput, state: &AppState) -> HookResponse {
    let tool = input.tool_name_str();
    if tool != "Write" && tool != "Edit" {
        return HookResponse::allow();
    }

    let file_path = input.get_tool_input_str("file_path");
    if file_path.is_empty() {
        return HookResponse::allow();
    }

    let filename = file_path.rsplit('/').next().unwrap_or("");

    // PATH 2: State file → SOLID pod sync (best-effort, async)
    match filename {
        "current-work.md" | "tech-debt.md" | "next-session.md" => {
            let pod_role = if file_path.contains("/engineer/") {
                Some("kade")
            } else if file_path.contains("/architect/") {
                Some("silas")
            } else if file_path.contains("/product-manager/") {
                Some("wren")
            } else {
                None
            };

            if let Some(role) = pod_role {
                let sync_script = state.config.repo_root.join("chorus/platform/scripts/pod-state-sync.sh");
                if sync_script.exists() {
                    let role = role.to_string();
                    let fp = file_path.clone();
                    tokio::spawn(async move {
                        let _ = tokio::process::Command::new("bash")
                            .args([&sync_script.to_string_lossy().to_string(), &role, &fp])
                            .output()
                            .await;
                    });
                }
            }
        }
        _ => {}
    }

    // PATH 1: Brief write → handoff logging
    if !file_path.contains("/briefs/") || !file_path.ends_with(".md") {
        return HookResponse::allow();
    }

    // Map file path to recipient role
    let to_role = if file_path.contains("/engineer/briefs/") {
        Role::Kade
    } else if file_path.contains("/architect/briefs/") {
        Role::Silas
    } else if file_path.contains("/product-manager/briefs/") {
        Role::Wren
    } else {
        Role::Unknown
    };

    let from_role = input.role();

    // Skip self-writes
    if from_role == to_role {
        return HookResponse::allow();
    }

    // Check for duplicate (workflow-ts already logged)
    let log_path = PathBuf::from("/Users/jeffbridwell/CascadeProjects/chorus/proving/logs/handoffs.log");
    if log_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&log_path) {
            if content.contains(filename) {
                return HookResponse::allow();
            }
        }
    }

    let ts = Utc::now().with_timezone(&super::clock_sync::boston_offset_pub()).format("%Y-%m-%dT%H:%M:%S%z").to_string();
    let ho_id = format!("HO-{}", Utc::now().timestamp());

    let entry = serde_json::json!({
        "id": ho_id,
        "type": "brief",
        "from": from_role.as_str(),
        "to": to_role.as_str(),
        "artifact": file_path,
        "status": "sent",
        "timestamp": ts
    })
    .to_string();

    append_log(&log_path, &entry).await;

    // Spine event (fire-and-forget)
    let from_str = from_role.as_str().to_string();
    let to_str = to_role.as_str().to_string();
    let basename = filename.to_string();
    tokio::spawn(async move {
        chorus_log(
            "brief.handoff.written",
            &from_str,
            &[("to", &to_str), ("artifact", &basename)],
        )
        .await;
    });

    // Push notification via alert-notifier (fire-and-forget, raw TCP)
    let from_str2 = from_role.as_str().to_string();
    let to_str2 = to_role.as_str().to_string();
    let basename2 = filename.to_string();
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        use tokio::net::TcpStream;
        if let Ok(mut stream) = TcpStream::connect("127.0.0.1:9095").await {
            let body = serde_json::json!({
                "from": from_str2,
                "to": to_str2,
                "artifact": basename2
            })
            .to_string();
            let request = format!(
                "POST /brief HTTP/1.1\r\nHost: localhost:9095\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(request.as_bytes()).await;
        }
    });

    HookResponse::allow()
}
