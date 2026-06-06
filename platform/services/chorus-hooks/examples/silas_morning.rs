//! #3003 demo: run the guard against the exact scenario silas hit 2026-05-19.
//! Shows allow/deny for five concrete paths a no-WIP /reboot might try.

use chorus_hooks::canonical_write_guard;
use chorus_hooks::HookInput;
use serde_json::json;

fn run_scenario(label: &str, role: &str, file_path: &str) {
    // Mimic chorus-env-setup.sh: no WIP card → <ROLE>_WERK unset.
    let canonical = "/Users/jeffbridwell/CascadeProjects/chorus";
    std::env::set_var("CHORUS_HOME", canonical);
    std::env::set_var("CHORUS_ROLE", role);
    std::env::set_var("CHORUS_WERK_BASE", "/Users/jeffbridwell/CascadeProjects/chorus-werk");
    std::env::remove_var(format!("{}_WERK", role.to_uppercase()));

    let input = HookInput {
        tool_name: Some("Write".to_string()),
        tool_input: Some(json!({"file_path": file_path, "content": "x"})),
        tool_response: None,
        session_id: None,
        cwd: None,
        prompt: None,
        stop_hook_active: None,
        hook_type: None,
        deploy_role: None,
        chorus_worktree_override: None,
        trace_id: None,
    };

    let response = canonical_write_guard::check(&input);
    let verdict = if response.stdout.is_none() { "ALLOW" } else { "DENY" };

    println!("---");
    println!("Scenario: {}", label);
    println!("Role:     {}", role);
    println!("Path:     {}", file_path);
    println!("Verdict:  {}", verdict);
    if let Some(msg) = response.stdout {
        if let Some(start) = msg.find("\"permissionDecisionReason\":\"") {
            let s = &msg[start + 28..];
            if let Some(end) = s.find("\"}") {
                println!("Reason:   {}", &s[..end]);
            }
        }
    }
}

fn main() {
    println!("\n=== #3003 canonical-write-guard demo ===");
    println!("All scenarios: role=silas, no WIP card (SILAS_WERK unset)");

    run_scenario(
        "silas /reboot at 10am — what was stranded as 8eb35bb0",
        "silas",
        "/Users/jeffbridwell/CascadeProjects/chorus/roles/silas/next-session.md",
    );

    run_scenario(
        "wren's allowlist extension — stories.md",
        "silas",
        "/Users/jeffbridwell/CascadeProjects/chorus/roles/silas/stories.md",
    );

    run_scenario(
        "narrow scope — non-allowlist file under role dir still refused",
        "silas",
        "/Users/jeffbridwell/CascadeProjects/chorus/roles/silas/scratch.md",
    );

    run_scenario(
        "cross-role attempt — silas trying to write wren's state, still refused",
        "silas",
        "/Users/jeffbridwell/CascadeProjects/chorus/roles/wren/next-session.md",
    );

    run_scenario(
        "arbitrary canonical path — still refused (allowlist is narrow)",
        "silas",
        "/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/foo.sh",
    );

    println!();
}
