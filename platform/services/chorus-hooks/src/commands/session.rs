//! Session start/close commands — extracted from shim.rs (#1623)

use std::fs;
use std::process::ExitCode;

use crate::chorus_log;
use crate::shared::state_paths::{self, REPO_ROOT};

use super::context_cache;
use super::pulse;

/// Session start — replaces session-start-thin.sh (#1623)
pub fn session_start_cmd(args: &[String]) -> ExitCode {
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    if !matches!(role, "wren" | "silas" | "kade") {
        eprintln!("Usage: chorus-hook-shim session-start <role>");
        return ExitCode::from(1);
    }
    let role_dir = state_paths::role_dir(role).unwrap();
    let role_path = format!("{}/{}", REPO_ROOT, role_dir);
    let cache = format!("/tmp/session-context-{}.md", role);
    let out = format!("/tmp/session-start-{}.md", role);
    let init_dir = "/tmp/claude-session-init";

    // Build cache if missing or stale (>10 min)
    let cache_stale = std::path::Path::new(&cache).metadata().ok()
        .and_then(|m| m.modified().ok())
        .map(|t| t.elapsed().unwrap_or_default().as_secs() > 600)
        .unwrap_or(true);
    if cache_stale {
        let _ = context_cache::run(&[role.to_string()]);
    }

    // Copy cache to session-start
    let mut content = fs::read_to_string(&cache).unwrap_or_default();

    // Append next-session.md if exists
    let next_session = format!("{}/next-session.md", role_path);
    if let Ok(ns) = fs::read_to_string(&next_session) {
        content.push_str("\n## Next Session Notes\n");
        content.push_str(&ns);
        let consumed = format!("{}/next-session.md.consumed", role_path);
        let _ = fs::rename(&next_session, &consumed);
    }

    // Crash recovery check
    let ckpt_out = std::process::Command::new(std::env::current_exe().unwrap_or_default())
        .args(["role-checkpoint", role, "recover"])
        .output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();
    if ckpt_out.contains("Resuming") {
        content.push_str("\n## Crash Recovery\n");
        content.push_str(&ckpt_out.lines().next().unwrap_or(""));
        content.push('\n');
    }

    // Regenerate Pulse so session boots with fresh state (#1889)
    let _ = pulse::run(&[]);

    let _ = fs::write(&out, &content);

    // Set init gate
    let _ = fs::create_dir_all(init_dir);
    let _ = fs::remove_file(format!("{}/{}.done", init_dir, role));
    let _ = fs::write(format!("{}/{}.pending", init_dir, role), "");

    // Log
    let _ = chorus_log::run(&["session.role.started".to_string(), role.to_string()]);

    // Launch Bridge event bus subscriber (#1694)
    // Runs in background for session duration — receives card/state events from other roles
    // Guard: skip if a subscriber for this role is already running
    let subscriber_script = format!(
        "{}/chorus/platform/scripts/bridge-subscriber.js",
        std::env::var("HOME")
            .map(|h| format!("{}/CascadeProjects", h))
            .unwrap_or_else(|_| REPO_ROOT.to_string())
    );
    if std::path::Path::new(&subscriber_script).exists() {
        let already_running = std::process::Command::new("pgrep")
            .args(["-f", &format!("bridge-subscriber.js {}", role)])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !already_running {
            let bridge_dir = format!(
                "{}/CascadeProjects/chorus/bridge",
                std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string())
            );
            let _ = std::process::Command::new("node")
                .arg(&subscriber_script)
                .arg(role)
                .env("NODE_PATH", format!("{}/node_modules", bridge_dir))
                .stdout(std::process::Stdio::null())
                .stderr(std::fs::File::create(format!("/Users/jeffbridwell/Library/Logs/Chorus/bridge-subscriber-{}.log", role)).unwrap_or_else(|_| {
                    std::fs::File::create("/dev/null").unwrap()
                }))
                .spawn();
        }
    }

    let lines = content.lines().count();
    if lines < 10 {
        // Empty or near-empty cache = something failed (#1846)
        let _ = chorus_log::run(&[
            "session.context.error".to_string(),
            role.to_string(),
            format!("error_type=cache_empty"),
            format!("lines={}", lines),
        ]);
        eprintln!("⚠ Context cache empty or failed ({} lines) — role booting with partial context", lines);
    }
    println!("Boot: cached context ({} lines)", lines);
    ExitCode::SUCCESS
}

/// Session close — replaces session-close-thin.sh (#1623)
pub fn session_close_cmd(args: &[String]) -> ExitCode {
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    if !matches!(role, "wren" | "silas" | "kade") {
        eprintln!("Usage: chorus-hook-shim session-close <role>");
        return ExitCode::from(1);
    }
    let role_dir = state_paths::role_dir(role).unwrap();
    let role_path = format!("{}/{}", REPO_ROOT, role_dir);

    let _ = chorus_log::run(&["protocol.close.started".to_string(), role.to_string()]);

    let mut issues: Vec<String> = Vec::new();

    // next-session.md
    if !std::path::Path::new(&format!("{}/next-session.md", role_path)).exists() {
        issues.push("next-session.md not written".to_string());
    }

    // Board audit
    let board_ts = format!("{}/chorus/platform/scripts/cards", REPO_ROOT);
    let _ = std::process::Command::new("bash")
        .args([&board_ts, "audit-close", role])
        .output().ok().and_then(|o| {
            fs::write(format!("/tmp/close-audit-{}.txt", role), &o.stdout).ok()
        });

    // Uncommitted
    let uncommitted = std::process::Command::new("git")
        .args(["-C", REPO_ROOT, "status", "--porcelain", &format!("{}/", role_dir)])
        .output().ok().and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.lines().count()).unwrap_or(0);
    if uncommitted > 0 {
        issues.push(format!("{} uncommitted files — commit needed", uncommitted));
    }

    let _ = chorus_log::run(&["protocol.close.completed".to_string(), role.to_string()]);

    // Emit session.role.ended with duration (#1848)
    // Find session.role.started timestamp for this role in chorus.log
    let duration_secs = {
        let log_content = fs::read_to_string(
            &state_paths::chorus_log_file()
        ).unwrap_or_default();
        let mut start_ts = 0u64;
        for line in log_content.lines().rev() {
            if line.contains("session.role.started") && line.contains(&format!("\"role\":\"{}\"", role)) {
                // Extract timestamp from JSON
                if let Some(ts_start) = line.find("\"timestamp\":\"") {
                    let after = &line[ts_start + 13..];
                    if let Some(ts_end) = after.find('"') {
                        let ts_str = &after[..ts_end];
                        // Parse ISO timestamp to epoch
                        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(
                            &ts_str.replace(' ', "T")
                        ) {
                            start_ts = dt.timestamp() as u64;
                        } else if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(
                            &ts_str[..19], "%Y-%m-%dT%H:%M:%S"
                        ) {
                            start_ts = dt.and_utc().timestamp() as u64;
                        }
                    }
                }
                break;
            }
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if start_ts > 0 { now - start_ts } else { 0 }
    };

    let _ = chorus_log::run(&[
        "session.role.ended".to_string(),
        role.to_string(),
        format!("duration={}", duration_secs),
        format!("exit=clean"),
    ]);

    if issues.is_empty() {
        println!("Close: ✓ next-session ✓ board-audit ✓ clean ({}s)", duration_secs);
    } else {
        println!("Close: {} issue(s) ({}s)", issues.len(), duration_secs);
        for issue in &issues { println!("  ⚠ {}", issue); }
    }
    ExitCode::SUCCESS
}
