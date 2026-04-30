//! role-state — L2 Team Awareness layer.
//!
//! Subcommands:
//!   chorus-hook-shim role-state <role> <state> [detail="text"] [gemba=<role>]
//!   chorus-hook-shim role-state query <role|all>
//!   chorus-hook-shim role-state cleanup
//!
//! State file: /tmp/claude-team-scan/{role}-declared.json
//! Contains: role, state, ts, detail, gemba, last_emit, session_alive, pid
//!
//! #2467 / #2629: card and card_type are NOT fields of role-state.
//! Card lives on the board; role-state owns session/attention only.
//! Calls passing `card=N` or `type=X` are REFUSED (exit 2).

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
        eprintln!("Usage: chorus-hook-shim role-state <role> <state> | query <role|all> | cleanup");
        return ExitCode::from(1);
    }

    // Query subcommand
    if args[0] == "query" {
        let target = args.get(1).map(|s| s.as_str()).unwrap_or("all");
        return query(target);
    }

    // #2467: cleanup subcommand — sweep all role-state files, demote stale
    // entries (state in {building,blocked,waiting,observing} but pid dead)
    // to `idle` with card cleared. The phantom-state class: writers don't
    // clean up when sessions die, so the Clearing + readers see stale
    // building-cards forever. This makes the substrate self-healing.
    if args[0] == "cleanup" {
        return cleanup_stale();
    }

    // Auto-cleanup before every write — every transition by any role
    // triggers a sweep, so the lie can't compound across writers.
    let _ = cleanup_stale_silent();

    if args.len() < 2 {
        eprintln!("Usage: chorus-hook-shim role-state <role> <state> [detail=\"text\"] [gemba=<role>]");
        return ExitCode::from(1);
    }

    let role = &args[0];
    let state = &args[1];

    if !VALID_STATES.contains(&state.as_str()) {
        eprintln!("Invalid state: {} (must be building|blocked|waiting|observing|idle)", state);
        return ExitCode::from(1);
    }

    // #2467 / #2629: card and card_type fields are NOT accepted. Card belongs
    // to the board; role-state owns session/attention only.
    //   - Wave 1 (#2467, PR #72): JSON output dropped card/card_type fields.
    //   - Wave 2 (#2467, PR #77): instruction text in skills + CLAUDE.md
    //     fragments cleaned. Transition window opened.
    //   - Wave 3 (#2629, this change): transition closed. Args are REFUSED
    //     at the affordance layer — non-zero exit, no state file written.
    //     Silent-drop is the same shape as the bug we're trying to prevent;
    //     a refusal makes the contract honest at every surface.
    let mut detail = String::new();
    let mut gemba = String::new();

    for kv in args.iter().skip(2) {
        if let Some((key, val)) = kv.split_once('=') {
            match key {
                "detail" => detail = val.replace('"', "\\\""),
                "gemba" => gemba = val.to_string(),
                "card" | "type" => {
                    eprintln!(
                        "role-state: REFUSED — `{}=` is no longer accepted (#2467/#2629).\n\
                         Card lives on the board, not in role-state. Drop the `{}=` arg.\n\
                         Caller (skill / script / fixture) needs updating to pass only:\n\
                           role-state <role> <state> [detail=\"text\"] [gemba=<role>]",
                        key, key
                    );
                    return ExitCode::from(2);
                }
                _ => {}
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

    // #2168 AC-7: unconditional source="declared" stamp.
    // #2467: no card/card_type fields — board is authoritative for cards.
    let mut json = format!(
        r#"{{"role":"{}","state":"{}","ts":{},"last_emit":"{}","session_alive":{},"wall_clock":"{}","source":"declared""#,
        role, state, ts, last_emit, session_alive, wall
    );
    if let Some(p) = pid {
        json.push_str(&format!(r#","pid":{}"#, p));
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
                // #2629: legacy state files written before wave 1 may carry
                // card / card_type fields. Strip them on read — refusal at
                // every surface includes the read path, not just the write
                // path. Found by wren in #2629 gate:product probe.
                if let Some(obj) = parsed.as_object_mut() {
                    obj.remove("card");
                    obj.remove("card_type");
                }

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

/// #2467: stale-state cleanup. Sweeps all 3 role-state files and demotes any
/// in active states (building/blocked/waiting/observing) to idle when the
/// role's session is dead (no claude process matching its cwd). Card field
/// cleared on demotion. The phantom-card class that today's worktree-hook
/// (#2625) tripped on — writers persisted state from sessions that ended.
///
/// Public via `chorus-hook-shim role-state cleanup` and called silently
/// from every write so the lie can't compound across writers.
fn cleanup_stale() -> ExitCode {
    let demoted = sweep_and_demote(true);
    println!("role-state cleanup: demoted {} stale entries", demoted);
    ExitCode::SUCCESS
}

fn cleanup_stale_silent() -> usize {
    sweep_and_demote(false)
}

fn sweep_and_demote(verbose: bool) -> usize {
    let active_states = ["building", "blocked", "waiting", "observing"];
    let mut demoted = 0;

    for role in ROLES {
        let state_file = PathBuf::from(format!("{}/{}-declared.json", SCAN_DIR, role));
        let Ok(content) = fs::read_to_string(&state_file) else { continue; };
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&content) else { continue; };
        let Some(state) = parsed["state"].as_str() else { continue; };
        if !active_states.contains(&state) {
            continue;
        }
        // Only demote if session is genuinely dead. Don't touch state on slow
        // tick — pid-alive is the only unambiguous "session is over" signal.
        if process::find_role_pid(role).is_some() {
            continue;
        }

        // Demote to idle. Stamp source="cleanup" so debugging is possible —
        // the next person can see this entry was auto-fixed by sweep, not
        // declared by a writer. No card field — cards belong to the board,
        // not role-state (Jeff's directive 2026-04-30).
        let prev_state = state.to_string();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let wall = process::wall_clock();
        let json = format!(
            r#"{{"role":"{}","state":"idle","ts":{},"last_emit":"{}","session_alive":false,"wall_clock":"{}","source":"cleanup","prev_state":"{}"}}"#,
            role, ts, wall, wall, prev_state
        );
        let tmp = PathBuf::from(format!("{}/{}-declared.json.tmp", SCAN_DIR, role));
        if fs::File::create(&tmp).and_then(|mut f| writeln!(f, "{}", json)).is_ok()
            && fs::rename(&tmp, &state_file).is_ok()
        {
            demoted += 1;
            if let Ok(mut log_file) = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(chorus_log_path())
            {
                let _ = writeln!(
                    log_file,
                    "role.state.cleanup | role={} prev_state={} reason=session-dead",
                    role, prev_state
                );
            }
            if verbose {
                eprintln!("  {} demoted: {} -> idle (session dead)", role, prev_state);
            }
        }
    }
    demoted
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
        // #2629: card= no longer accepted; pass only state (+ optional
        // detail/gemba). State file MUST NOT contain card field.
        let result = run(&[
            "kade".into(),
            "building".into(),
        ]);
        assert_eq!(result, ExitCode::SUCCESS);

        let state_file = format!("{}/kade-declared.json", SCAN_DIR);
        let content = fs::read_to_string(&state_file).expect("state file should exist");
        assert!(content.contains("\"state\":\"building\""));
        assert!(content.contains("\"role\":\"kade\""));
        // #2467/#2629: card field never appears
        assert!(!content.contains("\"card\":"), "card field must NOT appear: {}", content);
        assert!(!content.contains("\"card_type\":"), "card_type field must NOT appear: {}", content);
    }

    #[test]
    fn query_returns_declared_state() {
        // First declare (no card= per #2629 affordance)
        run(&["kade".into(), "building".into()]);

        // Then query — should succeed
        let result = query("kade");
        assert_eq!(result, ExitCode::SUCCESS);
    }

    #[test]
    fn query_all_returns_all_roles() {
        let result = query("all");
        assert_eq!(result, ExitCode::SUCCESS);
    }

    // --- #2629 wave 3: affordance-layer refusal of card= / type= ---
    //
    // History (git log): wave 1 (ce22b9c7, PR #72) made the JSON writer
    // drop card/card_type fields silently. Wave 2 (75f108b1, PR #77)
    // cleaned the instruction text in skills + CLAUDE.md fragments.
    // Prior to #2467, the writer ACCEPTED and PERSISTED card= via the
    // carry-forward block (#2058 → 6146baf7). Wave 3 closes the
    // remaining affordance gap: parser must REFUSE card=/type= args, not
    // silently drop them. Otherwise a script reaching for the old
    // syntax still works (no error) and the lie can return.

    #[test]
    fn rejects_card_arg() {
        // RED against current code (parser arm `"card" | "type" => {}`
        // silently drops). After wave 3: exit non-zero with a clear
        // error pointing to #2467.
        let result = run(&[
            "kade".into(),
            "building".into(),
            "card=1718".into(),
        ]);
        assert_eq!(result, ExitCode::from(2),
            "card= must be REFUSED at the affordance layer (#2629), not silently dropped");
    }

    #[test]
    fn rejects_type_arg() {
        let result = run(&[
            "kade".into(),
            "building".into(),
            "type=fix".into(),
        ]);
        assert_eq!(result, ExitCode::from(2),
            "type= must be REFUSED at the affordance layer (#2629), not silently dropped");
    }

    #[test]
    fn rejects_both_card_and_type() {
        let result = run(&[
            "kade".into(),
            "building".into(),
            "card=1718".into(),
            "type=fix".into(),
        ]);
        assert_eq!(result, ExitCode::from(2),
            "card=+type= must be REFUSED (#2629)");
    }

    // Note: query() writes to stdout — full end-to-end strip verification
    // lives in tests/role_state_legacy_strip.rs (integration test) where
    // CARGO_BIN_EXE_chorus-hook-shim is in scope. Unit tests here cover
    // refusal at the write path; the integration test covers the read path.

    #[test]
    fn refusal_does_not_write_state_file() {
        // When args are refused, no state file should be created/updated —
        // the call is rejected as a whole, not partially honored.
        let state_file = format!("{}/wren-declared.json", SCAN_DIR);
        let _ = fs::remove_file(&state_file);
        let result = run(&[
            "wren".into(),
            "building".into(),
            "card=99".into(),
        ]);
        assert_eq!(result, ExitCode::from(2));
        assert!(!std::path::Path::new(&state_file).exists() ||
                !fs::read_to_string(&state_file).unwrap_or_default().contains("\"state\":\"building\""),
                "refused call must not have updated wren state to building (#2629)");
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
