// @domain: identity
//! #4444 — Abby logs in to her own Gemini screen in her pane (Jeff 2026-10-06:
//! "thats my user interface — a background job that i cant interact with").
use chorus_principal::rows::{gemini_pane_cmd, pane_process_is, pane_runtime, PaneRuntime};

fn role_dir(name: &str, with: &[&str]) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("gp4444-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    for w in with {
        std::fs::create_dir_all(d.join(w)).unwrap();
    }
    d
}

#[test]
fn a_role_home_with_gemini_settings_and_no_claude_settings_starts_gemini() {
    let d = role_dir("abby", &[".gemini"]);
    assert_eq!(pane_runtime(&d), PaneRuntime::Gemini);
}

#[test]
fn negative_proof_a_claude_role_home_still_starts_claude() {
    assert_eq!(pane_runtime(&role_dir("wren", &[".claude"])), PaneRuntime::Claude);
    assert_eq!(pane_runtime(&role_dir("both", &[".claude", ".gemini"])), PaneRuntime::Claude);
    assert_eq!(pane_runtime(&role_dir("none", &[])), PaneRuntime::Claude);
}

#[test]
fn the_gemini_launch_runs_interactive_gemini_with_node_her_key_and_her_policy() {
    let cmd = gemini_pane_cmd("/b/chorus-agent", "/b/gemini", "/j/.claude/settings.json", "abby-normal-run-1a2b").unwrap();
    // the login's run is the session the guard hook names every tool call by
    assert!(cmd.contains("CHORUS_SESSION_ID='abby-normal-run-1a2b'"), "{cmd}");
    // node is on PATH (launchd and sudo give a PATH without it — measured 10-06 18:16)
    assert!(cmd.contains(":/opt/homebrew/bin:"), "{cmd}");
    // her own key, read as her
    assert!(cmd.contains("GEMINI_API_KEY=\"$(cat ~/.chorus/secrets/gemini.key)\""), "{cmd}");
    // the allowed tools are generated from the Claude roles' rules before she starts
    assert!(cmd.contains("CHORUS_ALLOW_RULES_FILE='/j/.claude/settings.json' '/b/chorus-agent' allowed-tools"), "{cmd}");
    // interactive: no --acp, no -p; the policy she was given
    assert!(cmd.ends_with("'/b/gemini' --policy ~/.chorus/gemini-allowed.toml"), "{cmd}");
    assert!(!cmd.contains("--acp") && !cmd.contains(" -p "), "{cmd}");
}

#[test]
fn the_pane_proof_finds_the_runtime_that_was_started() {
    assert!(pane_process_is("sudo -n -u chorus-abby-normal -H bash -c ... '/b/gemini' --policy x", PaneRuntime::Gemini));
    assert!(pane_process_is("/usr/local/bin/claude -c", PaneRuntime::Claude));
}

#[test]
fn negative_proof_a_bare_shell_in_the_pane_is_not_a_running_role() {
    assert!(!pane_process_is("-zsh", PaneRuntime::Gemini));
    assert!(!pane_process_is("/usr/local/bin/claude -c", PaneRuntime::Gemini));
    assert!(!pane_process_is("node gemini.js", PaneRuntime::Claude));
}

#[test]
fn negative_proof_a_run_name_that_could_break_the_shell_is_refused() {
    assert!(gemini_pane_cmd("/b/a", "/b/g", "/r", "x;rm -rf ~").is_err());
    assert!(gemini_pane_cmd("/b/a", "/b/g", "/r", "").is_err());
}

/// #4444 reopen — Jeff's 10:46 screenshot: her boot ran `chorus-hook-shim boot-read`
/// and got "No such file", because the shim's dir was not on her PATH. Proof at the
/// boundary: run the command's own PATH export in a shell and look the shim up.
fn shim_resolves_under(cmd: &str, bin: &std::path::Path) -> bool {
    let export = cmd.split(" && ").next().unwrap();
    let out = std::process::Command::new("/bin/bash")
        .arg("-c")
        .arg(format!("{export} 2>/dev/null; command -v chorus-hook-shim"))
        .env("PATH", "/usr/bin:/bin")
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim() == bin.join("chorus-hook-shim").to_str().unwrap()
}

fn fake_bin(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gemini-pane-4444-{}-{}", name, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let shim = dir.join("chorus-hook-shim");
    std::fs::write(&shim, "#!/bin/sh\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    dir
}

#[test]
fn her_shell_finds_chorus_hook_shim_by_its_bare_name() {
    let bin = fake_bin("on-path");
    let agent = bin.join("chorus-agent");
    let cmd = gemini_pane_cmd(agent.to_str().unwrap(), "/b/gemini", "/r", "abby-normal-run-1a2b").unwrap();
    assert!(shim_resolves_under(&cmd, &bin), "{cmd}");
}

#[test]
fn negative_proof_a_shim_outside_the_agent_bin_dir_is_not_found() {
    // the check must go red when the shim is not where the command put on PATH
    let bin = fake_bin("off-path");
    let cmd = gemini_pane_cmd("/nowhere/chorus-agent", "/b/gemini", "/r", "abby-normal-run-1a2b").unwrap();
    assert!(!shim_resolves_under(&cmd, &bin), "{cmd}");
}
