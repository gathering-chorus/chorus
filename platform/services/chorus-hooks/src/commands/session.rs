//! Session start/close commands — extracted from shim.rs (#1623)

use std::fs;
use std::process::ExitCode;

use crate::chorus_log;
use crate::shared::state_paths::{self, repo_root};
use crate::shared::protocol_contract;

use super::context_cache;
use super::pulse;
use super::principles_inject;
use super::athena_tree_inject;

/// Session start — replaces session-start-thin.sh (#1623)
pub fn session_start_cmd(args: &[String]) -> ExitCode {
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    if !matches!(role, "wren" | "silas" | "kade") {
        eprintln!("Usage: chorus-hook-shim session-start <role>");
        return ExitCode::from(1);
    }
    let role_dir = state_paths::role_dir(role).unwrap();
    let role_path = format!("{}/{}", repo_root(), role_dir);
    let cache = format!("/tmp/session-context-{}.md", role);
    let _out = format!("/tmp/session-start-{}.md", role);
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
        content.push_str(ckpt_out.lines().next().unwrap_or(""));
        content.push('\n');
    }

    // Regenerate Pulse so session boots with fresh state (#1889).
    // #2311: use assemble() not run() — run() prints to stdout which would
    // corrupt our hookSpecificOutput JSON envelope below.
    let _ = pulse::assemble();

    // #2731 AC4: defensive regen before protocol_contract::check. Under the
    // derived-artifact model, CLAUDE.md is rebuilt from fragments at the
    // moment of need. Running claudemd-gen here guarantees the file the
    // protocol check (and the harness) will read reflects the live fragment
    // set. Failures emit a spine event but do not block boot — protocol
    // contract is the existing safety net. Cost: ~1s per session start.
    let regen_started = std::time::Instant::now();
    let regen_status = std::process::Command::new(
        format!("{}/platform/scripts/claudemd-gen", repo_root())
    )
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    match regen_status {
        Ok(s) if s.success() => {
            let _ = chorus_log::run_silent(&[
                "session.bootstrap.regen_ok".to_string(),
                role.to_string(),
                format!("duration_ms={}", regen_started.elapsed().as_millis()),
            ]);
        }
        Ok(s) => {
            let _ = chorus_log::run_silent(&[
                "session.bootstrap.regen_failed".to_string(),
                role.to_string(),
                format!("exit_code={}", s.code().unwrap_or(-1)),
            ]);
        }
        Err(e) => {
            let _ = chorus_log::run_silent(&[
                "session.bootstrap.regen_failed".to_string(),
                role.to_string(),
                format!("error={}", e),
            ]);
        }
    }

    // #2311 rescope: run protocol contract check inline at boot. On pass,
    // write .done so PreToolUse gate allows work. On fail, keep .pending
    // armed and prepend PROTOCOL VIOLATION banner to content so the model
    // sees it in additionalContext (no more "please read the file" prose).
    let _ = fs::create_dir_all(init_dir);
    let done_path = format!("{}/{}.done", init_dir, role);
    let pending_path = format!("{}/{}.pending", init_dir, role);
    let _ = fs::remove_file(&done_path);
    let _ = fs::write(&pending_path, "");

    match protocol_contract::check(role) {
        Ok(()) => {
            let _ = fs::write(&done_path, "");
        }
        Err(v) => {
            let banner = protocol_contract::banner(role, &v);
            content = format!("{}\n{}", banner, content);
            let _ = chorus_log::run_silent(&[
                "session.protocol.violation".to_string(),
                role.to_string(),
                format!("reason={}", v.reason()),
            ]);
        }
    }

    // #2450 — inject live principles from /api/loom/principles. Emit a
    // sibling principles-hash for cross-role drift detection. Degrades
    // gracefully on API failure (cache fallback) and surfaces a loud banner
    // on empty response — boot continues either way.
    match principles_inject::fetch() {
        principles_inject::FetchResult::Fresh(ps) => {
            content.push_str(&principles_inject::render_section(&ps, false));
            let h = principles_inject::hash_principles(&ps);
            let _ = principles_inject::write_hash(role, &h);
            let _ = chorus_log::run_silent(&[
                "session.principles.injected".to_string(),
                role.to_string(),
                format!("count={}", ps.len()),
                format!("hash={}", &h[..16]),
            ]);
        }
        principles_inject::FetchResult::Stale(ps) => {
            content.push_str(&principles_inject::render_section(&ps, true));
            let h = principles_inject::hash_principles(&ps);
            let _ = principles_inject::write_hash(role, &h);
            let _ = chorus_log::run_silent(&[
                "session.principles.stale".to_string(),
                role.to_string(),
                format!("count={}", ps.len()),
            ]);
        }
        principles_inject::FetchResult::EmptyFromApi => {
            content.push_str(&principles_inject::render_empty_banner());
            let _ = chorus_log::run_silent(&[
                "session.principles.empty".to_string(),
                role.to_string(),
                "error_type=api_returned_empty_set".to_string(),
            ]);
        }
        principles_inject::FetchResult::Unavailable(reason) => {
            content.push_str(&principles_inject::render_unavailable_banner(&reason));
            let _ = chorus_log::run_silent(&[
                "session.principles.unavailable".to_string(),
                role.to_string(),
                format!("reason={}", reason),
            ]);
        }
    }

    // #2940 — Athena Move 0 tree injection. Appends owned + active + needs-work
    // ranking + ownership map. Graceful degradation: any failure injects a
    // one-line note rather than blocking boot.
    //
    // INVARIANT (Silas gate:arch #2940; cross-link: designing/docs/athena-subproduct-design.html):
    // SessionStart reads the Athena tree from the STATIC JSON at data/athena/tree.json.
    // It does NOT call SPARQL or any live query, even after Move 1's SHACL/OWL lands.
    // Boot latency is the surface defended; turning this into a live query at any
    // future Move re-introduces a per-boot network cost. If the invariant grows
    // (e.g., "all role-boot envelope reads are static") promote to an ADR; for now
    // the constraint lives here + at #2928's reference.
    content.push_str(&athena_tree_inject::render_for_role(role));

    // Log — silent: session-start owns stdout for the hookSpecificOutput envelope.
    let _ = chorus_log::run_silent(&["session.role.started".to_string(), role.to_string()]);

    // Launch Bridge event bus subscriber (#1694)
    // Runs in background for session duration — receives card/state events from other roles
    // Guard: skip if a subscriber for this role is already running
    let subscriber_script = format!(
        "{}/chorus/platform/scripts/bridge-subscriber.js",
        std::env::var("HOME")
            .map(|h| format!("{}/CascadeProjects", h))
            .unwrap_or_else(|_| repo_root().to_string())
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
        let _ = chorus_log::run_silent(&[
            "session.context.error".to_string(),
            role.to_string(),
            "error_type=cache_empty".to_string(),
            format!("lines={}", lines),
        ]);
        eprintln!("⚠ Context cache empty or failed ({} lines) — role booting with partial context", lines);
    }

    // #3125: register this session's {role, pid, tty, host} so nudge delivery
    // can route by tty instead of guessing by window title. Best-effort, writes
    // only to ~/.chorus/sessions/ (never stdout — the envelope below must stay
    // clean). Empty registry → pulse falls back to name-match (as-is).
    super::session_registry::register(role);

    // #2311 rescope: emit Claude Code SessionStart hookSpecificOutput JSON.
    // The harness reads `hookSpecificOutput.additionalContext` and appends it
    // to the model's system view — no "please read /tmp/session-start-<role>.md"
    // prose step, no dependence on the model invoking the Read tool to trigger
    // the protocol contract check. Boot context lands in-window directly.
    let envelope = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SessionStart",
            "additionalContext": content,
        }
    });
    println!("{}", envelope);
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
    let role_path = format!("{}/{}", repo_root(), role_dir);

    // #3125: drop this session's nudge-routing registration (AC2 tear-down).
    // Liveness already prevents a dead session being resolved; this keeps the
    // registry tidy.
    super::session_registry::deregister(role);

    let _ = chorus_log::run(&["protocol.close.started".to_string(), role.to_string()]);

    let mut issues: Vec<String> = Vec::new();

    // next-session.md
    if !std::path::Path::new(&format!("{}/next-session.md", role_path)).exists() {
        issues.push("next-session.md not written".to_string());
    }

    // Board audit
    let board_ts = format!("{}/chorus/platform/scripts/cards", repo_root());
    let _ = std::process::Command::new("bash")
        .args([&board_ts, "audit-close", role])
        .output().ok().and_then(|o| {
            fs::write(format!("/tmp/close-audit-{}.txt", role), &o.stdout).ok()
        });

    // Uncommitted (#2589: env-scrub helper — was raw Command::new("git").
    // -C flag overrides cwd but does NOT override inherited GIT_DIR.)
    let uncommitted = crate::shared::git_command::git_command()
        .args(["-C", repo_root(), "status", "--porcelain", &format!("{}/", role_dir)])
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
            state_paths::chorus_log_file()
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
        "exit=clean".to_string(),
    ]);

    if issues.is_empty() {
        println!("Close: ✓ next-session ✓ board-audit ✓ clean ({}s)", duration_secs);
    } else {
        println!("Close: {} issue(s) ({}s)", issues.len(), duration_secs);
        for issue in &issues { println!("  ⚠ {}", issue); }
    }
    ExitCode::SUCCESS
}
