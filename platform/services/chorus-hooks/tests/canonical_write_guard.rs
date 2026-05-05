//! Canonical write guard tests (#2735).
//!
//! Edit/Write to canonical (`$CHORUS_HOME/...`) is refused when the role's
//! werk is initialized — work belongs in the role's worktree, not in the
//! shared canonical tree. Edit/Write to another role's werk is also
//! refused (cross-role contamination).
//!
//! When the role env isn't set (e.g., bootstrap before migration, or a
//! generic shell session), the guard is silent so existing flows aren't
//! disrupted.

use chorus_hooks::canonical_write_guard;
use chorus_hooks::HookInput;
use serde_json::json;

fn make_input(tool: &str, input_json: serde_json::Value) -> HookInput {
    HookInput {
        tool_name: Some(tool.to_string()),
        tool_input: Some(input_json),
        tool_response: None,
        session_id: None,
        cwd: None,
        prompt: None,
        stop_hook_active: None,
        hook_type: None,
        deploy_role: None,
        chorus_worktree_override: None,
    }
}

fn write_input(file_path: &str) -> HookInput {
    make_input("Write", json!({"file_path": file_path, "content": "x"}))
}

fn edit_input(file_path: &str) -> HookInput {
    make_input(
        "Edit",
        json!({"file_path": file_path, "old_string": "a", "new_string": "b"}),
    )
}

fn read_input(file_path: &str) -> HookInput {
    make_input("Read", json!({"file_path": file_path}))
}

/// Helper: set CHORUS_HOME + <ROLE>_WERK + CHORUS_ROLE for the duration of a closure.
/// Tests run serially (cargo test single thread when env-driven) — but the
/// guard reads env once per call, so set/unset around each block.
fn with_env<F: FnOnce()>(canonical: &str, role: &str, role_werk: &str, body: F) {
    let role_var = format!("{}_WERK", role.to_uppercase());
    // Derive CHORUS_WERK_BASE from the role's werk path: parent dir.
    let werk_base = std::path::Path::new(role_werk)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    std::env::set_var("CHORUS_HOME", canonical);
    std::env::set_var("CHORUS_ROLE", role);
    std::env::set_var(&role_var, role_werk);
    std::env::set_var("CHORUS_WERK_BASE", &werk_base);
    body();
    std::env::remove_var("CHORUS_HOME");
    std::env::remove_var("CHORUS_ROLE");
    std::env::remove_var(&role_var);
    std::env::remove_var("CHORUS_WERK_BASE");
}

#[test]
fn write_to_canonical_refused_with_redirect_message() {
    with_env(
        "/Users/jeff/CascadeProjects/chorus",
        "kade",
        "/Users/jeff/CascadeProjects/chorus-werk/kade",
        || {
            let input = write_input("/Users/jeff/CascadeProjects/chorus/platform/scripts/foo.sh");
            let r = canonical_write_guard::check(&input);
            assert!(r.stdout.is_some(), "expected refusal, got allow");
            let msg = r.stdout.unwrap_or_default();
            assert!(msg.contains("canonical"), "refusal must name canonical: {msg}");
            assert!(
                msg.contains("/chorus-werk/kade/platform/scripts/foo.sh"),
                "refusal must suggest the werk path: {msg}"
            );
        },
    );
}

#[test]
fn write_to_own_werk_allowed() {
    with_env(
        "/Users/jeff/CascadeProjects/chorus",
        "kade",
        "/Users/jeff/CascadeProjects/chorus-werk/kade",
        || {
            let input =
                write_input("/Users/jeff/CascadeProjects/chorus-werk/kade/platform/scripts/foo.sh");
            let r = canonical_write_guard::check(&input);
            assert!(r.stdout.is_none(), "expected allow, got refusal");
            assert_eq!(r.exit_code, 0);
        },
    );
}

#[test]
fn write_to_other_role_werk_refused() {
    with_env(
        "/Users/jeff/CascadeProjects/chorus",
        "kade",
        "/Users/jeff/CascadeProjects/chorus-werk/kade",
        || {
            let input = write_input(
                "/Users/jeff/CascadeProjects/chorus-werk/wren/platform/scripts/foo.sh",
            );
            let r = canonical_write_guard::check(&input);
            assert!(r.stdout.is_some(), "expected cross-role refusal");
            let msg = r.stdout.unwrap_or_default();
            assert!(msg.contains("wren"), "refusal must name the other role: {msg}");
            assert!(msg.contains("cross-role"), "refusal must mark it cross-role: {msg}");
        },
    );
}

#[test]
fn edit_to_canonical_also_refused() {
    with_env(
        "/Users/jeff/CascadeProjects/chorus",
        "kade",
        "/Users/jeff/CascadeProjects/chorus-werk/kade",
        || {
            let input = edit_input("/Users/jeff/CascadeProjects/chorus/CLAUDE.md");
            let r = canonical_write_guard::check(&input);
            assert!(r.stdout.is_some(), "expected refusal on Edit too");
        },
    );
}

#[test]
fn read_to_canonical_allowed() {
    with_env(
        "/Users/jeff/CascadeProjects/chorus",
        "kade",
        "/Users/jeff/CascadeProjects/chorus-werk/kade",
        || {
            let input = read_input("/Users/jeff/CascadeProjects/chorus/CLAUDE.md");
            let r = canonical_write_guard::check(&input);
            assert!(r.stdout.is_none(), "Read of canonical must be allowed");
        },
    );
}

#[test]
fn write_to_tmp_allowed() {
    with_env(
        "/Users/jeff/CascadeProjects/chorus",
        "kade",
        "/Users/jeff/CascadeProjects/chorus-werk/kade",
        || {
            let input = write_input("/tmp/sketch.html");
            let r = canonical_write_guard::check(&input);
            assert!(r.stdout.is_none(), "Write to /tmp must remain allowed (sketch surface)");
        },
    );
}

#[test]
fn no_role_env_means_no_refusal() {
    // Bootstrap / migration / generic shell — guard is silent.
    std::env::remove_var("CHORUS_HOME");
    std::env::remove_var("CHORUS_ROLE");
    std::env::remove_var("KADE_WERK");
    std::env::remove_var("WREN_WERK");
    std::env::remove_var("SILAS_WERK");
    let input = write_input("/Users/jeff/CascadeProjects/chorus/platform/scripts/foo.sh");
    let r = canonical_write_guard::check(&input);
    assert!(
        r.stdout.is_none(),
        "without role env, guard must allow — bootstrap and migration paths depend on this"
    );
}
