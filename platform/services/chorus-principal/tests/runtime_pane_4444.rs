// @test-type: unit
// @domain: identity
//! #4444 reopen 2 — Abby's pane starts the runtime the model assigns her (agy),
//! with her hooks and tools files written from its row. Jeff, 2026-10-14 08:15:
//! "generating properties on agent config rather than hard coding".
use chorus_principal::rows::{runtime_for, runtime_hooks_file, runtime_mcp_file, runtime_pane_cmd, runtime_table};
use serde_json::{json, Value};
use std::process::Command;

fn abby() -> (String, Value) { runtime_for(&runtime_table(), "abby-normal").expect("the model assigns Abby a runtime") }

#[test]
fn abby_runs_on_agy_with_hooks_under_agys_own_event_names() {
    let (rt, row) = abby();
    assert_eq!(rt, "agy");
    let h = runtime_hooks_file(&row, &rt, "abby-normal", "/b/chorus-hook-shim", "/b/chorus-principal").unwrap();
    let c = &h["chorus"];
    assert_eq!(c["PreInvocation"][0]["command"], "/b/chorus-principal seen abby-normal", "the prompt stamps her seen first");
    assert_eq!(c["PreInvocation"][1]["command"], "/b/chorus-hook-shim runtime-hook agy PreInvocation");
    assert_eq!(c["PreToolUse"][0]["matcher"], "*");
    assert_eq!(c["PreToolUse"][0]["hooks"][0]["command"], "/b/chorus-hook-shim runtime-hook agy PreToolUse");
    assert!(c.get("Stop").is_some() && c.get("SessionStart").is_some() && c.get("PostToolUse").is_some());
}

/// The rendered command, run for real in a throwaway home with `agy` replaced
/// by a stub: it writes her hooks and tools files, exports no key, and starts
/// the row's binary.
#[test]
fn the_pane_command_writes_her_files_and_starts_the_rows_binary() {
    let (rt, row) = abby();
    let home = std::env::temp_dir().join(format!("runtime-pane-4444-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("agy"), format!("#!/bin/sh\necho started > '{}/started'\nenv > '{}/env'\n", home.display(), home.display())).unwrap();
    Command::new("chmod").arg("+x").arg(bin.join("agy")).status().unwrap();
    let hooks = runtime_hooks_file(&row, &rt, "abby-normal", "/b/shim", "/b/principal").unwrap();
    let mcp = runtime_mcp_file("/b/chorus-runner", "abby-normal", "/repo");
    let cmd = runtime_pane_cmd(&row, &rt, bin.to_str().unwrap(), "/j/.chorus/run-agents", "abby-normal-run-1", &hooks, &mcp).unwrap();
    let ok = Command::new("bash").arg("-c").arg(&cmd).env("HOME", &home).env_remove("GEMINI_API_KEY").status().unwrap();
    assert!(ok.success(), "{cmd}");
    let read = |p: &str| -> Value { serde_json::from_str(&std::fs::read_to_string(home.join(p)).unwrap()).unwrap() };
    assert_eq!(read(".gemini/config/hooks.json"), hooks);
    assert_eq!(read(".gemini/config/mcp_config.json")["mcpServers"]["chorus-api"]["args"], json!(["stdio"]));
    assert!(home.join("started").exists(), "the row's binary ran");
    let env = std::fs::read_to_string(home.join("env")).unwrap();
    assert!(env.contains("CHORUS_SESSION_ID=abby-normal-run-1"));
    // her hooks run as her account: they must find the guard's agent socket
    assert!(env.contains("CHORUS_HOOKS_RUN_DIR=/j/.chorus/run-agents"), "without it every tool is refused as daemon-down");
    assert!(!env.contains("GEMINI_API_KEY="), "a Google sign-in exports no key");
}

/// NEGATIVE PROOF: a principal on the retired Gemini CLI is refused with the
/// row's own reason, not a 402 or a sign-in screen.
#[test]
fn a_retired_runtime_is_refused_by_its_reason() {
    let t = runtime_table();
    let row = t["runtimes"]["gemini-cli"].clone();
    let why = runtime_pane_cmd(&row, "gemini-cli", "/b", "/r", "run-1", &json!({}), &json!({})).unwrap_err();
    assert!(why.contains("gemini-cli is retired: Google stopped serving Gemini CLI"), "{why}");
}

/// NEGATIVE PROOF: a hooks layout nothing renders is refused by name, never
/// written as an empty file.
#[test]
fn an_unrendered_hooks_layout_is_refused_by_name() {
    let (rt, mut row) = abby();
    row["runtimeHooksLayout"] = json!("claude-settings");
    let why = runtime_hooks_file(&row, &rt, "abby-normal", "/b/s", "/b/p").unwrap_err();
    assert!(why.contains("hooks layout \"claude-settings\" is not rendered yet"), "{why}");
}
