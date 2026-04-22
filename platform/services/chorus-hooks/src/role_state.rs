//! role-state — L2 Team Awareness layer.
//!
//! Subcommands:
//!   chorus-hook-shim role-state <role> <state> [card=N] [detail="text"] [gemba=<role>]
//!   chorus-hook-shim role-state query <role|all>
//!
//! State file: /tmp/claude-team-scan/{role}-declared.json
//! Contains: role, state, ts, card, detail, gemba, last_emit, session_alive, pid

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use crate::process;
use crate::shared::state_paths::chorus_root;

const SCAN_DIR: &str = "/tmp/claude-team-scan";
const VALID_STATES: &[&str] = &["building", "blocked", "waiting", "observing", "idle"];
const ROLES: &[&str] = &["wren", "silas", "kade"];

fn chorus_log_path() -> String {
    format!("{}/platform/logs/chorus.log", chorus_root())
}

pub fn run(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("Usage: chorus-hook-shim role-state <role> <state> | query <role|all>");
        return ExitCode::from(1);
    }

    // Query subcommand
    if args[0] == "query" {
        let target = args.get(1).map(|s| s.as_str()).unwrap_or("all");
        return query(target);
    }

    if args.len() < 2 {
        eprintln!("Usage: chorus-hook-shim role-state <role> <state> [card=N] [detail=\"text\"] [gemba=<role>]");
        return ExitCode::from(1);
    }

    let role = &args[0];
    let state = &args[1];

    if !VALID_STATES.contains(&state.as_str()) {
        eprintln!("Invalid state: {} (must be building|blocked|waiting|observing|idle)", state);
        return ExitCode::from(1);
    }

    // Parse key=value extras
    let mut card = String::new();
    let mut card_type = String::new();
    let mut detail = String::new();
    let mut gemba = String::new();

    for kv in args.iter().skip(2) {
        if let Some((key, val)) = kv.split_once('=') {
            match key {
                "card" => card = val.to_string(),
                "type" => card_type = val.to_string(),
                "detail" => detail = val.replace('"', "\\\""),
                "gemba" => gemba = val.to_string(),
                _ => {}
            }
        }
    }

    // Auto-resolve card_type from board when card is provided but type isn't.
    // The hook service can't run `cards` (no node in LaunchAgent PATH),
    // so we resolve it here in the role's terminal where node is available.
    if !card.is_empty() && card_type.is_empty() {
        let cards_script = format!("{}/platform/scripts/cards", chorus_root());
        if let Ok(output) = std::process::Command::new("bash")
            .args([cards_script.as_str(), "view", &card])
            .output()
        {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines() {
                    if line.contains("type:") {
                        if let Some(t) = line.split("type:").last() {
                            let resolved = t.split(|c: char| !c.is_alphanumeric()).next().unwrap_or("");
                            if !resolved.is_empty() && resolved != "unknown" {
                                card_type = resolved.to_string();
                            }
                        }
                    }
                }
            }
        }
    }

    // Carry forward card from previous state for building/blocked/waiting
    // — role is still pointing at a card ("waiting on #N for review" is valid).
    // idle/observing = no role-own card — clear it. (#2058, #2168 AC-8)
    if card.is_empty() && (state == "building" || state == "blocked" || state == "waiting") {
        let prev_file = PathBuf::from(format!("{}/{}-declared.json", SCAN_DIR, role));
        if let Ok(content) = fs::read_to_string(&prev_file) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(prev_card) = parsed.get("card") {
                    if let Some(n) = prev_card.as_u64() {
                        card = n.to_string();
                    } else if let Some(s) = prev_card.as_str() {
                        if !s.is_empty() {
                            card = s.to_string();
                        }
                    }
                }
            }
        }
    }

    // Build enriched state — L2 fields
    let _ = fs::create_dir_all(SCAN_DIR);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let wall = process::wall_clock();
    let pid = process::find_role_pid(role);
    let session_alive = pid.is_some();
    let last_emit = last_spine_emit(role).unwrap_or_else(|| wall.clone());

    // #2168 AC-7: unconditional source="declared" stamp. Semantic pair:
    // "declared" = this file reflects a stated intent (human or session-start hook).
    // "inferred" = reconciler-stamped from tool-call observation.
    let mut json = format!(
        r#"{{"role":"{}","state":"{}","ts":{},"last_emit":"{}","session_alive":{},"wall_clock":"{}","source":"declared""#,
        role, state, ts, last_emit, session_alive, wall
    );
    if let Some(p) = pid {
        json.push_str(&format!(r#","pid":{}"#, p));
    }
    if !card.is_empty() {
        json.push_str(&format!(r#","card":{}"#, card));
    }
    if !card_type.is_empty() {
        json.push_str(&format!(r#","card_type":"{}""#, card_type));
    }
    if !detail.is_empty() {
        json.push_str(&format!(r#","detail":"{}""#, detail));
    }
    if !gemba.is_empty() {
        json.push_str(&format!(r#","gemba":"{}""#, gemba));
    }
    json.push('}');

    let out = PathBuf::from(format!("{}/{}-declared.json", SCAN_DIR, role));
    let tmp = PathBuf::from(format!("{}/{}-declared.json.tmp", SCAN_DIR, role));

    if let Ok(mut f) = fs::File::create(&tmp) {
        let _ = writeln!(f, "{}", json);
    }
    let _ = fs::rename(&tmp, &out);

    // Emit spine event to chorus.log (#1945)
    let mut event_kv = format!("role={} state={}", role, state);
    if !card.is_empty() {
        event_kv.push_str(&format!(" card={}", card));
    }
    if !card_type.is_empty() {
        event_kv.push_str(&format!(" type={}", card_type));
    }
    if !gemba.is_empty() {
        event_kv.push_str(&format!(" gemba={}", gemba));
    }
    let log_path = chorus_log_path();
    if let Ok(mut log_file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        let _ = writeln!(log_file, "role.state.changed | {} {}", role, event_kv);
    }

    // #2435 atomic cutover — idle/waiting inbox drain retired. The /tmp/voice-inbox
    // queue was the fallback channel for inject-failed nudges. Under the spine-
    // tick-poller canonical receiver (platform/scripts/spine-tick-poller), there
    // is no inject failure to queue for — the poller retries on its next 2s tick
    // directly from spine. Queue file + drain + count-payload nudge.acknowledged
    // event all retire together.

    ExitCode::SUCCESS
}

/// Query role state — reads declared state, enriches with live PID check
fn query(target: &str) -> ExitCode {
    let roles: Vec<&str> = if target == "all" {
        ROLES.to_vec()
    } else if ROLES.contains(&target) {
        vec![target]
    } else {
        eprintln!("Unknown role: {}. Use wren, silas, kade, or all.", target);
        return ExitCode::from(1);
    };

    for role in &roles {
        let state_file = PathBuf::from(format!("{}/{}-declared.json", SCAN_DIR, role));
        let pid = process::find_role_pid(role);
        let session_alive = pid.is_some();

        if let Ok(content) = fs::read_to_string(&state_file) {
            if let Ok(mut parsed) = serde_json::from_str::<serde_json::Value>(&content) {
                // Enrich with live data
                parsed["session_alive"] = serde_json::Value::Bool(session_alive);
                if let Some(p) = pid {
                    parsed["pid"] = serde_json::json!(p);
                } else {
                    parsed["pid"] = serde_json::Value::Null;
                }

                // Compute staleness
                if let Some(ts) = parsed["ts"].as_u64() {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let age_secs = now.saturating_sub(ts);
                    parsed["age_secs"] = serde_json::json!(age_secs);
                    parsed["age_human"] = serde_json::json!(humanize_duration(age_secs));
                }

                // Refresh last_emit from spine
                if let Some(last) = last_spine_emit(role) {
                    parsed["last_emit"] = serde_json::json!(last);
                }

                println!("{}", serde_json::to_string_pretty(&parsed).unwrap_or_default());
            }
        } else {
            // No state file — report what we can from PID detection
            let status = if session_alive { "unknown (session alive, no state declared)" } else { "offline (no session)" };
            println!(r#"{{"role":"{}","state":"{}","session_alive":{}}}"#, role, status, session_alive);
        }
    }

    ExitCode::SUCCESS
}

/// Get last spine event timestamp for a role from chorus.log (JSON format, tail search)
fn last_spine_emit(role: &str) -> Option<String> {
    let content = fs::read_to_string(chorus_log_path()).ok()?;
    let role_pattern = format!("\"role\":\"{}\"", role);
    for line in content.lines().rev() {
        if line.contains(&role_pattern) {
            // Extract timestamp from JSON: {"timestamp":"2026-03-21T19:11:18.306Z",...}
            if let Some(start) = line.find("\"timestamp\":\"") {
                let after = &line[start + 13..];
                if let Some(end) = after.find('"') {
                    let ts = &after[..end];
                    // Convert ISO to Boston time display: take date+time portion
                    if ts.len() >= 19 {
                        return Some(ts[..19].replace('T', " "));
                    }
                }
            }
        }
    }
    None
}

fn humanize_duration(secs: u64) -> String {
    if secs < 60 { return format!("{}s ago", secs); }
    if secs < 3600 { return format!("{}m ago", secs / 60); }
    if secs < 86400 { return format!("{}h ago", secs / 3600); }
    format!("{}d ago", secs / 86400)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // --- AC2: declare/query round-trip ---

    #[test]
    fn declare_creates_state_file() {
        // Uses the real SCAN_DIR since run() hardcodes it
        let result = run(&[
            "kade".into(),
            "building".into(),
            "card=1718".into(),
        ]);
        assert_eq!(result, ExitCode::SUCCESS);

        let state_file = format!("{}/kade-declared.json", SCAN_DIR);
        let content = fs::read_to_string(&state_file).expect("state file should exist");
        assert!(content.contains("\"state\":\"building\""));
        assert!(content.contains("\"card\":1718"));
        assert!(content.contains("\"role\":\"kade\""));
    }

    #[test]
    fn query_returns_declared_state() {
        // First declare
        run(&["kade".into(), "building".into(), "card=1718".into()]);

        // Then query — should succeed
        let result = query("kade");
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn query_all_returns_all_roles() {
        let result = query("all");
        assert_eq!(result, ExitCode::SUCCESS);
    }

    // --- AC2: state validation ---

    #[test]
    fn rejects_invalid_state() {
        let result = run(&["kade".into(), "dancing".into()]);
        assert_eq!(result, ExitCode::from(1));
    }

    #[test]
    fn accepts_all_valid_states() {
        for state in VALID_STATES {
            let result = run(&["kade".into(), state.to_string()]);
            assert_eq!(result, ExitCode::SUCCESS, "state '{}' should be valid", state);
        }
    }

    // --- AC2: corrupt state file recovery ---

    #[test]
    fn query_handles_corrupt_state_file() {
        let _ = fs::create_dir_all(SCAN_DIR);
        let state_file = format!("{}/kade-declared.json", SCAN_DIR);

        // Write garbage JSON
        fs::write(&state_file, "not json at all {{{").expect("write corrupt file");

        // Query should not crash — returns SUCCESS even if parse fails
        let result = query("kade");
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn query_handles_missing_state_file() {
        let _ = fs::create_dir_all(SCAN_DIR);
        let state_file = format!("{}/wren-declared.json", SCAN_DIR);
        let _ = fs::remove_file(&state_file);

        // Should report "offline" gracefully, not crash
        let result = query("wren");
        assert_eq!(result, ExitCode::SUCCESS);
    }

    // #2435 atomic cutover — inbox drain tests retired alongside the drain
    // code. /tmp/voice-inbox is no longer written (inject retired in nudge.rs)
    // nor read (drain removed from role_state). The spine-tick-poller is the
    // canonical receiver; there's nothing for role_state to drain. New coverage
    // for tick-poller delivery lives in the receiver-side tests for that
    // script and its spine fold.

    /// #2435 atomic cutover — regression guard that no role-state transition
    /// (idle, waiting, building) touches the retired /tmp/voice-inbox queue.
    /// Pre-existing queue files must survive all transitions unchanged.
    #[test]
    fn no_transition_drains_retired_inbox() {
        let test_role = "test-no-drain-post-cutover";
        let inbox_dir = format!("/tmp/voice-inbox/{}", test_role);
        let inbox_file = format!("{}/pending-inject.txt", inbox_dir);

        for state in &["idle", "waiting", "building"] {
            let _ = fs::create_dir_all(&inbox_dir);
            fs::write(&inbox_file, "should stay\n").expect("write test inbox");

            let result = run(&[test_role.into(), (*state).into()]);
            assert_eq!(result, ExitCode::SUCCESS);

            let content = fs::read_to_string(&inbox_file).unwrap_or_default();
            assert!(
                content.contains("should stay"),
                "retired drain must NOT consume inbox on transition={}", state
            );
        }

        // Cleanup
        let _ = fs::remove_file(&inbox_file);
        let _ = fs::remove_dir(&inbox_dir);
    }

    // --- AC2: detail and gemba extras ---

    #[test]
    fn declare_with_detail_persists() {
        let result = run(&[
            "silas".into(),
            "blocked".into(),
            "detail=waiting for review".into(),
        ]);
        assert_eq!(result, ExitCode::SUCCESS);

        let state_file = format!("{}/silas-declared.json", SCAN_DIR);
        let content = fs::read_to_string(&state_file).expect("state file");
        assert!(content.contains("\"detail\":\"waiting for review\""));
    }

    #[test]
    fn declare_with_gemba_persists() {
        let result = run(&[
            "wren".into(),
            "observing".into(),
            "gemba=kade".into(),
        ]);
        assert_eq!(result, ExitCode::SUCCESS);

        let state_file = format!("{}/wren-declared.json", SCAN_DIR);
        let content = fs::read_to_string(&state_file).expect("state file");
        assert!(content.contains("\"gemba\":\"kade\""));
    }

    // --- AC2: humanize_duration ---

    #[test]
    fn humanize_duration_formats_correctly() {
        assert_eq!(humanize_duration(30), "30s ago");
        assert_eq!(humanize_duration(120), "2m ago");
        assert_eq!(humanize_duration(7200), "2h ago");
        assert_eq!(humanize_duration(172800), "2d ago");
    }

    // --- AC2: query rejects unknown role ---

    #[test]
    fn query_rejects_unknown_role() {
        let result = query("jeff");
        assert_eq!(result, ExitCode::from(1));
    }

    // --- AC2: no args returns error ---

    #[test]
    fn run_no_args_returns_error() {
        let result = run(&[]);
        assert_eq!(result, ExitCode::from(1));
    }

    #[test]
    fn run_role_only_returns_error() {
        let result = run(&["kade".into()]);
        assert_eq!(result, ExitCode::from(1));
    }
}
