//! chorus-hook-shim — compiled CLI for Claude Code hooks + nudge.
//!
//! Modes:
//!   chorus-hook-shim <endpoint>         — hook proxy: stdin JSON → unix socket → stdout
//!   chorus-hook-shim nudge <role> <msg> — direct nudge delivery via osascript
//!
//! Falls open if the service is unavailable.

#[path = "nudge.rs"]
mod nudge;
#[path = "role_state.rs"]
mod role_state;
#[path = "chorus_log.rs"]
mod chorus_log;
#[path = "process.rs"]
mod process;
#[path = "ops.rs"]
mod ops;

use std::io::Read;
use std::os::unix::net::UnixStream;
use std::io::Write;
use std::process::ExitCode;

const SOCKET_PATH: &str = "/tmp/chorus-hooks.sock";

fn log_debug(msg: &str) {
    use std::fs::OpenOptions;
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open("/Users/jeffbridwell/Library/Logs/Chorus/chorus-shim-debug.log") {
        let _ = writeln!(f, "[{}] {}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(), msg);
    }
}

fn main() -> ExitCode {
    // Unified dispatch — handles both argv[0] symlinks and `shim <subcommand>` invocations.
    // argv[0] symlink: args start at index 1 (skip binary name)
    // Subcommand:      args start at index 2 (skip binary name + subcommand)
    let all_args: Vec<String> = std::env::args().collect();
    let bin_name = std::path::Path::new(&all_args[0])
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();

    // Determine command and arg offset
    let (cmd, skip) = if bin_name != "chorus-hook-shim" && bin_name != "chorus-hooks" {
        // argv[0] symlink dispatch (e.g., `chorus-log event role`)
        (bin_name.as_str().to_string(), 1usize)
    } else if let Some(subcmd) = all_args.get(1) {
        // Subcommand dispatch (e.g., `chorus-hook-shim log event role`)
        (subcmd.clone(), 2usize)
    } else {
        eprintln!("Usage: chorus-hook-shim <endpoint|nudge>");
        return ExitCode::from(1);
    };

    let args: Vec<String> = all_args.into_iter().skip(skip).collect();

    match cmd.as_str() {
        // --- Core signal path ---
        "chorus-log" | "log" => return chorus_log::run(&args),
        "role-state" => return role_state::run(&args),
        "nudge" => return nudge::run(&args),
        "wall-clock" => return wall_clock_cmd(),
        "heartbeat" => return heartbeat_cmd(),

        // --- Inject (needs special arg check) ---
        "inject" => {
            if args.len() < 2 {
                eprintln!("Usage: chorus-hook-shim inject <role> <message>");
                return ExitCode::from(1);
            }
            return match process::inject_by_tab_name(&args[0], &args[1]) {
                Ok(()) => { println!("DELIVERED to {}", args[0]); ExitCode::SUCCESS }
                Err(e) => { eprintln!("FAILED: {}", e); ExitCode::from(1) }
            };
        }

        // --- Ops + workflow ---
        "chorus-ops" | "ops" => return ops::run(&args),
        "workflow" => return workflow_ts_cmd(),

        // --- Scheduled tasks ---
        "health-hourly" | "context-cache-hourly" => return health_hourly_cmd(&args),
        "health-daily" | "context-cache-daily" => return health_daily_cmd(&args),
        "context-cache" | "context-cache-5min" => return context_cache_cmd(&args),
        "session-start" | "session-start-thin" => return session_start_cmd(&args),
        "session-close" | "session-close-thin" | "session-end-hook" => return session_close_cmd(&args),

        // --- Utilities ---
        "log-rotate" => return log_rotate_cmd(),
        "cruft-scan" => return cruft_scan_cmd(),
        "claudemd-gen" => return claudemd_gen_cmd(),
        "role-checkpoint" => return role_checkpoint_cmd(&args),
        "chorus-init-db" => return chorus_init_db_cmd(),
        "observe-missed" => return observe_missed_cmd(&args),

        _ => {} // Fall through to hook proxy
    }

    // For the hook proxy path, cmd is the endpoint name (pre-tool-use, post-tool-use, etc.)
    let endpoint = cmd;
    log_debug(&format!("START endpoint={}", endpoint));

    // Read stdin with timeout — prevents hang when called from test scripts without piped input.
    // Uses a thread + channel so we can bail after 3 seconds.
    let input = {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = std::io::stdin().read_to_string(&mut buf);
            let _ = tx.send(buf);
        });
        match rx.recv_timeout(std::time::Duration::from_secs(3)) {
            Ok(s) => s,
            Err(_) => {
                log_debug("TIMEOUT reading stdin (3s) — no input piped?");
                // Fall open — return success so the tool isn't blocked
                return ExitCode::SUCCESS;
            }
        }
    };

    // Inject DEPLOY_ROLE into hook input so the service can detect role (#1714)
    let input = if let Ok(deploy_role) = std::env::var("DEPLOY_ROLE") {
        if let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&input) {
            json["deploy_role"] = serde_json::Value::String(deploy_role);
            json.to_string()
        } else {
            input
        }
    } else {
        input
    };

    log_debug(&format!("stdin={}bytes", input.len()));

    // Connect to unix socket — fail open if unavailable
    let mut stream = match UnixStream::connect(SOCKET_PATH) {
        Ok(s) => s,
        Err(e) => {
            log_debug(&format!("FAIL connect: {}", e));
            return ExitCode::SUCCESS;
        }
    };

    // Set timeout
    let timeout = std::time::Duration::from_secs(5);
    let _ = stream.set_write_timeout(Some(timeout));
    let _ = stream.set_read_timeout(Some(timeout));

    // Send HTTP POST
    let body = input.as_bytes();
    let request = format!(
        "POST /{} HTTP/1.1\r\n\
         Host: localhost\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n",
        endpoint,
        body.len()
    );

    if let Err(e) = stream.write_all(request.as_bytes()) {
        log_debug(&format!("FAIL write headers: {}", e));
        return ExitCode::SUCCESS;
    }
    if let Err(e) = stream.write_all(body) {
        log_debug(&format!("FAIL write body: {}", e));
        return ExitCode::SUCCESS;
    }

    // Read response
    let mut response = Vec::new();
    if let Err(e) = stream.read_to_end(&mut response) {
        log_debug(&format!("FAIL read response: {}", e));
        return ExitCode::SUCCESS;
    }

    let response_str = String::from_utf8_lossy(&response);
    log_debug(&format!("response={}bytes first100={}", response.len(), &response_str[..response_str.len().min(100)]));

    // Find the JSON body after headers (separated by \r\n\r\n)
    let json_body = match response_str.find("\r\n\r\n") {
        Some(pos) => &response_str[pos + 4..],
        None => {
            log_debug("FAIL no header separator");
            return ExitCode::SUCCESS;
        }
    };

    // Handle chunked transfer encoding
    let json_str = if response_str.contains("transfer-encoding: chunked")
        || response_str.contains("Transfer-Encoding: chunked")
    {
        let decoded = decode_chunked(json_body);
        log_debug(&format!("chunked decoded={}bytes", decoded.len()));
        decoded
    } else {
        json_body.trim().to_string()
    };

    if json_str.is_empty() {
        log_debug("FAIL empty json body");
        return ExitCode::SUCCESS;
    }

    // Parse JSON response
    let parsed: serde_json::Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            log_debug(&format!("FAIL json parse: {} body={}", e, &json_str[..json_str.len().min(200)]));
            return ExitCode::SUCCESS;
        }
    };

    let exit_code = parsed.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(0);
    log_debug(&format!("OK exit={} has_stdout={} has_stderr={}", exit_code, parsed.get("stdout").is_some(), parsed.get("stderr").is_some()));

    // --raw mode: output full JSON response for test scripts (#1926)
    if std::env::var("CHORUS_HOOK_RAW").is_ok() {
        println!("{}", json_str);
        return ExitCode::from(exit_code as u8);
    }

    if let Some(stdout) = parsed.get("stdout").and_then(|v| v.as_str()) {
        print!("{}\n", stdout);
    }

    if let Some(stderr) = parsed.get("stderr").and_then(|v| v.as_str()) {
        eprint!("{}\n", stderr);
    }

    // L3: drain nudge inbox on PostToolUse — exit 2 surfaces stderr to the model
    if endpoint == "post-tool-use" {
        if let Some(nudges) = drain_nudge_inbox() {
            eprint!("{}\n", nudges);
            return ExitCode::from(2);
        }
    }

    ExitCode::from(exit_code as u8)
}

/// Drain nudge inbox for this role (from DEPLOY_ROLE env or parent CWD)
fn drain_nudge_inbox() -> Option<String> {
    let role = detect_drain_role()?;
    let inbox = format!("/tmp/voice-inbox/{}/pending-inject.txt", role);
    let path = std::path::Path::new(&inbox);
    if !path.exists() { return None; }
    // Atomic drain: rename file first, then read. New writes go to a fresh file.
    let drain_path = format!("/tmp/voice-inbox/{}/draining-{}.txt", role, std::process::id());
    if std::fs::rename(path, &drain_path).is_err() { return None; }
    let content = std::fs::read_to_string(&drain_path).ok()?;
    let _ = std::fs::remove_file(&drain_path);
    if content.trim().is_empty() { return None; }
    // Log consumption as spine event
    let msg_count = content.lines().filter(|l| !l.trim().is_empty()).count();
    let _ = std::process::Command::new("/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log")
        .args(["role.nudge.consumed", &role, &format!("count={}", msg_count)])
        .output();
    // Post consumed nudges to Bridge so Jeff sees them land
    // Skip [bridge] events — they already came FROM Bridge, re-posting creates feedback loop
    for line in content.lines().filter(|l| !l.trim().is_empty()) {
        let display = line.trim().to_string();
        if display.starts_with("[bridge]") {
            continue;
        }
        let body = serde_json::json!({ "from": role, "text": display });
        let _ = std::process::Command::new("curl")
            .args(["-s", "-X", "POST", "http://localhost:3470/api/message",
                   "-H", "Content-Type: application/json",
                   "-d", &body.to_string(), "--connect-timeout", "1"])
            .output();
    }
    Some(format!("<team-scan>\n{}</team-scan>", content.trim()))
}

fn detect_drain_role() -> Option<String> {
    if let Ok(role) = std::env::var("DEPLOY_ROLE") {
        if matches!(role.as_str(), "wren" | "silas" | "kade") {
            return Some(role);
        }
    }
    // Fall back to andon state files
    for role in &["wren", "silas", "kade"] {
        let state_file = format!("/tmp/claude-team-scan/{}-declared.json", role);
        if let Ok(content) = std::fs::read_to_string(&state_file) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(pid) = v.get("pid").and_then(|p| p.as_u64()) {
                    // Check if this shim's ancestor is that role's Claude process
                    if is_ancestor(pid as u32) {
                        return Some(role.to_string());
                    }
                }
            }
        }
    }
    None
}

fn is_ancestor(target_pid: u32) -> bool {
    let mut pid = std::process::id();
    for _ in 0..10 {
        if pid == target_pid { return true; }
        if pid <= 1 { return false; }
        // Get parent PID
        let output = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "ppid="])
            .output()
            .ok();
        pid = output
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
    }
    false
}

/// CLAUDE.md generator — replaces claudemd-gen.sh (#1624)
/// Bash eliminated; Python logic extracted to claudemd-gen.py, called directly.
fn claudemd_gen_cmd() -> ExitCode {
    let repo = "/Users/jeffbridwell/CascadeProjects";
    let script = format!("{}/chorus/platform/scripts/claudemd-gen.py", repo);
    let claudemd_dir = format!("{}/chorus/designing/claudemd", repo);
    let manifest = format!("{}/manifest.json", claudemd_dir);

    // Parse args — skip binary name and "claudemd-gen" subcommand
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Skip "claudemd-gen" if passed as subcommand (vs argv[0])
    let skip = if args.first().map(|s| s.as_str()) == Some("claudemd-gen") { 1 } else { 0 };
    let user_args: Vec<&str> = args.iter().skip(skip).map(|s| s.as_str()).collect();

    let mut mode = "generate";
    let mut role_filter = "";
    let mut card_ref = "";

    let mut i = 0;
    while i < user_args.len() {
        match user_args[i] {
            "--dry-run" => mode = "dry-run",
            "--diff" => mode = "diff",
            "--check" => mode = "check",
            "--validate" => mode = "validate",
            "--compat" => mode = "compat",
            "--history" => mode = "history",
            "--pipeline" => mode = "pipeline",
            "--card" => { i += 1; if i < user_args.len() { card_ref = user_args[i]; } }
            "wren" | "silas" | "kade" => role_filter = user_args[i],
            _ => {}
        }
        i += 1;
    }

    let status = std::process::Command::new("python3")
        .args([&script, &manifest, &claudemd_dir, mode, role_filter, card_ref])
        .status();

    match status {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(s) => ExitCode::from(s.code().unwrap_or(1) as u8),
        Err(e) => {
            eprintln!("Failed to run claudemd-gen.py: {}", e);
            ExitCode::from(1)
        }
    }
}

/// Session start — replaces session-start-thin.sh (#1623)
fn session_start_cmd(args: &[String]) -> ExitCode {
    use std::fs;
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    if !matches!(role, "wren" | "silas" | "kade") {
        eprintln!("Usage: chorus-hook-shim session-start <role>");
        return ExitCode::from(1);
    }
    let repo = "/Users/jeffbridwell/CascadeProjects";
    let role_dir = match role { "wren" => "product-manager", "silas" => "architect", "kade" => "engineer", _ => unreachable!() };
    let role_path = format!("{}/{}", repo, role_dir);
    let cache = format!("/tmp/session-context-{}.md", role);
    let out = format!("/tmp/session-start-{}.md", role);
    let init_dir = "/tmp/claude-session-init";

    // Build cache if missing
    if !std::path::Path::new(&cache).exists() {
        let _ = context_cache_cmd(&[role.to_string()]);
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
    let ckpt_args = vec![role.to_string(), "recover".to_string()];
    let ckpt_out = std::process::Command::new(std::env::current_exe().unwrap_or_default())
        .args(["role-checkpoint", role, "recover"])
        .output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();
    if ckpt_out.contains("Resuming") {
        content.push_str("\n## Crash Recovery\n");
        content.push_str(&ckpt_out.lines().next().unwrap_or(""));
        content.push('\n');
    }

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
            .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects".to_string())
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
    println!("Boot: cached context ({} lines)", lines);
    ExitCode::SUCCESS
}

/// Session close — replaces session-close-thin.sh (#1623)
fn session_close_cmd(args: &[String]) -> ExitCode {
    use std::fs;
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    if !matches!(role, "wren" | "silas" | "kade") {
        eprintln!("Usage: chorus-hook-shim session-close <role>");
        return ExitCode::from(1);
    }
    let repo = "/Users/jeffbridwell/CascadeProjects";
    let role_dir = match role { "wren" => "product-manager", "silas" => "architect", "kade" => "engineer", _ => unreachable!() };
    let role_path = format!("{}/{}", repo, role_dir);

    let _ = chorus_log::run(&["protocol.close.started".to_string(), role.to_string()]);

    let mut issues: Vec<String> = Vec::new();

    // next-session.md
    if !std::path::Path::new(&format!("{}/next-session.md", role_path)).exists() {
        issues.push("next-session.md not written".to_string());
    }

    // Board audit
    let board_ts = format!("{}/chorus/platform/scripts/cards", repo);
    let _ = std::process::Command::new("bash")
        .args([&board_ts, "audit-close", role])
        .output().ok().and_then(|o| {
            fs::write(format!("/tmp/close-audit-{}.txt", role), &o.stdout).ok()
        });

    // Uncommitted
    let uncommitted = std::process::Command::new("git")
        .args(["-C", repo, "status", "--porcelain", &format!("{}/", role_dir)])
        .output().ok().and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.lines().count()).unwrap_or(0);
    if uncommitted > 0 {
        issues.push(format!("{} uncommitted files — commit needed", uncommitted));
    }

    let _ = chorus_log::run(&["protocol.close.completed".to_string(), role.to_string()]);

    if issues.is_empty() {
        println!("Close: ✓ next-session ✓ board-audit ✓ clean");
    } else {
        println!("Close: {} issue(s)", issues.len());
        for issue in &issues { println!("  ⚠ {}", issue); }
    }
    ExitCode::SUCCESS
}

/// Generic Python dispatch — run a .py script with remaining args (#1624)
/// Dispatch to TypeScript workflow engine (replaces workflow.py per DEC-100 / #1775)
fn workflow_ts_cmd() -> ExitCode {
    let cli_path = "/Users/jeffbridwell/CascadeProjects/chorus/platform/workflow-engine/dist/cli.js";
    let all_args: Vec<String> = std::env::args().collect();
    // Skip binary name + "workflow" subcommand if present
    let skip = if all_args.len() > 1 && all_args[1] == "workflow" { 2 } else { 1 };
    let args: Vec<String> = all_args.into_iter().skip(skip).collect();
    let status = std::process::Command::new("node")
        .arg(cli_path)
        .args(&args)
        .status();
    match status {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(s) => ExitCode::from(s.code().unwrap_or(1) as u8),
        Err(e) => { eprintln!("Failed to run workflow-ts: {}", e); ExitCode::from(1) }
    }
}

/// Heartbeat — replaces heartbeat.sh (#1917)
/// Emits system.heartbeat pulse + checks key services.
fn heartbeat_cmd() -> ExitCode {
    // Emit heartbeat
    chorus_log::run(&["system.heartbeat".into(), "silas".into(), "--level=info".into()]);

    // Quick health checks
    let services: &[(&str, &str, &str)] = &[
        ("app", "http://localhost:3000/health", "critical"),
        ("fuseki", "http://localhost:3030/$/ping", "warn"),
        ("clearing", "http://localhost:3470/", "warn"),
        ("chorus-api", "http://localhost:3340/", "warn"),
        ("vikunja", "http://localhost:3456/", "warn"),
    ];

    for (name, url, level) in services {
        let ok = ureq::get(url)
            .timeout(std::time::Duration::from_secs(3))
            .call()
            .is_ok();
        if !ok {
            chorus_log::run(&[
                "system.service.down".into(),
                "silas".into(),
                format!("service={}", name),
                format!("--level={}", level),
            ]);
        }
    }

    ExitCode::SUCCESS
}

/// Role checkpoint — replaces role-checkpoint.sh (#1622)
fn role_checkpoint_cmd(args: &[String]) -> ExitCode {
    use std::fs;
    use std::process::Command as Cmd;

    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    let action = args.get(1).map(|s| s.as_str()).unwrap_or("write");

    if !matches!(role, "wren" | "silas" | "kade") {
        eprintln!("Usage: chorus-hook-shim role-checkpoint <role> [write|read|recover]");
        return ExitCode::from(1);
    }

    let repo = "/Users/jeffbridwell/CascadeProjects";
    let role_dir = match role { "wren" => "product-manager", "silas" => "architect", "kade" => "engineer", _ => unreachable!() };
    let checkpoint = format!("/tmp/role-checkpoint-{}.json", role);
    let board_ts = format!("{}/chorus/platform/scripts/cards", repo);

    match action {
        "write" => {
            // Get WIP card
            let mine_out = Cmd::new("zsh").arg("-lc").arg(format!("{} mine {}", &board_ts, role))
                .output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();
            let wip_line = mine_out.lines().find(|l| l.to_lowercase().contains("[wip]")).unwrap_or("");
            let card_id = wip_line.split_whitespace()
                .find(|w| w.parse::<u32>().is_ok()).unwrap_or("");
            let card_title = wip_line.split(']').last().unwrap_or("").trim()
                .splitn(2, ' ').nth(1).unwrap_or("").split('[').next().unwrap_or("").trim();

            // Recent files
            let recent = Cmd::new("git").args(["-C", repo, "diff", "--name-only", "HEAD"])
                .output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();
            let files: Vec<&str> = recent.lines().take(10).collect();

            let clock = process::wall_clock();
            let json = serde_json::json!({
                "role": role,
                "timestamp": clock,
                "state": "active",
                "card_id": if card_id.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(card_id.to_string()) },
                "card_title": if card_title.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(card_title.to_string()) },
                "recent_files": files,
            });
            let _ = fs::write(&checkpoint, serde_json::to_string_pretty(&json).unwrap_or_default());
        }
        "read" => {
            match fs::read_to_string(&checkpoint) {
                Ok(content) => print!("{}", content),
                Err(_) => println!("{{\"error\": \"no checkpoint found\"}}"),
            }
        }
        "recover" => {
            let next_session = format!("{}/{}/next-session.md", repo, role_dir);
            let needs_recovery = match fs::metadata(&next_session) {
                Ok(meta) => meta.modified().ok()
                    .map(|t| t.elapsed().unwrap_or_default().as_secs() > 7200)
                    .unwrap_or(true),
                Err(_) => true,
            };

            if !needs_recovery {
                println!("Clean session — next-session.md is current");
            } else if let Ok(content) = fs::read_to_string(&checkpoint) {
                let ckpt_age = fs::metadata(&checkpoint).ok()
                    .and_then(|m| m.modified().ok())
                    .map(|t| t.elapsed().unwrap_or_default().as_secs())
                    .unwrap_or(u64::MAX);

                if ckpt_age < 7200 {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                        let card_id = v.get("card_id").and_then(|c| c.as_str()).unwrap_or("?");
                        let card_title = v.get("card_title").and_then(|c| c.as_str()).unwrap_or("unknown");
                        let state = v.get("state").and_then(|s| s.as_str()).unwrap_or("unknown");
                        let ts = v.get("timestamp").and_then(|t| t.as_str()).unwrap_or("unknown");
                        println!("Resuming from checkpoint: card #{}: {} (state: {}, checkpoint: {})", card_id, card_title, state, ts);
                    }
                    print!("{}", content);
                } else {
                    println!("No recent checkpoint available (age: {}s)", ckpt_age);
                }
            } else {
                println!("No checkpoint available for crash recovery");
            }
        }
        _ => {
            eprintln!("Unknown action: {}. Use write|read|recover", action);
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}

/// Chorus DB init — replaces chorus-init-db.sh (#1622)
fn chorus_init_db_cmd() -> ExitCode {
    use std::fs;
    use std::process::Command as Cmd;

    let db_path = std::env::var("CHORUS_DB")
        .unwrap_or_else(|_| format!("{}/.chorus/index.db", std::env::var("HOME").unwrap_or_default()));

    if std::path::Path::new(&db_path).exists() {
        println!("Database already exists at {}", db_path);
        let out = Cmd::new("sqlite3").args([&db_path, ".tables"])
            .output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();
        println!("Tables:\n{}", out);
        let counts = Cmd::new("sqlite3").args([&db_path, "SELECT source, COUNT(*) FROM messages GROUP BY source;"])
            .output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();
        println!("Message count:\n{}", counts);
        return ExitCode::SUCCESS;
    }

    println!("Creating database at {}...", db_path);
    // Ensure parent dir exists
    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        let _ = fs::create_dir_all(parent);
    }

    let sql = r#"
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source TEXT NOT NULL,
    source_id TEXT UNIQUE,
    channel TEXT,
    role TEXT,
    author TEXT,
    content TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    session_id TEXT,
    thread_id TEXT,
    is_bridge INTEGER DEFAULT 0,
    metadata TEXT
);
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    content, role, channel, content='messages', content_rowid='id'
);
CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, content, role, channel) VALUES (new.id, new.content, new.role, new.channel);
END;
CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, content, role, channel) VALUES ('delete', old.id, old.content, old.role, old.channel);
END;
CREATE TABLE IF NOT EXISTS watermarks (
    source TEXT PRIMARY KEY, last_seen TEXT NOT NULL, last_indexed TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_messages_source ON messages(source);
CREATE INDEX IF NOT EXISTS idx_messages_role ON messages(role);
CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp);
CREATE INDEX IF NOT EXISTS idx_messages_channel ON messages(channel);
CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
"#;

    let result = Cmd::new("sqlite3").args([&db_path]).stdin(std::process::Stdio::piped())
        .spawn().and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin { let _ = stdin.write_all(sql.as_bytes()); }
            child.wait()
        });

    match result {
        Ok(status) if status.success() => {
            println!("Database initialized.");
            let out = Cmd::new("sqlite3").args([&db_path, ".tables"])
                .output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();
            println!("{}", out);
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("Failed to initialize database");
            ExitCode::from(1)
        }
    }
}

/// Cruft scan — replaces cruft-scan.sh (#1622)
fn cruft_scan_cmd() -> ExitCode {
    use std::fs;

    let repo = "/Users/jeffbridwell/CascadeProjects";
    let out_path = "/tmp/cruft-scan-latest.md";
    let clock = process::wall_clock();
    let clock_short: String = clock.chars().take(16).collect();

    let mut out = String::with_capacity(4096);
    out.push_str(&format!("# Cruft Scan — {} Boston\n\n", clock_short));

    // Activity log size
    out.push_str("## Activity Log\n");
    let activity_size = fs::metadata(&format!("{}/chorus/activity.md", repo))
        .map(|m| m.len()).unwrap_or(0);
    out.push_str(&format!("Size: {} bytes\n\n", activity_size));

    // Disk check — APFS-aware via Finder free space (includes purgeable, matches Finder)
    out.push_str("## Disk\n");
    let finder_free = std::process::Command::new("osascript")
        .args(["-e", "tell application \"Finder\" to get free space of startup disk"])
        .output().ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<f64>().ok());
    let container_total = std::process::Command::new("diskutil").args(["info", "/"])
        .output().ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|text| {
            text.lines()
                .find(|l| l.contains("Container Total Space"))
                .and_then(|l| l.split('(').nth(1))
                .and_then(|s| s.split_whitespace().next())
                .and_then(|n| n.parse::<u64>().ok())
        });
    let disk_pct = match (container_total, finder_free) {
        (Some(total), Some(free)) if total > 0 => {
            format!("{}%", ((total as f64 - free) / total as f64 * 100.0) as u64)
        }
        _ => "unknown".to_string(),
    };
    out.push_str(&format!("Library: {}\n", disk_pct));
    if disk_pct.starts_with("9") { out.push_str("WARNING: disk above 90%\n"); }
    out.push('\n');

    // Session-start file sizes
    out.push_str("## Session Start Files\n");
    for role in &["silas", "wren", "kade"] {
        let path = format!("/tmp/session-start-{}.md", role);
        if let Ok(meta) = fs::metadata(&path) {
            let size = meta.len();
            let lines = fs::read_to_string(&path).map(|c| c.lines().count()).unwrap_or(0);
            let age_h = meta.modified().ok()
                .map(|t| t.elapsed().unwrap_or_default().as_secs() / 3600).unwrap_or(0);
            let mut flag = String::new();
            if size > 50000 { flag.push_str(" ⚠ BLOATED"); }
            if age_h > 24 { flag.push_str(&format!(" ⚠ STALE ({}h)", age_h)); }
            out.push_str(&format!("  {}: {} bytes, {} lines{}\n", role, size, lines, flag));
        } else {
            out.push_str(&format!("  {}: not found\n", role));
        }
    }
    out.push('\n');

    // Memory file sizes
    out.push_str("## Memory Files (>10KB)\n");
    let mem_dir = "/Users/jeffbridwell/.claude/projects/-Users-jeffbridwell-CascadeProjects/memory";
    if let Ok(entries) = fs::read_dir(mem_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().map(|e| e == "md").unwrap_or(false) {
                if let Ok(meta) = entry.metadata() {
                    if meta.len() > 10240 {
                        out.push_str(&format!("  {}: {} bytes\n",
                            entry.file_name().to_string_lossy(), meta.len()));
                    }
                }
            }
        }
    }
    out.push('\n');

    // CLAUDE.md sizes
    out.push_str("## CLAUDE.md Sizes\n");
    for dir in &["architect", "product-manager", "engineer"] {
        let path = format!("{}/{}/CLAUDE.md", repo, dir);
        if let Ok(meta) = fs::metadata(&path) {
            let flag = if meta.len() > 30000 { " ⚠ HEAVY" } else { "" };
            out.push_str(&format!("  {}: {} bytes{}\n", dir, meta.len(), flag));
        }
    }
    out.push('\n');

    // Disk trend
    out.push_str("## Disk Trend\n");
    let trend_file = "/Users/jeffbridwell/Library/Logs/Chorus/disk-trend.log";
    let disk_num = disk_pct.trim_end_matches('%');
    let today: String = clock.chars().take(10).collect();
    let trend_line = format!("{},{}\n", today, disk_num);
    let _ = fs::OpenOptions::new().create(true).append(true).open(trend_file)
        .and_then(|mut f| { use std::io::Write; f.write_all(trend_line.as_bytes()) });
    if let Ok(content) = fs::read_to_string(trend_file) {
        let recent: Vec<&str> = content.lines().rev().take(5).collect();
        out.push_str(&format!("Recent entries: {}\n", recent.into_iter().rev().collect::<Vec<_>>().join(" ")));
    }
    out.push('\n');

    out.push_str("---\n");
    // Next scan date — 3 days from now
    out.push_str("Next scan: +3 days\n");

    let _ = fs::write(out_path, &out);
    eprintln!("Cruft scan complete → {}", out_path);
    print!("{}", out);
    ExitCode::SUCCESS
}

/// Session context cache — replaces context-cache-5min.sh (#1622)
/// Builds /tmp/session-context-<role>.md with board state, briefs, health, memory.
fn context_cache_cmd(args: &[String]) -> ExitCode {
    use std::fs;
    use std::process::Command as Cmd;

    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    if !matches!(role, "wren" | "silas" | "kade") {
        eprintln!("Usage: chorus-hook-shim context-cache <role>");
        return ExitCode::from(1);
    }

    let repo = "/Users/jeffbridwell/CascadeProjects";
    let role_dir_name = match role { "wren" => "product-manager", "silas" => "architect", "kade" => "engineer", _ => unreachable!() };
    let role_dir = format!("{}/{}", repo, role_dir_name);
    let board_ts = format!("{}/chorus/platform/scripts/cards", repo);
    let out_path = format!("/tmp/session-context-{}.md", role);

    // Werk version
    let manifest_path = format!("{}/chorus/designing/claudemd/manifest.json", repo);
    let werk_version = fs::read_to_string(&manifest_path).ok()
        .and_then(|c| c.lines().find(|l| l.contains("\"version\"")).map(|l| {
            l.split('"').nth(3).unwrap_or("unknown").to_string()
        }))
        .unwrap_or_else(|| "unknown".to_string());

    // --- Parallel data gathering via threads ---
    let board_ts_c = board_ts.clone();
    let role_s = role.to_string();
    let board_mine = std::thread::spawn(move || -> (bool, String) {
        match Cmd::new("zsh").arg("-lc").arg(format!("{} mine {}", board_ts_c, role_s)).output() {
            Ok(o) if o.status.success() => (true, String::from_utf8(o.stdout).unwrap_or_default()),
            _ => (false, String::new()),
        }
    });

    let board_ts_c = board_ts.clone();
    let board_list = std::thread::spawn(move || -> (bool, String) {
        match Cmd::new("zsh").arg("-lc").arg(format!("{} list", board_ts_c)).output() {
            Ok(o) if o.status.success() => (true, String::from_utf8(o.stdout).unwrap_or_default()),
            _ => (false, String::new()),
        }
    });

    let board_ts_c = board_ts.clone();
    let role_s = role.to_string();
    let board_audit = std::thread::spawn(move || {
        Cmd::new("zsh").arg("-lc").arg(format!("{} audit-start {}", board_ts_c, role_s))
            .output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default()
    });

    // Recent decisions
    let decisions_path = format!("{}/product-manager/decisions.md", repo);
    let decisions = fs::read_to_string(&decisions_path).ok()
        .map(|c| c.lines().filter(|l| l.starts_with("## DEC-"))
            .collect::<Vec<_>>().into_iter().rev().take(10).collect::<Vec<_>>()
            .into_iter().rev().collect::<Vec<_>>().join("\n"))
        .unwrap_or_default();

    // Recent briefs
    let briefs_dir = format!("{}/briefs", role_dir);
    let briefs = if let Ok(mut entries) = fs::read_dir(&briefs_dir) {
        let mut files: Vec<_> = entries.by_ref()
            .flatten()
            .filter(|e| e.path().extension().map(|x| x == "md").unwrap_or(false))
            .collect();
        files.sort_by(|a, b| b.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            .cmp(&a.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH)));
        files.iter().take(5)
            .map(|e| format!("- {}", e.file_name().to_string_lossy()))
            .collect::<Vec<_>>().join("\n")
    } else { String::new() };

    // Handoff check
    let handoff_log = format!("{}/chorus/proving/logs/handoffs.log", repo);
    let role_s = role.to_string();
    let briefs_dir_c = briefs_dir.clone();
    let archive_dir = format!("{}/chorus/proving/workflows/archive", repo);
    let handoff_check = std::thread::spawn(move || {
        check_handoffs(&handoff_log, &role_s, &briefs_dir_c, &archive_dir)
    });

    // Memory context from Chorus API
    let board_ts_c = board_ts.clone();
    let role_s = role.to_string();
    let memory_ctx = std::thread::spawn(move || {
        fetch_memory_context(&board_ts_c, &role_s)
    });

    // Wait for threads — track failures for spine event
    let mut failed_sources: Vec<&str> = Vec::new();
    let mut ok_sources: Vec<&str> = Vec::new();

    let (mine_ok, mine_text) = board_mine.join().unwrap_or((false, String::new()));
    if mine_ok { ok_sources.push("board_mine"); } else { failed_sources.push("board_mine"); }

    let (list_ok, list_text) = board_list.join().unwrap_or((false, String::new()));
    if list_ok { ok_sources.push("board_list"); } else { failed_sources.push("board_list"); }

    let audit_text = board_audit.join().unwrap_or_default();
    if !audit_text.is_empty() { ok_sources.push("board_audit"); } else { failed_sources.push("board_audit"); }

    let handoff_text = handoff_check.join().unwrap_or_default();
    ok_sources.push("handoffs"); // handoffs returning empty is normal (no pending)

    let memory_text = memory_ctx.join().unwrap_or_default();
    if !memory_text.is_empty() { ok_sources.push("memory"); } else { failed_sources.push("memory"); }

    // Health checks — APFS disk via Finder free space (includes purgeable, matches Finder)
    let disk_pct = {
        // osascript returns Finder's free space which includes purgeable — matches what Jeff sees
        let finder_free = Cmd::new("osascript")
            .args(["-e", "tell application \"Finder\" to get free space of startup disk"])
            .output().ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse::<f64>().ok());
        let container_total = Cmd::new("diskutil").args(["info", "/"])
            .output().ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|text| {
                text.lines()
                    .find(|l| l.contains("Container Total Space"))
                    .and_then(|l| l.split('(').nth(1))
                    .and_then(|s| s.split_whitespace().next())
                    .and_then(|n| n.parse::<u64>().ok())
            });
        match (container_total, finder_free) {
            (Some(total), Some(free)) if total > 0 => {
                format!("{}%", ((total as f64 - free) / total as f64 * 100.0) as u64)
            }
            _ => "?%".to_string(),
        }
    };
    let disk_pct = disk_pct.as_str();

    let uncommitted = Cmd::new("git").args(["-C", repo, "status", "--porcelain", &format!("{}/", role_dir_name)])
        .output().ok().and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.lines().count()).unwrap_or(0);

    let activity_path = format!("{}/chorus/activity.md", repo);
    let activity_age = fs::metadata(&activity_path).ok()
        .and_then(|m| m.modified().ok())
        .map(|t| format!("{}h", t.elapsed().unwrap_or_default().as_secs() / 3600))
        .unwrap_or_else(|| "unknown".to_string());

    let claude_path = format!("{}/CLAUDE.md", role_dir);
    let claude_status = fs::metadata(&claude_path).ok()
        .and_then(|m| m.modified().ok())
        .map(|t| {
            let days = t.elapsed().unwrap_or_default().as_secs() / 86400;
            if days > 3 { format!("stale ({}d)", days) } else { "clean".to_string() }
        })
        .unwrap_or_else(|| "missing".to_string());

    // Capitalize role name
    let role_cap = format!("{}{}", &role[..1].to_uppercase(), &role[1..]);
    let clock = process::wall_clock();
    let clock_short: String = clock.chars().take(16).collect();

    // --- Assemble ---
    let mut out = String::with_capacity(8192);
    out.push_str(&format!("# {} — Session Context (Werk v{})\n", role_cap, werk_version));
    out.push_str(&format!("Generated: {} Boston | Werk v{}\n\n", clock_short, werk_version));

    out.push_str("## Your Active Cards\n");
    out.push_str(if !mine_ok { "(board unreachable)" } else if mine_text.is_empty() { "(none)" } else { &mine_text });
    out.push_str("\n\n## Boards\n\n");
    out.push_str(if !list_ok { "(board unreachable)" } else if list_text.is_empty() { "(none)" } else { &list_text });
    out.push_str("\n\n## Board Audit\n\n");
    out.push_str(if audit_text.is_empty() { "(none)" } else { &audit_text });

    out.push_str("\n\n## Workflow Steps Waiting\n");
    out.push_str("(none)\n"); // workflow dispatches to TS engine (#1775)

    out.push_str("\n## Recent Briefs\n");
    out.push_str(if briefs.is_empty() { "(none)" } else { &briefs });

    out.push_str("\n\n## Recent Decisions\n");
    out.push_str(if decisions.is_empty() { "(decisions.md not found)" } else { &decisions });

    out.push_str("\n\n## Health\n\n");
    out.push_str(&format!("- Disk: {}\n", disk_pct));
    out.push_str(&format!("- Uncommitted in {}/: {}\n", role_dir_name, uncommitted));
    out.push_str(&format!("- Activity.md: updated {} ago\n", activity_age));
    out.push_str(&format!("- CLAUDE.md: {}\n", claude_status));

    out.push_str("\n## Memory Context\n");
    if memory_text.is_empty() {
        out.push_str("(none)\n");
    } else {
        let line_count = memory_text.lines().count();
        out.push_str(&format!("Related memories for WIP cards ({} found):\n\n", line_count));
        out.push_str(&memory_text);
    }

    out.push_str("\n## Handoff Check\n");
    out.push_str(if handoff_text.is_empty() { "(clean)" } else { &handoff_text });
    out.push('\n');

    let _ = fs::write(&out_path, &out);
    let lines = out.lines().count();
    println!("Context cached: {} ({} lines)", out_path, lines);

    // Spine events — AC for #1808
    let log_path = format!("{}/chorus/platform/logs/chorus.log", repo);
    let eastern_offset = {
        let out = std::process::Command::new("date").args(["+%z"]).env("TZ", "America/New_York").output();
        out.ok().and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| {
                let s = s.trim();
                if s.len() >= 5 {
                    let sign = if s.starts_with('-') { -1 } else { 1 };
                    let h: i32 = s[1..3].parse().unwrap_or(5);
                    let m: i32 = s[3..5].parse().unwrap_or(0);
                    chrono::FixedOffset::east_opt(sign * (h * 3600 + m * 60))
                } else { None }
            })
            .unwrap_or_else(|| chrono::FixedOffset::west_opt(5 * 3600).unwrap())
    };
    let ts = chrono::Utc::now().with_timezone(&eastern_offset).format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();

    if !failed_sources.is_empty() {
        let event = serde_json::json!({
            "timestamp": ts,
            "level": "warn",
            "appName": "chorus-events",
            "component": "context-cache",
            "event": "session.context.error",
            "role": role,
            "failed_sources": failed_sources.join(","),
            "ok_sources": ok_sources.join(","),
            "lines": lines,
        });
        eprintln!("session.context.error | {} — failed: {}", role, failed_sources.join(", "));
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(&log_path)
            .and_then(|mut f| { use std::io::Write; writeln!(f, "{}", event) });
    }

    // Always emit success with source inventory
    let event = serde_json::json!({
        "timestamp": ts,
        "level": "info",
        "appName": "chorus-events",
        "component": "context-cache",
        "event": "session.context.built",
        "role": role,
        "sources": ok_sources.join(","),
        "failed": failed_sources.join(","),
        "lines": lines,
    });
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(&log_path)
        .and_then(|mut f| { use std::io::Write; writeln!(f, "{}", event) });

    ExitCode::SUCCESS
}

/// Fetch memory context from Chorus API for WIP card domains
fn fetch_memory_context(board_ts: &str, role: &str) -> String {
    use std::process::Command as Cmd;

    // Get WIP card IDs
    let mine_out = Cmd::new("zsh").arg("-lc").arg(format!("{} mine {}", board_ts, role))
        .output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();

    let wip_ids: Vec<String> = mine_out.lines()
        .filter(|l| l.to_lowercase().contains("[wip]"))
        .filter_map(|l| l.split_whitespace().find(|w| w.parse::<u32>().is_ok()).map(|s| s.to_string()))
        .take(3).collect();

    if wip_ids.is_empty() {
        return "No WIP cards — no memory context to load.".to_string();
    }

    // Get domains from WIP cards
    let mut domains = std::collections::HashSet::new();
    for cid in &wip_ids {
        let info = Cmd::new("zsh").arg("-lc").arg(format!("{} view {}", board_ts, cid))
            .output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();
        for part in info.split_whitespace() {
            if part.starts_with("domain:") {
                domains.insert(part.replace("domain:", ""));
            }
        }
    }

    if domains.is_empty() { return String::new(); }

    // Query Chorus API for each domain
    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for domain in &domains {
        let url = format!("http://localhost:3340/api/chorus/search?q={}&limit=5", domain);
        let resp = Cmd::new("curl").args(["-s", "--max-time", "3", &url])
            .output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();

        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&resp) {
            if let Some(arr) = v.get("results").and_then(|r| r.as_array()) {
                for r in arr.iter().take(5) {
                    let src = r.get("source").and_then(|s| s.as_str()).unwrap_or("");
                    let content = r.get("content").and_then(|s| s.as_str()).unwrap_or("");
                    let ts = r.get("timestamp").and_then(|s| s.as_str()).unwrap_or("");
                    let key: String = content.chars().take(60).collect();
                    if seen.contains(&key) { continue; }
                    seen.insert(key);

                    let ts_short: String = ts.chars().take(10).collect();
                    let content_short: String = content.chars().take(120).collect();
                    let content_clean = content_short.replace('\n', " ");

                    match src {
                        "memory" | "state" | "story" | "decision" | "brief" | "adr" | "artifact" | "spine" => {
                            results.push(format!("  - [{}] [{}] {}", ts_short, src, content_clean));
                        }
                        "claude" => {
                            let author = r.get("author").and_then(|a| a.as_str()).unwrap_or("");
                            if author == "assistant" && content.len() > 40
                                && !content.starts_with('<') && !content.starts_with('{')
                                && !content.starts_with('[') && !content.starts_with('/')
                                && !content.starts_with("bash ") && !content.starts_with("curl ")
                            {
                                results.push(format!("  - [{}] [session] {}", ts_short, content_clean));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    results.sort();
    results.dedup();
    results.truncate(10);
    results.join("\n")
}

/// Check handoff staleness from handoffs.log
fn check_handoffs(log_path: &str, role: &str, briefs_dir: &str, archive_dir: &str) -> String {
    use std::fs;

    let content = match fs::read_to_string(log_path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    let mut received_ids = std::collections::HashSet::new();
    let mut events = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if v.get("status").and_then(|s| s.as_str()) == Some("received") {
                if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                    received_ids.insert(id.to_string());
                }
            }
            events.push(v);
        }
    }

    let mut output = Vec::new();
    let mut pending_count = 0u32;
    let mut stale_count = 0u32;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();

    for e in &events {
        if e.get("status").and_then(|s| s.as_str()) != Some("sent") { continue; }
        if e.get("to").and_then(|t| t.as_str()) != Some(role) { continue; }
        let id = match e.get("id").and_then(|i| i.as_str()) { Some(i) => i, None => continue };
        if received_ids.contains(id) { continue; }

        pending_count += 1;

        if let Some(ts_str) = e.get("timestamp").and_then(|t| t.as_str()) {
            // Simple age check — parse ISO timestamp
            if let Ok(dt) = chrono_parse_approx(ts_str) {
                let age_hours = (now.saturating_sub(dt)) / 3600;
                if age_hours > 4 {
                    stale_count += 1;
                    let from = e.get("from").and_then(|f| f.as_str()).unwrap_or("?");
                    let artifact = e.get("artifact").and_then(|a| a.as_str())
                        .map(|p| p.rsplit('/').next().unwrap_or(p)).unwrap_or("?");
                    output.push(format!("STALE ({}h): {} from {} - {}", age_hours,
                        e.get("type").and_then(|t| t.as_str()).unwrap_or("?"), from, artifact));
                }
            }
        }
    }

    // Stale workflow briefs
    let mut stale_brief_count = 0u32;
    if let Ok(entries) = std::fs::read_dir(briefs_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.contains("wf-") && name.contains("step") {
                // Check if workflow is archived
                if let Some(wf_num) = name.split("wf-").nth(1).and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next()) {
                    let archive_path = format!("{}/WF-{}.json", archive_dir, wf_num.trim_start_matches('0'));
                    if std::path::Path::new(&archive_path).exists() {
                        stale_brief_count += 1;
                    }
                }
            }
        }
    }

    if pending_count == 0 && stale_brief_count == 0 {
        return String::new();
    }

    let mut parts = Vec::new();
    if pending_count > 0 { parts.push(format!("{} pending", pending_count)); }
    if stale_count > 0 { parts.push(format!("{} stale handoff(s)", stale_count)); }
    if stale_brief_count > 0 { parts.push(format!("{} stale brief(s)", stale_brief_count)); }
    output.push(format!("SUMMARY:{}", parts.join(", ")));
    output.join("\n")
}

/// Approximate ISO timestamp parse → unix seconds (no chrono crate)
fn chrono_parse_approx(ts: &str) -> Result<u64, ()> {
    // Parse "2026-03-22T21:29:55Z" or similar
    let clean = ts.replace('Z', "+00:00");
    let parts: Vec<&str> = clean.split('T').collect();
    if parts.len() < 2 { return Err(()); }
    let date_parts: Vec<u32> = parts[0].split('-').filter_map(|s| s.parse().ok()).collect();
    let time_str = parts[1].split('+').next().unwrap_or("00:00:00");
    let time_parts: Vec<u32> = time_str.split(':').filter_map(|s| s.parse().ok()).collect();
    if date_parts.len() < 3 || time_parts.len() < 2 { return Err(()); }

    // Rough unix timestamp calculation (good enough for age comparison)
    let year = date_parts[0];
    let month = date_parts[1];
    let day = date_parts[2];
    let hour = time_parts[0];
    let min = time_parts[1];

    // Days since epoch (rough)
    let mut days: u64 = 0;
    for y in 1970..year { days += if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 366 } else { 365 }; }
    let month_days = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month { days += month_days[m as usize] as u64; }
    if month > 2 && year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) { days += 1; }
    days += (day - 1) as u64;

    Ok(days * 86400 + hour as u64 * 3600 + min as u64 * 60)
}

/// Hourly health checks — replaces context-cache-hourly.sh (#1622)
fn health_hourly_cmd(args: &[String]) -> ExitCode {
    use std::fs;

    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    if !matches!(role, "wren" | "silas" | "kade") {
        eprintln!("Usage: chorus-hook-shim health-hourly <role>");
        return ExitCode::from(1);
    }

    let repo = "/Users/jeffbridwell/CascadeProjects";
    let role_dir = match role {
        "wren" => "product-manager",
        "silas" => "architect",
        "kade" => "engineer",
        _ => unreachable!(),
    };

    // Disk check — APFS-aware via Finder free space (includes purgeable, matches Finder)
    let disk_pct: u32 = {
        let finder_free = std::process::Command::new("osascript")
            .args(["-e", "tell application \"Finder\" to get free space of startup disk"])
            .output().ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse::<f64>().ok());
        let container_total = std::process::Command::new("diskutil").args(["info", "/"])
            .output().ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|text| {
                text.lines()
                    .find(|l| l.contains("Container Total Space"))
                    .and_then(|l| l.split('(').nth(1))
                    .and_then(|s| s.split_whitespace().next())
                    .and_then(|n| n.parse::<u64>().ok())
            });
        match (container_total, finder_free) {
            (Some(total), Some(free)) if total > 0 => {
                ((total as f64 - free) / total as f64 * 100.0) as u32
            }
            _ => 0,
        }
    };
    if disk_pct > 95 { eprintln!("CRITICAL: disk at {}%", disk_pct); }
    else if disk_pct > 90 { eprintln!("WARNING: disk at {}%", disk_pct); }

    // Cost log check
    let today = process::wall_clock().chars().take(10).collect::<String>();
    let cost_path = format!("{}/chorus/cost-log.md", repo);
    if let Ok(content) = fs::read_to_string(&cost_path) {
        if !content.contains(&today) {
            eprintln!("WARNING: no cost entry for today");
        }
    }

    // Activity.md recency
    let activity_path = format!("{}/chorus/activity.md", repo);
    if let Ok(meta) = fs::metadata(&activity_path) {
        if let Ok(modified) = meta.modified() {
            let age_h = modified.elapsed().unwrap_or_default().as_secs() / 3600;
            if age_h > 24 { eprintln!("WARNING: activity.md not updated in {}h", age_h); }
        }
    }

    // Uncommitted files
    let uncommitted = std::process::Command::new("git")
        .args(["-C", repo, "status", "--porcelain", &format!("{}/", role_dir)])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.lines().count())
        .unwrap_or(0);
    if uncommitted > 5 { eprintln!("WARNING: {} uncommitted files in {}/", uncommitted, role_dir); }

    // Recurring errors
    let error_log = format!("{}/chorus/proving/logs/command-errors.log", repo);
    if let Ok(content) = fs::read_to_string(&error_log) {
        let mut fps: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for line in content.lines() {
            if !line.contains(&format!("\"date\":\"{}\"", today)) { continue; }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(fp) = v.get("fingerprint").and_then(|f| f.as_str()) {
                    *fps.entry(fp.to_string()).or_insert(0) += 1;
                }
            }
        }
        for (fp, count) in &fps {
            if *count >= 3 { eprintln!("WARNING: error {} repeated {}x today", fp, count); }
        }
    }

    println!("Hourly check complete: disk={}% uncommitted={}", disk_pct, uncommitted);
    ExitCode::SUCCESS
}

/// Daily health checks — replaces context-cache-daily.sh (#1622)
fn health_daily_cmd(args: &[String]) -> ExitCode {
    use std::fs;

    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    if !matches!(role, "wren" | "silas" | "kade") {
        eprintln!("Usage: chorus-hook-shim health-daily <role>");
        return ExitCode::from(1);
    }

    let repo = "/Users/jeffbridwell/CascadeProjects";
    let role_dir_path = match role {
        "wren" => format!("{}/product-manager", repo),
        "silas" => format!("{}/architect", repo),
        "kade" => format!("{}/engineer", repo),
        _ => unreachable!(),
    };

    // Doc freshness (4h window)
    let session_window = 14400u64;
    let check_fresh = |path: &str, label: &str| {
        if let Ok(meta) = fs::metadata(path) {
            if let Ok(modified) = meta.modified() {
                let age = modified.elapsed().unwrap_or_default().as_secs();
                if age > session_window {
                    eprintln!("WARNING: {} stale ({}h)", label, age / 3600);
                }
            }
        }
    };
    match role {
        "silas" => {
            check_fresh(&format!("{}/system-architecture.md", role_dir_path), "system-architecture.md");
            check_fresh(&format!("{}/ontology-status.md", role_dir_path), "ontology-status.md");
        }
        "wren" => {
            check_fresh(&format!("{}/decisions.md", role_dir_path), "decisions.md");
            check_fresh(&format!("{}/backlog.md", role_dir_path), "backlog.md");
        }
        "kade" => {
            check_fresh(&format!("{}/current-work.md", role_dir_path), "current-work.md");
            check_fresh(&format!("{}/tech-debt.md", role_dir_path), "tech-debt.md");
        }
        _ => {}
    }

    // CLAUDE.md staleness
    let claude_path = format!("{}/CLAUDE.md", role_dir_path);
    if let Ok(meta) = fs::metadata(&claude_path) {
        if let Ok(modified) = meta.modified() {
            let age_days = modified.elapsed().unwrap_or_default().as_secs() / 86400;
            if age_days > 3 { eprintln!("WARNING: CLAUDE.md stale ({} days)", age_days); }
        }
    }

    // Stale briefs
    let briefs_dir = format!("{}/briefs", role_dir_path);
    if let Ok(entries) = fs::read_dir(&briefs_dir) {
        let stale_count = entries.flatten().filter(|e| {
            e.path().extension().map(|ext| ext == "md").unwrap_or(false)
                && e.metadata().ok()
                    .and_then(|m| m.modified().ok())
                    .map(|t| t.elapsed().unwrap_or_default().as_secs() > 7 * 86400)
                    .unwrap_or(false)
        }).count();
        if stale_count > 0 { eprintln!("INFO: {} briefs older than 7 days in {} inbox", stale_count, role); }
    }

    // Git log summary
    let _ = std::process::Command::new("git")
        .args(["-C", repo, "log", "--oneline", "--since=24 hours ago"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| fs::write(format!("/tmp/git-daily-{}.txt", role), s));

    println!("Daily check complete for {}", role);
    ExitCode::SUCCESS
}

/// Log rotation — replaces log-rotate.sh (#1622)
fn log_rotate_cmd() -> ExitCode {
    use std::fs;
    use std::process::Command as Cmd;

    let log_dir = "/Users/jeffbridwell/CascadeProjects/chorus/platform/logs";
    let max_size: u64 = 10 * 1024 * 1024; // 10MB
    let keep_rotations = 3u32;

    let logs = ["chorus.log", "permission-prompts.log", "command-errors.log", "handoffs.log"];

    println!("=== Log rotation {} ===", process::wall_clock().chars().take(16).collect::<String>());

    for name in &logs {
        let path = format!("{}/{}", log_dir, name);
        let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let size_mb = size / 1_048_576;

        if size < max_size { continue; }

        println!("Rotating {} ({}MB > {}MB threshold)", name, size_mb, max_size / 1_048_576);

        // Shift existing rotations
        for i in (1..keep_rotations).rev() {
            let from = format!("{}.{}.gz", path, i);
            let to = format!("{}.{}.gz", path, i + 1);
            let _ = fs::rename(&from, &to);
        }

        // Compress current as .1.gz
        let gz_path = format!("{}.1.gz", path);
        let _ = Cmd::new("gzip").args(["-c", &path]).output().and_then(|o| {
            fs::write(&gz_path, &o.stdout)
        });

        // Truncate original
        let _ = fs::write(&path, "");
        println!("  → Compressed to {}.1.gz, truncated original", name);

        // Delete old rotations beyond limit
        for i in (keep_rotations + 1)..10 {
            let old = format!("{}.{}.gz", path, i);
            if fs::metadata(&old).is_ok() {
                let _ = fs::remove_file(&old);
                println!("  → Deleted old rotation: {}.{}.gz", name, i);
            }
        }
    }

    // Report sizes
    println!("Current log sizes:");
    if let Ok(entries) = fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().map(|e| e == "log").unwrap_or(false) {
                let size = fs::metadata(entry.path()).map(|m| m.len()).unwrap_or(0);
                println!("  {}: {}KB", entry.file_name().to_string_lossy(), size / 1024);
            }
        }
    }
    println!("=== Done ===");
    ExitCode::SUCCESS
}

/// Wall clock in Boston timezone — replaces wall-clock.sh (#1621)
fn wall_clock_cmd() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Skip "wall-clock" if passed as subcommand (vs argv[0] dispatch)
    let flag = args.iter()
        .find(|a| a.starts_with("--"))
        .map(|s| s.as_str())
        .unwrap_or("");
    match flag {
        "--iso" => println!("{}", process::wall_clock()),
        "--write" => {
            let ts = process::wall_clock();
            let short: String = ts.chars().take(16).collect();
            let _ = std::fs::write("/tmp/wall-clock.txt", format!("{}\n", short));
            println!("{}", short);
        }
        _ => {
            let ts = process::wall_clock();
            let short: String = ts.chars().take(16).collect();
            println!("{}", short);
        }
    }
    ExitCode::SUCCESS
}

/// Load missed observations from other roles — self-contained, no server needed.
/// Reads JSONL files directly from /tmp/claude-team-scan/.
fn observe_missed_cmd(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("Usage: chorus-hook-shim observe-missed <my-role> [since-ts]");
        return ExitCode::from(1);
    }

    let my_role = &args[0];
    // Default: last 2 hours
    let default_since = {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let two_hours_ago = now - 7200;
        // Convert to rough ISO format
        format!("{}",
            chrono_lite_ts(two_hours_ago))
    };
    let since = args.get(1).map(|s| s.as_str()).unwrap_or(&default_since);

    let roles = ["wren", "silas", "kade"];
    let mut all_obs: Vec<(String, String, String, String)> = Vec::new(); // (ts, role, digest, card)

    for role in &roles {
        if *role == my_role.as_str() {
            continue;
        }

        // Check if role is active (state file exists and recent)
        let state_file = format!("/tmp/claude-team-scan/{}-declared.json", role);
        let is_active = if let Ok(content) = std::fs::read_to_string(&state_file) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(ts) = parsed.get("ts").and_then(|v| v.as_u64()) {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    now.saturating_sub(ts) < 1800
                } else { false }
            } else { false }
        } else { false };

        if !is_active {
            continue;
        }

        let obs_file = format!("/tmp/claude-team-scan/{}-observations.jsonl", role);
        if let Ok(content) = std::fs::read_to_string(&obs_file) {
            for line in content.lines() {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(line) {
                    let ts = parsed.get("ts").and_then(|v| v.as_str()).unwrap_or("");
                    if ts > since {
                        let digest = parsed.get("digest").and_then(|v| v.as_str()).unwrap_or("");
                        let card = parsed.get("card").and_then(|v| {
                            if v.is_string() { v.as_str().map(|s| s.to_string()) }
                            else if v.is_number() { Some(v.to_string()) }
                            else { None }
                        }).unwrap_or_default();
                        all_obs.push((
                            ts.to_string(),
                            role.to_string(),
                            digest.to_string(),
                            card,
                        ));
                    }
                }
            }
        }
    }

    if all_obs.is_empty() {
        return ExitCode::SUCCESS;
    }

    all_obs.sort_by(|a, b| a.0.cmp(&b.0));

    // Group by role and format
    println!("## Cross-role activity since last session\n");
    let mut current_role = String::new();
    for (ts, role, digest, card) in &all_obs {
        if role != &current_role {
            if !current_role.is_empty() {
                println!();
            }
            current_role = role.clone();
            println!("**{}**:", role);
        }
        let ts_short = if ts.len() >= 16 { &ts[11..16] } else { ts };
        let card_label = if card.is_empty() { String::new() } else { format!(" [#{}]", card) };
        println!("  - {} {}{}", ts_short, digest, card_label);
    }
    println!();

    ExitCode::SUCCESS
}

/// Minimal timestamp from unix epoch (no chrono dependency in shim)
fn chrono_lite_ts(epoch_secs: u64) -> String {
    // Approximate ISO timestamp — good enough for comparison
    let days = epoch_secs / 86400;
    let secs_in_day = epoch_secs % 86400;
    let hours = secs_in_day / 3600;
    let mins = (secs_in_day % 3600) / 60;
    let secs = secs_in_day % 60;

    // Days since epoch to Y-M-D (simplified, good enough for 2026)
    let mut y = 1970u64;
    let mut remaining = days;
    loop {
        let days_in_year = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 366 } else { 365 };
        if remaining < days_in_year { break; }
        remaining -= days_in_year;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let month_days: [u64; 12] = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0;
    while m < 12 && remaining >= month_days[m] {
        remaining -= month_days[m];
        m += 1;
    }
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m + 1, remaining + 1, hours, mins, secs)
}

fn decode_chunked(body: &str) -> String {
    let mut result = String::new();
    let mut remaining = body;

    loop {
        // Find chunk size line
        let size_end = match remaining.find("\r\n") {
            Some(pos) => pos,
            None => break,
        };
        let size_str = remaining[..size_end].trim();
        let size = match usize::from_str_radix(size_str, 16) {
            Ok(s) => s,
            Err(_) => break,
        };
        if size == 0 {
            break;
        }
        let chunk_start = size_end + 2;
        let chunk_end = chunk_start + size;
        if chunk_end > remaining.len() {
            break;
        }
        result.push_str(&remaining[chunk_start..chunk_end]);
        remaining = &remaining[chunk_end..];
        if remaining.starts_with("\r\n") {
            remaining = &remaining[2..];
        }
    }
    result
}
