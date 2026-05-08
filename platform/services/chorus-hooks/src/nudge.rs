//! nudge — Role-to-role message delivery.
//! Subcommand of chorus-hook-shim: `chorus-hook-shim nudge <role> <message> [--from <sender>]`
//!
//! Uses L2 (process.rs) for session detection and delivery routing.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use crate::process;

/// #2765 — UUIDv7 trace ID for nudge correlation. Sortable + time-ordered
/// (millisecond-precision timestamp prefix), 36-char canonical UUID format.
/// Replaces the old `ntr-<ms>-<4hex>` format which was 24 chars and
/// non-standard. Propagates via `X-Chorus-Trace-Id` HTTP header on the
/// pulse POST and on every spine event in the nudge lifecycle.
fn trace_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

const INBOX_DIR: &str = "/tmp/voice-inbox";
const EXCHANGE_DIR: &str = "/tmp/nudge-exchanges";
fn chorus_log_path() -> String { format!("{}/platform/scripts/chorus-log", crate::shared::state_paths::chorus_root()) }

/// Role directory names for Terminal tab matching
fn role_dir(role: &str) -> Option<&'static str> {
    match role {
        "wren" => Some("wren"),
        "silas" => Some("silas"),
        "kade" => Some("kade"),
        _ => None,
    }
}

/// Detect sender from DEPLOY_ROLE env var.
/// #2287: Contract C1 — DEPLOY_ROLE must be set to wren, silas, or kade.
/// No silent "jeff" fallback. If the caller needs a non-role sender, use --from.
fn detect_sender() -> Result<String, String> {
    match std::env::var("DEPLOY_ROLE") {
        Ok(role) if matches!(role.as_str(), "silas" | "wren" | "kade") => Ok(role),
        Ok(role) => Err(format!(
            "DEPLOY_ROLE={} is not a valid role (expected wren/silas/kade). \
             Use --from <role> if calling from a non-role context.",
            role
        )),
        Err(_) => Err(
            "DEPLOY_ROLE unset — contract violation. \
             Check session-start for the caller. \
             Use --from <role> if calling from a non-role context.".to_string()
        ),
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

/// #2765 AC4: required-field schema for nudge.* spine events. Returns the
/// list of substring tokens (each `<field>=`) that MUST appear in the
/// `extra` payload. Empty list = no enforcement (event isn't in the nudge
/// schema). Validation runs before chorus-log invocation; missing field is
/// logged to stderr and the event is dropped (non-zero exit semantics).
fn required_fields_for(event: &str) -> &'static [&'static str] {
    match event {
        "nudge.requested" => &["from=", "to=", "trace=", "origin="],
        "nudge.surfaced" => &["from=", "to=", "trace=", "attempt="],
        "nudge.surface.failed" => &["from=", "to=", "trace=", "attempt=", "reason=", "permanent="],
        _ => &[],
    }
}

/// Validate that `extra` contains all required field tokens for the given
/// event. Returns a Vec of missing tokens (empty = pass). Used by chorus_log
/// to refuse-at-call-site for nudge.* events with incomplete payloads.
fn missing_required_fields(event: &str, extra: &str) -> Vec<&'static str> {
    required_fields_for(event)
        .iter()
        .filter(|tok| !extra.contains(*tok))
        .copied()
        .collect()
}

/// Emit spine event via chorus-log. Public within the crate so the nudge
/// read path (hooks::nudge_poll::mark_surfaced) can emit nudge.surfaced events
/// through the same path as nudge.emitted — one canonical emission route per
/// #2435 (no parallel chorus-log helpers across hooks).
///
/// #2765 AC4: nudge.* events validated against required-field schema before
/// emission. Missing field = stderr warning + event dropped (non-zero exit
/// semantics enforced at call site by skipping the chorus-log invocation).
pub(crate) fn chorus_log(event: &str, role: &str, extra: &str) {
    let missing = missing_required_fields(event, extra);
    if !missing.is_empty() {
        eprintln!(
            "chorus_log REFUSED — event {} missing required fields: {:?} (extra={})",
            event, missing, extra
        );
        return;
    }
    let log_script = chorus_log_path();
    if Path::new(&log_script).exists() {
        let _ = Command::new(&log_script)
            .args([event, role, extra])
            .output();
    }
}

#[cfg(test)]
mod chorus_log_validation_tests {
    use super::*;

    #[test]
    fn nudge_requested_with_all_fields_passes() {
        let missing = missing_required_fields(
            "nudge.requested",
            "from=silas,to=wren,chars=10,trace=018f-uuid,origin=cli,content=hi",
        );
        assert!(missing.is_empty(), "expected pass, got missing: {:?}", missing);
    }

    #[test]
    fn nudge_requested_missing_trace_fails() {
        let missing = missing_required_fields(
            "nudge.requested",
            "from=silas,to=wren,origin=cli,content=hi",
        );
        assert!(missing.contains(&"trace="), "expected trace= in missing, got: {:?}", missing);
    }

    #[test]
    fn nudge_surfaced_requires_attempt() {
        let missing = missing_required_fields(
            "nudge.surfaced",
            "from=silas,to=wren,trace=018f",
        );
        assert!(missing.contains(&"attempt="), "expected attempt= in missing, got: {:?}", missing);
    }

    #[test]
    fn nudge_surface_failed_requires_reason_and_permanent() {
        let missing = missing_required_fields(
            "nudge.surface.failed",
            "from=silas,to=wren,trace=018f,attempt=1",
        );
        assert!(missing.contains(&"reason="));
        assert!(missing.contains(&"permanent="));
    }

    #[test]
    fn unknown_event_passes_through() {
        let missing = missing_required_fields("session.bootstrap", "anything goes here");
        assert!(missing.is_empty(), "non-nudge events bypass validation");
    }
}


pub fn run(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("Usage: chorus-hook-shim nudge <role> <message> [--from <sender>] [--force] [--dry-run]");
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
        eprintln!("Usage: chorus-hook-shim nudge <role> <message> [--from <sender>] [--force] [--dry-run]");
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
    // #2283: DEC-107 — all nudges inject. --force flag accepted but ignored (always on).
    let mut dry_run = false;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--from" => {
                i += 1;
                if i < args.len() { explicit_sender = Some(args[i].clone()); }
            }
            "--force" => { /* DEC-107: force is always on, flag accepted but ignored */ }
            "--dry-run" => { dry_run = true; }
            _ => {}
        }
        i += 1;
    }

    let sender = match explicit_sender {
        Some(s) => s,
        None => match detect_sender() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("CONTRACT VIOLATION: {}", e);
                return ExitCode::from(1);
            }
        },
    };
    let tid = trace_id();

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

    // #2727 child A (#2763) — sender-side spine emit refactor.
    //
    // Canonical sender-side event is now `nudge.requested` (was `nudge.emitted`).
    // Fires BEFORE the HTTP POST to pulse — belt-and-suspenders for pulse-down
    // resilience (Silas review AC9): if pulse is down, the spine still has a
    // record of the sender's intent, even though messages.db never receives
    // the row. Pulse worker emits per-attempt `nudge.surfaced` /
    // `nudge.surface.failed` (#2727 AC2); together with `nudge.requested` they
    // form the full lifecycle audit.
    //
    // MIGRATION WINDOW (Silas ops review 2026-05-07): we dual-emit
    // `nudge.requested` AND `nudge.emitted` during the read-side migration.
    // Readers (Clearing tailer.ts, MCP server.ts, pulse service.ts, Loki/
    // Grafana, any operator log greps) still reference the old name and
    // would see ZERO events without the dual-emit — the worst observability
    // mode (looks fine, isn't). After read-side cards land for each
    // consumer, the nudge.emitted line below drops in a final cleanup card.
    // Cost: one extra spine line per nudge during the window.
    //
    // #2475 — origin tag (cli/mcp/http) preserved on both events so audit
    // can still distinguish how the nudge was sent.
    // #2443 — no truncation: full content in payload for downstream readers.
    let origin = std::env::var("CHORUS_NUDGE_ORIGIN").unwrap_or_else(|_| "cli".to_string());
    let payload = format!("from={},to={},chars={},trace={},origin={},content={}", sender, target, message.len(), tid, origin, message);
    chorus_log("nudge.requested", &sender, &payload);
    // Migration-window dual-emit — drop in cleanup card after readers migrate.
    chorus_log("nudge.emitted", &sender, &payload);

    // Persist for history — NOT for delivery. Fire-and-forget, failure doesn't
    // block inject. Per #2727 + #2763: the chorus_log calls above already
    // fired regardless of pulse health — the spine has the audit trail even
    // when pulse is down (Silas review AC9 belt-and-suspenders).
    //
    // CHORUS_PULSE_URL env override added 2026-05-07 for hermetic testing
    // (#2763 AC3 explicit-test gap from Kade gemba). Production default stays
    // localhost:3475.
    let persist_body = serde_json::json!({
        "from": sender,
        "to": target,
        "content": message,
        "traceId": tid,
    });
    let pulse_url = std::env::var("CHORUS_PULSE_URL").unwrap_or_else(|_| "http://localhost:3475/api/nudge".to_string());
    let trace_header = format!("X-Chorus-Trace-Id: {}", tid);
    let _ = Command::new("curl")
        .args([
            "-s", "-X", "POST",
            &pulse_url,
            "-H", "Content-Type: application/json",
            "-H", &trace_header,
            "-d", &persist_body.to_string(),
            "--connect-timeout", "2",
        ])
        .output();

    // Delivery owned by pulse worker (#2727) which emits nudge.surfaced /
    // nudge.surface.failed per attempt, plus the per-role spine-tick-poller
    // LaunchAgent (2s cadence, reads chorus.log for nudge.requested or
    // nudge.emitted targeting the role) during the migration window.
    // No sender-side inject, no queue file, no dedup event here.
    //
    // Retired with this commit: inject_by_tab_name call, queue_message fallback,
    // role.nudge.inject_failed event, role.nudge.delivered event, /tmp/voice-inbox
    // writes, role_state.rs idle/waiting drain, inject-watcher.sh LaunchAgent.
    // First operational demonstration of practice-canonical-surface-designation
    // and practice-atomic-cutover (Wren + Kade's Loom triangle).
    let env_dry_run = std::env::var("CHORUS_INJECT_DRY_RUN").is_ok();
    let dry_run = dry_run || env_dry_run;

    if dry_run {
        println!("DRY-RUN: would emit nudge to {} at {} | text={}",
            target, clock_short, &full_text.chars().take(120).collect::<String>());
    } else {
        // Canonical: emit event, exit. Tick-poller surfaces on the receiver's
        // next 2s tick. Sender sees EMITTED (not DELIVERED — delivery is a
        // receiver-side property we don't know yet).
        println!("EMITTED to {} at {}", target, clock_short);
    }

    ExitCode::SUCCESS
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
    let roles = [("wren", "wren"), ("silas", "silas"), ("kade", "kade")];
    let mut failures = 0;

    // #2077 collapse: all osascript routes through chorus-inject (one TCC grant).
    let inject_bin = format!(
        "{}/platform/services/chorus-inject/target/release/chorus-inject",
        crate::shared::state_paths::chorus_root()
    );

    for (role, pattern) in &roles {
        let output = Command::new(&inject_bin)
            .args(["--count-windows", pattern])
            .output();

        let result = match output {
            Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            Err(e) => {
                println!("ALERT: {} — chorus-inject failed: {}", role, e);
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
        assert_eq!(role_dir("wren"), Some("wren"));
        assert_eq!(role_dir("silas"), Some("silas"));
        assert_eq!(role_dir("kade"), Some("kade"));
    }

    #[test]
    fn role_dir_rejects_unknown() {
        assert_eq!(role_dir("jeff"), None);
        assert_eq!(role_dir("unknown"), None);
    }

    // #2435 — queue_message + drain tests retired alongside their functions.
    // Nudge delivery is the spine-tick-poller (platform/scripts/spine-tick-poller);
    // the queue file and drain subcommand no longer exist. Tick-poller coverage
    // is end-to-end: fire dry-run nudge, --once pass, verify nudge.surfaced.

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

    // #2435 — level-based delivery (#1898) retired. V2 has no levels; all nudges
    // emit the same nudge.emitted event and the tick-poller surfaces. Any
    // 'info vs critical' distinction becomes a receiver-side display choice
    // from the canonical record, not a sender-side branch.

    #[test]
    fn needs_reply_with_reply_expected() {
        assert!(needs_reply("[REPLY EXPECTED — nudge kade back]"));
    }

    // #2435 — dry_run_queues_without_inject retired. V2 dry-run prints
    // "DRY-RUN: would emit ..." and exits. End-to-end coverage lives in
    // nudge_suite::nudge_cli_emits_canonical_emitted_event and the tick-poller
    // integration path.

    // ── #2287: DEPLOY_ROLE contract enforcement ───────────────────────────
    // Per chorus-contracts.md C1: DEPLOY_ROLE must be set. The nudge binary
    // fails loud instead of defaulting to "jeff".
    // These tests use serial env manipulation — run one at a time per thread.

    #[test]
    fn detect_sender_returns_role_when_deploy_role_is_valid() {
        std::env::set_var("DEPLOY_ROLE", "silas");
        assert_eq!(detect_sender().ok(), Some("silas".to_string()));
        std::env::set_var("DEPLOY_ROLE", "wren");
        assert_eq!(detect_sender().ok(), Some("wren".to_string()));
        std::env::set_var("DEPLOY_ROLE", "kade");
        assert_eq!(detect_sender().ok(), Some("kade".to_string()));
    }

    #[test]
    fn detect_sender_errors_when_deploy_role_unset() {
        std::env::remove_var("DEPLOY_ROLE");
        let result = detect_sender();
        assert!(result.is_err(), "DEPLOY_ROLE unset must be a contract violation");
        let err = result.unwrap_err();
        assert!(err.contains("DEPLOY_ROLE"), "error must name the contract: {}", err);
        // Restore for other tests
        std::env::set_var("DEPLOY_ROLE", "silas");
    }

    #[test]
    fn detect_sender_errors_when_deploy_role_invalid() {
        std::env::set_var("DEPLOY_ROLE", "not-a-role");
        let result = detect_sender();
        assert!(result.is_err(), "DEPLOY_ROLE=not-a-role must be a contract violation");
        let err = result.unwrap_err();
        assert!(err.contains("not-a-role") || err.contains("valid role"), "error must explain: {}", err);
        // Restore
        std::env::set_var("DEPLOY_ROLE", "silas");
    }
}
