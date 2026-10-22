// @test-type: unit
// @domain: identity
//! #4691 — Borg Queen's login opens agy, like Abby's (Jeff 2026-10-22 07:31:
//! she runs on Gemini). Login never reads the door: it reads runsOn from
//! shared/agent_runtimes.json, compiled in, which athena-gen writes from the
//! model's agentRuntime (Silas 08:05). Without her line there, her pane
//! would open Claude.
use chorus_principal::rows::{runtime_for, runtime_hooks_file, runtime_table};

#[test]
fn borg_queen_runs_on_agy_with_her_own_name_in_the_seen_hook() {
    let (rt, row) = runtime_for(&runtime_table(), "borg-queen").expect("the model assigns Borg Queen a runtime");
    assert_eq!(rt, "agy");
    let h = runtime_hooks_file(&row, &rt, "borg-queen", "/b/chorus-hook-shim", "/b/chorus-principal").unwrap();
    assert_eq!(h["chorus"]["PreInvocation"][0]["command"], "/b/chorus-principal seen borg-queen");
}

/// NEGATIVE: a role the model gives no runtime gets none, so the table is
/// not answering agy for every name.
#[test]
fn a_role_with_no_agent_runtime_gets_none() {
    assert!(runtime_for(&runtime_table(), "kade").is_none());
    assert!(runtime_for(&runtime_table(), "no-such-role").is_none());
}
