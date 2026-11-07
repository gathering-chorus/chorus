// @test-type: unit
// @domain: identity
//! #4777 — Dedicated Mac role accounts (`chorus-silas`, `chorus-wren`, `chorus-kade`)
//! running Claude Code:
//! 1. Export `CLAUDE_CODE_OAUTH_TOKEN` from `~/.chorus/secrets/claude.key` when present
//!    and non-empty so `claude` never touches macOS Keychain (`#4202` / `#4544`).
//! 2. Render `claude-settings` hooks layout (`{"hooks": ...}`) in `runtime_hooks_file`.
//! 3. Export `CHORUS_SESSIONS_DIR` in `own_account_inner_cmd` (`launch_line` under `Some(acct)`)
//!    so `SessionStart` registers `<role>-<pid>.json` in the shared sessions directory.
use chorus_principal::own_account_inner_cmd;
use chorus_principal::rows::{runtime_hooks_file, runtime_mcp_file, runtime_pane_cmd, runtime_table};
use serde_json::{json, Value};
use std::process::Command;

fn claude_row() -> Value {
    let table = runtime_table();
    table["runtimes"]["claude-code"].clone()
}

fn run_claude_pane(secret: Option<&str>, tag: &str) -> String {
    let row = claude_row();
    let home = std::env::temp_dir().join(format!("claude-acct-4777-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    if let Some(contents) = secret {
        let sec_dir = home.join(".chorus/secrets");
        std::fs::create_dir_all(&sec_dir).unwrap();
        std::fs::write(sec_dir.join("claude.key"), contents).unwrap();
    }
    std::fs::write(bin.join("claude"), format!("#!/bin/sh\nenv > '{}/env'\n", home.display())).unwrap();
    Command::new("chmod").arg("+x").arg(bin.join("claude")).status().unwrap();
    let hooks = runtime_hooks_file(&row, "claude-code", "silas", "/b/shim", "/b/principal").unwrap();
    let mcp = runtime_mcp_file("/b/chorus-runner", "silas", "/repo");
    let cmd = runtime_pane_cmd(&row, "claude-code", bin.to_str().unwrap(), "/j/.chorus/run-agents", "silas-run-4777", &hooks, &mcp).unwrap();
    let ok = Command::new("bash")
        .arg("-c")
        .arg(&cmd)
        .env("HOME", &home)
        .env_remove("CLAUDE_CODE_OAUTH_TOKEN")
        .status()
        .unwrap();
    assert!(ok.success(), "{cmd}");
    std::fs::read_to_string(home.join("env")).unwrap()
}

#[test]
fn claude_pane_exports_oauth_token_when_claude_key_exists() {
    let env = run_claude_pane(Some("sk-ant-oat01-fixture-4777\n"), "with-key");
    assert!(
        env.lines().any(|l| l == "CLAUDE_CODE_OAUTH_TOKEN=sk-ant-oat01-fixture-4777"),
        "expected CLAUDE_CODE_OAUTH_TOKEN from ~/.chorus/secrets/claude.key, got:\n{env}"
    );
}

#[test]
fn negative_missing_or_empty_claude_key_exports_no_oauth_token() {
    let env_missing = run_claude_pane(None, "no-key");
    assert!(
        !env_missing.lines().any(|l| l.starts_with("CLAUDE_CODE_OAUTH_TOKEN=")),
        "missing claude.key must not export CLAUDE_CODE_OAUTH_TOKEN"
    );
    let env_empty = run_claude_pane(Some(""), "empty-key");
    assert!(
        !env_empty.lines().any(|l| l.starts_with("CLAUDE_CODE_OAUTH_TOKEN=")),
        "empty claude.key must not export empty CLAUDE_CODE_OAUTH_TOKEN"
    );
}

#[test]
fn claude_settings_hooks_layout_renders_hooks_object_and_refuses_unknown() {
    let row = claude_row();
    let h = runtime_hooks_file(&row, "claude-code", "silas", "/b/shim", "/b/principal").unwrap();
    let hooks = &h["hooks"];
    assert!(hooks.is_object(), "claude-settings layout writes top-level 'hooks' object: {h}");
    assert_eq!(hooks["UserPromptSubmit"][0]["matcher"], "*");
    assert_eq!(hooks["UserPromptSubmit"][0]["hooks"][0]["command"], "/b/principal seen silas");
    assert_eq!(hooks["UserPromptSubmit"][0]["hooks"][1]["command"], "/b/shim runtime-hook claude-code UserPromptSubmit");
    assert_eq!(hooks["PreToolUse"][0]["matcher"], "*");

    let mut bad = row.clone();
    bad["runtimeHooksLayout"] = json!("bogus-layout");
    let err = runtime_hooks_file(&bad, "claude-code", "silas", "/b/shim", "/b/principal").unwrap_err();
    assert!(err.contains("hooks layout \"bogus-layout\" is not rendered yet"), "{err}");
}

#[test]
fn own_account_launch_line_exports_shared_sessions_dir_and_claude_key() {
    let inner = own_account_inner_cmd(
        "/repo/roles/silas",
        "/repo",
        "/Users/chorus-silas/.chorus/identity",
        "/Users/chorus-silas/.chorus/identity/silas/token.cache",
        "/Users/jeffbridwell/.chorus/sessions",
        "silas",
        "claude -c",
    );
    assert!(inner.contains("CHORUS_SESSIONS_DIR='/Users/jeffbridwell/.chorus/sessions'"), "{inner}");
    assert!(inner.contains(".chorus/secrets/claude.key"), "{inner}");
    assert!(inner.contains("CLAUDE_CODE_OAUTH_TOKEN"), "{inner}");
}
