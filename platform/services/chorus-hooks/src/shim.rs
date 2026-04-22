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
pub mod shared;
mod commands;

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

        // #2435 — "inject" subcommand retired alongside inject_by_tab_name.
        // Nudge delivery is the spine-tick-poller reading chorus.log; callers
        // that need raw osascript (pair-enforce only) invoke chorus-inject
        // binary directly.

        // --- Ops + workflow ---
        "chorus-ops" | "ops" => return ops::run(&args),
        "workflow" => return workflow_ts_cmd(),

        // --- Scheduled tasks (extracted to commands/) ---
        "health-hourly" | "context-cache-hourly" => return commands::health::health_hourly(&args),
        "health-daily" | "context-cache-daily" => return commands::health::health_daily(&args),
        "health-weekly" | "context-cache-weekly" => return commands::health::health_weekly(&args),
        "context-cache" | "context-cache-5min" => return commands::context_cache::run(&args),
        "pulse" => return commands::pulse::run(&args),
        "session-start" | "session-start-thin" => return commands::session::session_start_cmd(&args),
        "session-close" | "session-close-thin" | "session-end-hook" => return commands::session::session_close_cmd(&args),

        // --- Utilities ---
        "log-rotate" => return commands::health::log_rotate(),
        "cruft-scan" => return commands::health::cruft_scan(),
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

    // #2283: PostToolUse drain removed. UserPromptSubmit is the single drain point.
    // Draining here caused duplicate delivery when both fired on the same queued message.

    ExitCode::from(exit_code as u8)
}

/// CLAUDE.md generator — replaces claudemd-gen.sh (#1624)
/// Bash eliminated; Python logic extracted to claudemd-gen.py, called directly.
fn claudemd_gen_cmd() -> ExitCode {
    let repo = shared::state_paths::chorus_root();
    let script = format!("{}/platform/scripts/claudemd-gen.py", repo);
    let claudemd_dir = format!("{}/designing/claudemd", repo);
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



/// Generic Python dispatch — run a .py script with remaining args (#1624)
/// Dispatch to TypeScript workflow engine (replaces workflow.py per DEC-100 / #1775)
fn workflow_ts_cmd() -> ExitCode {
    let cli_path = format!("{}/platform/workflow-engine/dist/cli.js", shared::state_paths::chorus_root());
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
        ("fuseki", "http://localhost:3030/$/ping", "error"),
        ("clearing", "http://localhost:3470/", "error"),
        ("chorus-api", "http://localhost:3340/health", "error"),
        ("vikunja", "http://localhost:3456/", "error"),
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

    let repo = shared::state_paths::chorus_root();
    let role_dir = match role { "wren" => "roles/wren", "silas" => "roles/silas", "kade" => "roles/kade", _ => unreachable!() };
    let checkpoint = format!("/tmp/role-checkpoint-{}.json", role);
    let board_ts = format!("{}/platform/scripts/cards", repo);

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
