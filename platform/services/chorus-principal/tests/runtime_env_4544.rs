// @test-type: unit
// @domain: identity
//! #4544 — Abby's agy skips the keychain. agy saves its refreshed Google token
//! to the login keychain every hour; her account never logs in at the console,
//! so the save times out and macOS asks Jeff on his screen. agy keeps the token
//! in a file instead when SSH_CONNECTION is set (measured 10-15 from its
//! binary). The variable comes from the runtime's own row (runtimeEnv), so no
//! other runtime gets it.
use chorus_principal::rows::{runtime_for, runtime_hooks_file, runtime_mcp_file, runtime_pane_cmd, runtime_table};
use serde_json::{json, Value};
use std::process::Command;

/// Run the rendered pane command in a throwaway home with the binary replaced
/// by a stub that records its environment; return that environment.
fn pane_env(rt: &str, row: &Value, binary: &str, tag: &str) -> String {
    let home = std::env::temp_dir().join(format!("runtime-env-4544-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join(binary), format!("#!/bin/sh\nenv > '{}/env'\n", home.display())).unwrap();
    Command::new("chmod").arg("+x").arg(bin.join(binary)).status().unwrap();
    let hooks = runtime_hooks_file(row, rt, "abby-normal", "/b/shim", "/b/principal").unwrap_or(json!({}));
    let mcp = runtime_mcp_file("/b/chorus-runner", "abby-normal", "/repo");
    let cmd = runtime_pane_cmd(row, rt, bin.to_str().unwrap(), "/r", "run-4544", &hooks, &mcp).unwrap();
    let ok = Command::new("bash").arg("-c").arg(&cmd).env("HOME", &home).env_remove("SSH_CONNECTION").status().unwrap();
    assert!(ok.success(), "{cmd}");
    std::fs::read_to_string(home.join("env")).unwrap()
}

#[test]
fn abbys_agy_starts_with_ssh_connection_from_its_row() {
    let (rt, row) = runtime_for(&runtime_table(), "abby-normal").expect("the model assigns Abby a runtime");
    assert_eq!(rt, "agy");
    let env = pane_env(&rt, &row, "agy", "agy");
    assert!(env.lines().any(|l| l.starts_with("SSH_CONNECTION=")), "agy keeps its token in a file only when SSH_CONNECTION is set");
}

/// NEGATIVE PROOF: a runtime whose row carries no runtimeEnv starts without it,
/// so the keychain bypass is agy's alone.
#[test]
fn a_runtime_without_runtime_env_gets_no_ssh_connection() {
    let table = runtime_table();
    let row = table["runtimes"]["claude-code"].clone();
    assert!(row.is_object(), "the claude-code row is in the table");
    assert!(row.get("runtimeEnv").is_none(), "claude-code carries no runtimeEnv");
    let binary = row["runtimeBinary"].as_str().unwrap().to_string();
    let env = pane_env("claude-code", &row, &binary, "claude");
    assert!(!env.lines().any(|l| l.starts_with("SSH_CONNECTION=")));
}

/// NEGATIVE PROOF: a runtimeEnv entry that is not NAME=value with a safe name
/// and value is refused, never pasted into the pane line.
#[test]
fn an_unsafe_runtime_env_entry_is_refused() {
    let (rt, mut row) = runtime_for(&runtime_table(), "abby-normal").unwrap();
    for bad in ["lower=x", "SSH_CONNECTION=it's", "NOEQUALS", "PATH=$(rm -rf ~)"] {
        row["runtimeEnv"] = json!([bad]);
        let err = runtime_pane_cmd(&row, &rt, "/b", "/r", "run-1", &json!({}), &json!({})).unwrap_err();
        assert!(err.contains("runtimeEnv"), "{bad}: {err}");
    }
}
