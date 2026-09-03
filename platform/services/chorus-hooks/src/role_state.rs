//! role-state — #4028: nothing declared, nothing stored.
//!
//! Until #4028 this subcommand wrote /tmp/claude-team-scan/{role}-declared.json
//! and a sweep demoted it when a pid looked dead. The file reverted to
//! "unknown" overnight, said "idle" through six pipeline rounds, and 142 cards
//! of declaring never made it true. Jeff, 2026-08-28: "i dont know why we cant
//! rip out 'declared' role state and have it be derived directly from streams."
//!
//! Now (agreed with Silas 2026-09-02): the state is DERIVED on every read by
//! chorus-api's GET /api/chorus/context/roles from the spine the hooks daemon
//! already writes (hook.decision / context.inject.request per tool call) plus
//! the board's WIP. The only thing a role still says is that it is BLOCKED —
//! as a spine EVENT (`role.blocked`, with a detail) that expires the moment
//! activity resumes. No file. No sweep. No reconciler.
//!
//! Subcommands:
//!   chorus-hook-shim role-state <role> blocked detail="why"   → emits role.blocked
//!   chorus-hook-shim role-state <role> <other-state>          → no-op (derived), exit 0
//!   chorus-hook-shim role-state query <role|all>              → reads the derived rows
//!   chorus-hook-shim role-state cleanup                       → no-op (nothing stored)

use std::fs;
use std::process::ExitCode;

const ROLES: &[&str] = &["wren", "silas", "kade"];
const DERIVED_STATES: &[&str] = &["building", "waiting", "observing", "idle"];

fn roles_endpoint() -> String {
    std::env::var("CHORUS_ROLES_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "http://localhost:3340/api/chorus/context/roles".to_string())
}

/// Parse `[detail="…"] [gemba=…] [card=…]` after `<role> <state>`. Returns
/// Err(exit code) for the refused `card=` / `type=` forms (#2467/#2629 — the
/// board owns the card; that contract did not change).
pub fn parse_detail(args: &[String]) -> Result<String, u8> {
    let mut detail = String::new();
    for kv in args {
        if let Some((key, val)) = kv.split_once('=') {
            match key {
                "detail" => detail = val.trim_matches('"').to_string(),
                "card" | "type" => return Err(2),
                _ => {}
            }
        }
    }
    Ok(detail)
}

pub fn run(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("Usage: chorus-hook-shim role-state <role> blocked detail=\"why\" | <role> <state> | query <role|all> | cleanup");
        return ExitCode::from(1);
    }
    if args[0] == "query" {
        return query(args.get(1).map(|s| s.as_str()).unwrap_or("all"));
    }
    if args[0] == "cleanup" {
        println!("role-state cleanup: nothing to clean — state is derived on read (#4028)");
        return ExitCode::SUCCESS;
    }
    if args.len() < 2 {
        eprintln!("Usage: chorus-hook-shim role-state <role> <state> [detail=\"text\"]");
        return ExitCode::from(1);
    }
    let role = args[0].as_str();
    let state = args[1].as_str();
    if !ROLES.contains(&role) {
        eprintln!("Unknown role: {} (wren|silas|kade)", role);
        return ExitCode::from(1);
    }

    let detail = match parse_detail(&args[2..]) {
        Ok(d) => d,
        Err(code) => {
            eprintln!(
                "role-state: REFUSED — `card=` / `type=` are not accepted (#2467/#2629). \
                 The board owns the card; pass only: role-state <role> blocked detail=\"why\""
            );
            return ExitCode::from(code);
        }
    };

    match state {
        "blocked" => {
            // Through the shim's own spine emitter (#3140 schema enrichment, Eastern
            // timestamp, the same JSON every other event carries). No file.
            let detail_kv = format!("detail={}", detail);
            let code = crate::chorus_log::run_silent(&["role.blocked".to_string(), role.to_string(), detail_kv]);
            if code != ExitCode::SUCCESS {
                eprintln!("role-state: could not emit role.blocked");
                return code;
            }
            println!("role.blocked | {} detail=\"{}\" — expires on your next activity (#4028)", role, detail);
            ExitCode::SUCCESS
        }
        s if DERIVED_STATES.contains(&s) => {
            println!(
                "role-state: '{}' is derived from the streams now (#4028) — nothing to declare. \
                 Read it: role-state query {}",
                s, role
            );
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("Invalid state: {} (blocked is the only declarable state; building|waiting|observing|idle are derived)", other);
            ExitCode::from(1)
        }
    }
}

/// Read the derived rows from chorus-api and print them. No file is consulted.
fn query(target: &str) -> ExitCode {
    if target != "all" && !ROLES.contains(&target) {
        eprintln!("Unknown role: {}. Use wren, silas, kade, or all.", target);
        return ExitCode::from(1);
    }
    let url = roles_endpoint();
    let body: serde_json::Value = match ureq::get(&url).timeout(std::time::Duration::from_secs(5)).call() {
        Ok(resp) => match resp.into_json() {
            Ok(v) => v,
            Err(e) => {
                eprintln!("role-state query: {} answered non-JSON: {}", url, e);
                return ExitCode::from(1);
            }
        },
        Err(e) => {
            eprintln!("role-state query: {} unreachable: {} — the state is derived there; nothing local to read", url, e);
            return ExitCode::from(1);
        }
    };
    let rows = body
        .pointer("/data/roles")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for row in rows {
        let name = row.get("role").and_then(|v| v.as_str()).unwrap_or("");
        if target != "all" && name != target {
            continue;
        }
        println!("{}", serde_json::to_string_pretty(&row).unwrap_or_default());
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn detail_is_parsed_and_quotes_stripped() {
        assert_eq!(parse_detail(&args(&["detail=\"why not\"", "gemba=kade"])).unwrap(), "why not");
        assert_eq!(parse_detail(&args(&[])).unwrap(), "");
    }

    #[test]
    fn card_and_type_are_still_refused_at_the_cli() {
        // #2467/#2629 — the board owns the card. #4028 removed the file, not the refusal.
        assert_eq!(parse_detail(&args(&["card=4028"])), Err(2));
        assert_eq!(parse_detail(&args(&["type=fix"])), Err(2));
    }

    #[test]
    fn declaring_a_derived_state_writes_nothing_anywhere() {
        // Negative proof (#3734): the declared file this module used to write must
        // NOT appear after a "building" call — the state is derived, so a
        // declaration has no place to land.
        let scan = std::env::temp_dir().join(format!("4028-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&scan);
        let code = run(&args(&["wren", "building"]));
        assert_eq!(code, ExitCode::SUCCESS);
        assert!(!scan.join("wren-declared.json").exists(), "no declared file may be written");
        assert!(!std::path::Path::new("/tmp/claude-team-scan/wren-declared.json").exists()
            || fs::metadata("/tmp/claude-team-scan/wren-declared.json")
                .and_then(|m| m.modified())
                .map(|t| t.elapsed().map(|d| d.as_secs() > 5).unwrap_or(true))
                .unwrap_or(true),
            "a pre-existing legacy file may exist, but this call must not have touched it");
    }

    #[test]
    fn blocked_appends_one_json_line_to_the_membrane_log() {
        let dir = std::env::temp_dir().join(format!("4028-log-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let log = dir.join("chorus.log");
        let _ = fs::remove_file(&log);
        std::env::set_var("CHORUS_LOG_FILE", log.to_string_lossy().to_string());
        let code = run(&args(&["silas", "blocked", "detail=\"waiting on the DAL cred\""]));
        std::env::remove_var("CHORUS_LOG_FILE");
        assert_eq!(code, ExitCode::SUCCESS);
        let body = fs::read_to_string(&log).expect("log written");
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 1, "exactly one line: {body}");
        let v: serde_json::Value = serde_json::from_str(lines[0]).expect("json line");
        assert_eq!(v["event"], "role.blocked");
        assert_eq!(v["role"], "silas");
        assert_eq!(v["detail"], "waiting on the DAL cred");
    }

    #[test]
    fn unknown_role_and_unknown_state_are_refused() {
        assert_ne!(run(&args(&["bob", "blocked"])), ExitCode::SUCCESS);
        assert_ne!(run(&args(&["wren", "napping"])), ExitCode::SUCCESS);
    }
}
