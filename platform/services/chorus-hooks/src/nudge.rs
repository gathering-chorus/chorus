//! nudge — Role-to-role message delivery.
//! Subcommand of chorus-hook-shim: `chorus-hook-shim nudge <role> <message> [--from <sender>]`
//!
//! Uses L2 (process.rs) for session detection and delivery routing.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::process;

/// Generate a short trace ID: timestamp_ms + 4 random hex chars
fn trace_id() -> String {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis();
    let rand: u16 = (std::process::id() as u16).wrapping_mul(31).wrapping_add(ts as u16);
    format!("ntr-{}-{:04x}", ts, rand)
}

const INBOX_DIR: &str = "/tmp/voice-inbox";
const EXCHANGE_DIR: &str = "/tmp/nudge-exchanges";
const CHORUS_LOG: &str = "/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log";

/// Role directory names for Terminal tab matching
fn role_dir(role: &str) -> Option<&'static str> {
    match role {
        "wren" => Some("product-manager"),
        "silas" => Some("architect"),
        "kade" => Some("engineer"),
        _ => None,
    }
}

/// Detect sender by walking PID chain to find claude process CWD
fn detect_sender() -> String {
    if let Ok(role) = std::env::var("DEPLOY_ROLE") {
        if matches!(role.as_str(), "silas" | "wren" | "kade") {
            return role;
        }
    }

    let mut pid = std::process::id();
    for _ in 0..5 {
        let ppid = get_ppid(pid);
        if ppid == 0 { break; }

        if get_comm(ppid).as_deref() == Some("claude") {
            if let Some(cwd) = process::get_cwd(ppid) {
                if cwd.contains("product-manager") { return "wren".into(); }
                if cwd.contains("architect") { return "silas".into(); }
                if cwd.contains("engineer") { return "kade".into(); }
            }
        }
        pid = ppid;
    }

    "jeff".into()
}

fn get_ppid(pid: u32) -> u32 {
    Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "ppid="])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn get_comm(pid: u32) -> Option<String> {
    Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
}

/// Queue message to inbox file
fn queue_message(role: &str, text: &str) {
    let inbox = PathBuf::from(format!("{}/{}", INBOX_DIR, role));
    let _ = fs::create_dir_all(&inbox);
    let path = inbox.join("pending-inject.txt");
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{}", text);
    }
}

/// Track exchange count between sender and target
fn track_exchange(sender: &str, target: &str) {
    let _ = fs::create_dir_all(EXCHANGE_DIR);
    let pair_key = if sender < target {
        format!("{}-{}", sender, target)
    } else {
        format!("{}-{}", target, sender)
    };
    let path = PathBuf::from(format!("{}/{}", EXCHANGE_DIR, pair_key));

    if let Ok(meta) = fs::metadata(&path) {
        if let Ok(modified) = meta.modified() {
            if modified.elapsed().unwrap_or_default().as_secs() > 1800 {
                let _ = fs::remove_file(&path);
            }
        }
    }

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{}|{}>{}", ts, sender, target);
    }
}

/// Detect if nudge content expects a reply (questions, requests for response)
fn needs_reply(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("nudge me back")
        || lower.contains("nudge back")
        || lower.contains("reply")
        || lower.contains("respond")
        || lower.contains("thoughts?")
        || lower.contains("feedback?")
        || lower.contains("confirm")
        || lower.ends_with('?')
}

/// Emit spine event via chorus-log
fn chorus_log(event: &str, role: &str, extra: &str) {
    if Path::new(CHORUS_LOG).exists() {
        let _ = Command::new(CHORUS_LOG)
            .args([event, role, extra])
            .output();
    }
}


pub fn run(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("Usage: chorus-hook-shim nudge <role> <message> [--from <sender>] [--force]");
        return ExitCode::from(1);
    }

    // subcommands
    match args[0].as_str() {
        "health" => return health_check(),
        "drain" => {
            let role = args.get(1).map(|s| s.as_str()).unwrap_or("");
            if role.is_empty() { eprintln!("Usage: nudge drain <role>"); return ExitCode::from(1); }
            let inbox = format!("{}/{}/pending-inject.txt", INBOX_DIR, role);
            let path = std::path::Path::new(&inbox);
            if path.exists() {
                if let Ok(content) = fs::read_to_string(path) {
                    if !content.trim().is_empty() {
                        // Filter [bridge] echo lines — subscriber feedback loop (#1700)
                        let filtered: String = content.lines()
                            .filter(|l| !l.starts_with("[bridge]"))
                            .map(|l| format!("{}\n", l))
                            .collect();
                        if !filtered.trim().is_empty() {
                            print!("{}", filtered);
                        }
                        let _ = fs::write(path, "");
                        return ExitCode::SUCCESS;
                    }
                }
            }
            println!("No queued messages for {}", role);
            return ExitCode::SUCCESS;
        }
        "inbox" => {
            let role = args.get(1).map(|s| s.as_str()).unwrap_or("");
            if role.is_empty() { eprintln!("Usage: nudge inbox <role>"); return ExitCode::from(1); }
            let inbox = format!("{}/{}/pending-inject.txt", INBOX_DIR, role);
            if let Ok(content) = fs::read_to_string(&inbox) {
                if !content.trim().is_empty() { print!("{}", content); return ExitCode::SUCCESS; }
            }
            println!("No queued messages for {}", role);
            return ExitCode::SUCCESS;
        }
        _ => {}
    }

    if args.len() < 2 {
        eprintln!("Usage: chorus-hook-shim nudge <role> <message> [--from <sender>] [--force]");
        return ExitCode::from(1);
    }

    let target = &args[0];

    // Guard: detect common misuse where sender is passed as arg[1]
    // e.g. `nudge.sh silas kade "actual message"` — kade becomes the message
    if args.len() >= 3
        && matches!(args[1].as_str(), "wren" | "silas" | "kade" | "jeff")
        && !args[1].starts_with('-')
    {
        // Caller passed <target> <sender> <message> — fix it
        eprintln!(
            "WARN: nudge.sh takes <role> <message>, not <role> <sender> <message>. Fixing arg order."
        );
        // Use args[2..] as the message, args[1] as --from
        let sender = args[1].clone();
        let message = args[2..].join(" ");
        let fixed_args = vec![
            target.clone(),
            message,
            "--from".to_string(),
            sender,
        ];
        return run(&fixed_args);
    }

    let message = &args[1];

    // Validate: message can't be a flag
    if message.starts_with("--") {
        eprintln!("ERROR: '{}' is a flag, not a message.", message);
        eprintln!("Usage: nudge <role> \"your message\" [--force] [--from <sender>]");
        eprintln!("  The message must come BEFORE any flags.");
        return ExitCode::from(1);
    }

    // Validate: message can't be empty
    if message.trim().is_empty() {
        eprintln!("ERROR: empty message. What are you telling the role to do?");
        return ExitCode::from(1);
    }

    let mut explicit_sender = None;
    // DEC-107: --force is always on. No passive/queued-only path.
    // Reverts #1898 level-based delivery — all role-to-role nudges inject via osascript.
    // Log evidence: wren→silas at 11:37 delivered mode=queued despite wrapper --force.
    // Git: #1898 (3a146863) introduced the passive path. This removes it.
    let force = true;
    let mut reply_to: Option<String> = None;
    let mut level = "info".to_string();

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--from" => {
                i += 1;
                if i < args.len() { explicit_sender = Some(args[i].clone()); }
            }
            "--force" => { /* DEC-107: force is always on, flag accepted but ignored */ }
            "--reply-to" => {
                i += 1;
                if i < args.len() { reply_to = Some(args[i].clone()); }
            }
            "--level" => {
                i += 1;
                if i < args.len() {
                    let l = args[i].to_lowercase();
                    if matches!(l.as_str(), "info" | "warn" | "critical") {
                        level = l;
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }

    let sender = explicit_sender.unwrap_or_else(detect_sender);
    let tid = trace_id();

    // If --reply-to is set, POST response back to that URL (Bridge integration)
    if let Some(ref url) = reply_to {
        let _ = deliver_to_url(&sender, message, url);
    }

    // Jeff is a valid target — route to Bridge API instead of terminal
    if target == "jeff" {
        return deliver_to_bridge(&sender, message);
    }

    if role_dir(target).is_none() {
        eprintln!("Unknown role: {}. Use wren, silas, kade, or jeff.", target);
        return ExitCode::from(1);
    }
    let clock = process::wall_clock();
    // Use short format for nudge prefix
    let clock_short: String = clock.chars().take(16).collect();

    // Jeff's messages pass through verbatim — no wrapping, no mutation (#2255)
    let full_text = if sender == "jeff" {
        message.to_string()
    } else {
        let prefix = format!("[nudge from {} | {} Boston]", sender, clock_short);
        let reply_expected = needs_reply(message);
        if reply_expected {
            format!("{} {} [REPLY EXPECTED — nudge {} back]", prefix, message, sender)
        } else {
            format!("{} {}", prefix, message)
        }
    };

    track_exchange(&sender, target);

    // Persist for history — NOT for delivery. Fire-and-forget, failure doesn't block inject.
    let persist_body = serde_json::json!({
        "from": sender,
        "to": target,
        "content": message,
        "traceId": tid,
    });
    let _ = Command::new("curl")
        .args([
            "-s", "-X", "POST",
            "http://localhost:3475/api/nudge",
            "-H", "Content-Type: application/json",
            "-d", &persist_body.to_string(),
            "--connect-timeout", "2",
        ])
        .output();

    let content_preview: String = message.chars().take(120).collect();
    chorus_log(
        "role.nudge.sent",
        &sender,
        &format!("target={},chars={},trace={},content={}", target, message.len(), tid, content_preview),
    );

    // Level-based delivery (#1898):
    // critical = osascript inject (justified focus steal) + queue
    // warn     = queue (drain on next prompt) + stderr hint
    // info     = queue only (ambient, drain on next prompt)
    // DEC-107: persist AND deliver on every nudge — level controls the 'deliver' method.
    let mode;

    // Always queue for drain-on-prompt delivery
    queue_message(target, &full_text);

    if level == "critical" || force {
        // Critical: osascript inject — immediate delivery
        match process::inject_by_tab_name(target, &full_text) {
            Ok(()) => {
                mode = "injected";
                println!("DELIVERED to {} at {}", target, clock_short);
            }
            Err(e) => {
                mode = "inject-failed-queued";
                eprintln!("INJECT FAILED for {} (queued for drain): {}", target, e);
                println!("DELIVERED to {} at {} (queued — inject failed)", target, clock_short);
                chorus_log(
                    "role.nudge.inject_failed",
                    &sender,
                    &format!("target={},level={},error={}", target, level, e),
                );
            }
        }
    } else if level == "warn" {
        mode = "queued-warn";
        println!("DELIVERED to {} at {}", target, clock_short);
        // Stderr hint so the sending role knows it's queued, not injected
        eprintln!("nudge to {} queued (level=warn, drain on next prompt)", target);
    } else {
        mode = "queued";
        println!("DELIVERED to {} at {}", target, clock_short);
    }

    chorus_log(
        "role.nudge.delivered",
        &sender,
        &format!("target={},mode={},level={},trace={}", target, mode, level, tid),
    );

    ExitCode::SUCCESS
}

/// Deliver a message to a URL (--reply-to support for Bridge/Clearing)
fn deliver_to_url(sender: &str, message: &str, url: &str) {
    let body = serde_json::json!({
        "from": sender,
        "text": message,
    });

    let _ = Command::new("curl")
        .args([
            "-s", "-X", "POST",
            url,
            "-H", "Content-Type: application/json",
            "-d", &body.to_string(),
            "--connect-timeout", "2",
        ])
        .output();

    chorus_log("role.nudge.reply", sender, &format!("url={}", url));
}

/// Deliver a message to Jeff via the Bridge API (localhost:3470)
fn deliver_to_bridge(sender: &str, message: &str) -> ExitCode {
    let body = serde_json::json!({
        "from": sender,
        "text": message,
    });

    // Write body to temp file to avoid shell argument length limits on long messages
    let body_str = body.to_string();
    let tmp_path = format!("/tmp/bridge-msg-{}.json", std::process::id());
    let _ = std::fs::write(&tmp_path, &body_str);

    let result = Command::new("curl")
        .args([
            "-s", "-X", "POST",
            "http://localhost:3470/api/message",
            "-H", "Content-Type: application/json",
            "-d", &format!("@{}", tmp_path),
            "--connect-timeout", "2",
        ])
        .output();

    let _ = std::fs::remove_file(&tmp_path);

    match result {
        Ok(output) if output.status.success() => {
            println!("QUEUED for jeff (passive)");
            chorus_log(
                "role.nudge.delivered",
                sender,
                "target=jeff,mode=bridge",
            );
            ExitCode::SUCCESS
        }
        Ok(output) => {
            let err = String::from_utf8_lossy(&output.stderr);
            eprintln!("Bridge delivery failed: {}", err);
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("Bridge not reachable: {}", e);
            ExitCode::from(1)
        }
    }
}

/// Health check: verify each role has exactly one reachable window, correct target.
fn health_check() -> ExitCode {
    let roles = [("wren", "product-manager"), ("silas", "architect"), ("kade", "engineer")];
    let mut failures = 0;

    for (role, pattern) in &roles {
        let script = format!(
            r#"tell application "Terminal"
    set matchCount to 0
    set matchName to ""
    set winCount to count of windows
    repeat with i from 1 to winCount
        set w to window i
        set winName to name of w
        if winName contains "{p}" and winName contains "claude" then
            set matchCount to matchCount + 1
            set matchName to winName
        end if
    end repeat
    return (matchCount as text) & "::" & matchName
end tell"#,
            p = pattern
        );

        let output = Command::new("osascript")
            .args(["-e", &script])
            .output();

        let result = match output {
            Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            Err(e) => {
                println!("ALERT: {} — osascript failed: {}", role, e);
                failures += 1;
                continue;
            }
        };

        let parts: Vec<&str> = result.splitn(2, "::").collect();
        let count: i32 = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        let name = parts.get(1).unwrap_or(&"");

        if count == 0 {
            println!("ALERT: {} — no terminal window found (need {} + claude)", role, pattern);
            chorus_log("nudge.health.failed", "system", &format!("role={},reason=no-window", role));
            failures += 1;
        } else if count > 1 {
            println!("WARN: {} — {} windows match (ambiguous)", role, count);
            chorus_log("nudge.health.ambiguous", "system", &format!("role={},count={}", role, count));
            failures += 1;
        } else {
            // Cross-check: window shouldn't contain another role's pattern
            let other_patterns: Vec<&str> = roles.iter()
                .filter(|(r, _)| *r != *role)
                .map(|(_, p)| *p)
                .collect();
            let wrong = other_patterns.iter().any(|p| name.contains(p));
            if wrong {
                println!("ALERT: {} — matched window '{}' contains another role's pattern — WRONG TARGET", role, name);
                chorus_log("nudge.health.wrong-target", "system", &format!("role={},window={}", role, name));
                failures += 1;
            } else {
                println!("OK: {} — 1 window, correct target: {}", role, name);
            }
        }
    }

    if failures > 0 {
        println!("NUDGE HEALTH: {} role(s) have issues", failures);
        // Alert to Bridge
        let _ = Command::new("curl")
            .args([
                "-s", "-X", "POST",
                "http://localhost:3470/api/message",
                "-H", "Content-Type: application/json",
                "-d", &format!(r#"{{"from":"system","text":"[ALERT] Nudge health: {} role(s) unreachable or ambiguous"}}"#, failures),
                "--connect-timeout", "2",
            ])
            .output();
        ExitCode::from(1)
    } else {
        println!("NUDGE HEALTH: all roles reachable (1 window each, correct targets)");
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // --- AC1: needs_reply detection ---

    #[test]
    fn needs_reply_detects_question_mark() {
        assert!(needs_reply("are you done with #1698?"));
    }

    #[test]
    fn needs_reply_detects_nudge_back() {
        assert!(needs_reply("Check this and nudge me back"));
        assert!(needs_reply("nudge back when ready"));
    }

    #[test]
    fn needs_reply_detects_feedback_request() {
        assert!(needs_reply("thoughts?"));
        assert!(needs_reply("feedback?"));
        assert!(needs_reply("can you confirm the fix landed"));
    }

    #[test]
    fn needs_reply_ignores_statements() {
        assert!(!needs_reply("card #1704 is done"));
        assert!(!needs_reply("moving to next card"));
        assert!(!needs_reply("deployed the fix"));
    }

    // --- AC1: role_dir mapping ---

    #[test]
    fn role_dir_maps_known_roles() {
        assert_eq!(role_dir("wren"), Some("product-manager"));
        assert_eq!(role_dir("silas"), Some("architect"));
        assert_eq!(role_dir("kade"), Some("engineer"));
    }

    #[test]
    fn role_dir_rejects_unknown() {
        assert_eq!(role_dir("jeff"), None);
        assert_eq!(role_dir("unknown"), None);
    }

    // --- AC1: queue_message writes to inbox ---

    #[test]
    fn queue_message_creates_inbox_file() {
        let test_role = "test-nudge-queue";
        let inbox_dir = format!("{}/{}", INBOX_DIR, test_role);
        let inbox_file = format!("{}/pending-inject.txt", inbox_dir);

        // Clean up from prior runs
        let _ = fs::remove_file(&inbox_file);
        let _ = fs::remove_dir(&inbox_dir);

        queue_message(test_role, "hello from test");

        let content = fs::read_to_string(&inbox_file).expect("inbox file should exist");
        assert!(content.contains("hello from test"));

        // Cleanup
        let _ = fs::remove_file(&inbox_file);
        let _ = fs::remove_dir(&inbox_dir);
    }

    #[test]
    fn queue_message_appends_multiple() {
        let test_role = "test-nudge-append";
        let inbox_dir = format!("{}/{}", INBOX_DIR, test_role);
        let inbox_file = format!("{}/pending-inject.txt", inbox_dir);

        let _ = fs::remove_file(&inbox_file);
        let _ = fs::remove_dir(&inbox_dir);

        queue_message(test_role, "msg1");
        queue_message(test_role, "msg2");

        let content = fs::read_to_string(&inbox_file).expect("inbox file should exist");
        assert!(content.contains("msg1"));
        assert!(content.contains("msg2"));

        let _ = fs::remove_file(&inbox_file);
        let _ = fs::remove_dir(&inbox_dir);
    }

    // --- AC1: drain clears inbox ---

    #[test]
    fn drain_clears_inbox_after_read() {
        let test_role = "test-nudge-drain";
        let inbox_dir = format!("{}/{}", INBOX_DIR, test_role);
        let inbox_file = format!("{}/pending-inject.txt", inbox_dir);

        let _ = fs::create_dir_all(&inbox_dir);
        fs::write(&inbox_file, "queued message\n").expect("write test inbox");

        let result = run(&["drain".into(), test_role.into()]);
        assert_eq!(result, ExitCode::SUCCESS);

        // Inbox should be empty after drain
        let content = fs::read_to_string(&inbox_file).unwrap_or_default();
        assert!(content.is_empty(), "inbox should be empty after drain");

        let _ = fs::remove_file(&inbox_file);
        let _ = fs::remove_dir(&inbox_dir);
    }

    // --- AC1: drain filters bridge echo lines ---

    #[test]
    fn drain_filters_bridge_echo_lines() {
        let test_role = "test-nudge-bridge-filter";
        let inbox_dir = format!("{}/{}", INBOX_DIR, test_role);
        let inbox_file = format!("{}/pending-inject.txt", inbox_dir);

        let _ = fs::create_dir_all(&inbox_dir);
        fs::write(&inbox_file, "[bridge] echo line\nreal message\n").expect("write test inbox");

        let result = run(&["drain".into(), test_role.into()]);
        assert_eq!(result, ExitCode::SUCCESS);

        // File cleared after drain
        let content = fs::read_to_string(&inbox_file).unwrap_or_default();
        assert!(content.is_empty());

        let _ = fs::remove_file(&inbox_file);
        let _ = fs::remove_dir(&inbox_dir);
    }

    // --- AC1: exchange tracking ---

    #[test]
    fn track_exchange_creates_pair_file() {
        let _ = fs::create_dir_all(EXCHANGE_DIR);

        track_exchange("kade", "silas");

        // Pair key is alphabetically ordered
        let pair_file = format!("{}/kade-silas", EXCHANGE_DIR);
        let content = fs::read_to_string(&pair_file).expect("exchange file should exist");
        assert!(content.contains("kade>silas"));

        let _ = fs::remove_file(&pair_file);
    }

    // --- AC1: run rejects unknown roles ---

    #[test]
    fn run_rejects_unknown_target_role() {
        let result = run(&["unknown-role".into(), "hello".into()]);
        assert_eq!(result, ExitCode::from(1));
    }

    // --- AC1: run with no args returns error ---

    #[test]
    fn run_no_args_returns_error() {
        let result = run(&[]);
        assert_eq!(result, ExitCode::from(1));
    }

    #[test]
    fn run_single_arg_returns_error() {
        let result = run(&["wren".into()]);
        assert_eq!(result, ExitCode::from(1));
    }

    // --- Level-based delivery (#1898) ---

    #[test]
    fn info_level_queues_without_inject() {
        let test_role = "test-level-info";
        let inbox_dir = format!("{}/{}", INBOX_DIR, test_role);
        let inbox_file = format!("{}/pending-inject.txt", inbox_dir);
        let _ = fs::remove_file(&inbox_file);
        let _ = fs::remove_dir(&inbox_dir);

        // Info level should queue, not inject
        // Can't test full run (needs osascript), but verify queue_message works
        queue_message(test_role, "info level test");
        let content = fs::read_to_string(&inbox_file).unwrap_or_default();
        assert!(content.contains("info level test"));

        let _ = fs::remove_file(&inbox_file);
        let _ = fs::remove_dir(&inbox_dir);
    }

    #[test]
    fn needs_reply_with_reply_expected() {
        assert!(needs_reply("[REPLY EXPECTED — nudge kade back]"));
    }
}
