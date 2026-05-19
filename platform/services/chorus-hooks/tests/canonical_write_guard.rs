//! Canonical write guard tests (#2735, #2913).
//!
//! Edit/Write to canonical (`$CHORUS_HOME/...`) is refused — work belongs in
//! the role's worktree, not the shared canonical tree. Edit/Write to another
//! role's werk is also refused (cross-role contamination).
//!
//! #2913: werks are ephemeral per-card — `/chorus-werk/<role>-<card>/`. The
//! cross-role check parses the owning role as the segment before the first
//! `-`, so a role editing inside its own card werk is allowed and a write
//! into another role's card werk is still refused. A bare `<role>/` slot
//! (pre-#2913 persistent model) still parses — no `-` — for migration.
//!
//! When the role env isn't set (bootstrap / generic shell), the guard is
//! silent so existing flows aren't disrupted.

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

/// Helper: set CHORUS_HOME + <ROLE>_WERK + CHORUS_ROLE + CHORUS_WERK_BASE for
/// the duration of a closure. `role_werk` is the role's ephemeral per-card
/// werk path (#2913: chorus-werk/<role>-<card>/); CHORUS_WERK_BASE is derived
/// as its parent. The guard reads env once per call.
fn with_env<F: FnOnce()>(canonical: &str, role: &str, role_werk: &str, body: F) {
    let role_var = format!("{}_WERK", role.to_uppercase());
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
        "/Users/jeff/CascadeProjects/chorus-werk/kade-2913",
        || {
            let input = write_input("/Users/jeff/CascadeProjects/chorus/platform/scripts/foo.sh");
            let r = canonical_write_guard::check(&input);
            assert!(r.stdout.is_some(), "expected refusal, got allow");
            let msg = r.stdout.unwrap_or_default();
            assert!(msg.contains("canonical"), "refusal must name canonical: {msg}");
            assert!(
                msg.contains("/chorus-werk/kade-2913/platform/scripts/foo.sh"),
                "refusal must suggest the ephemeral werk path: {msg}"
            );
        },
    );
}

#[test]
fn write_to_own_ephemeral_werk_allowed() {
    // #2913 core fix: a role editing inside its own chorus-werk/<role>-<card>/
    // worktree is allowed. Pre-patch the guard parsed the slot "kade-2913" as
    // the role, saw "kade-2913" != "kade", and DENIED every edit a role made
    // in its own werk.
    with_env(
        "/Users/jeff/CascadeProjects/chorus",
        "kade",
        "/Users/jeff/CascadeProjects/chorus-werk/kade-2913",
        || {
            let input = write_input(
                "/Users/jeff/CascadeProjects/chorus-werk/kade-2913/platform/scripts/foo.sh",
            );
            let r = canonical_write_guard::check(&input);
            assert!(r.stdout.is_none(), "expected allow for own ephemeral werk, got refusal");
            assert_eq!(r.exit_code, 0);
        },
    );
}

#[test]
fn write_to_own_second_card_werk_allowed() {
    // #2913 enables >1 card per role. kade working on card 2914 while a
    // 2913 werk also exists — both kade-* slots parse to role "kade", both
    // allowed.
    with_env(
        "/Users/jeff/CascadeProjects/chorus",
        "kade",
        "/Users/jeff/CascadeProjects/chorus-werk/kade-2913",
        || {
            let input = write_input(
                "/Users/jeff/CascadeProjects/chorus-werk/kade-2914/platform/api/x.ts",
            );
            let r = canonical_write_guard::check(&input);
            assert!(r.stdout.is_none(), "kade's other card werk must also be allowed");
        },
    );
}

#[test]
fn write_to_bare_role_slot_still_allowed_for_migration() {
    // Heterogeneous migration window: a pre-#2913 persistent chorus-werk/kade/
    // slot has no '-', so the whole segment is the role — still parses as own.
    with_env(
        "/Users/jeff/CascadeProjects/chorus",
        "kade",
        "/Users/jeff/CascadeProjects/chorus-werk/kade-2913",
        || {
            let input = write_input("/Users/jeff/CascadeProjects/chorus-werk/kade/foo.md");
            let r = canonical_write_guard::check(&input);
            assert!(r.stdout.is_none(), "bare <role>/ slot must still parse as own werk");
        },
    );
}

#[test]
fn write_to_other_role_ephemeral_werk_refused() {
    with_env(
        "/Users/jeff/CascadeProjects/chorus",
        "kade",
        "/Users/jeff/CascadeProjects/chorus-werk/kade-2913",
        || {
            let input = write_input(
                "/Users/jeff/CascadeProjects/chorus-werk/wren-3000/platform/scripts/foo.sh",
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
        "/Users/jeff/CascadeProjects/chorus-werk/kade-2913",
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
        "/Users/jeff/CascadeProjects/chorus-werk/kade-2913",
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
        "/Users/jeff/CascadeProjects/chorus-werk/kade-2913",
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

#[test]
fn guard_active_regardless_of_feature_flag() {
    // #2908 (2026-05-11): the CHORUS_WERK_ENABLE feature flag is retired. The
    // guard fires whenever the role is determinable, full stop — no flag.
    // This is the regression guard for that contract; #2913 doesn't change it.
    std::env::set_var("CHORUS_HOME", "/Users/jeff/CascadeProjects/chorus");
    std::env::set_var("CHORUS_ROLE", "kade");
    std::env::set_var("KADE_WERK", "/Users/jeff/CascadeProjects/chorus-werk/kade-2913");
    std::env::set_var("CHORUS_WERK_BASE", "/Users/jeff/CascadeProjects/chorus-werk");

    // Flag missing → guard MUST still refuse.
    std::env::remove_var("CHORUS_WERK_ENABLE");
    let input = write_input("/Users/jeff/CascadeProjects/chorus/platform/scripts/foo.sh");
    let r = canonical_write_guard::check(&input);
    assert!(
        r.stdout.is_some(),
        "without CHORUS_WERK_ENABLE, guard must STILL refuse — flag retired #2908"
    );

    // Flag set to any non-"1" value → still refuses.
    for val in ["", "0", "true", "yes", "false", "no"] {
        std::env::set_var("CHORUS_WERK_ENABLE", val);
        let input = write_input("/Users/jeff/CascadeProjects/chorus/platform/scripts/foo.sh");
        let r = canonical_write_guard::check(&input);
        assert!(
            r.stdout.is_some(),
            "CHORUS_WERK_ENABLE='{val}' must STILL refuse — flag retired #2908"
        );
    }

    // Flag set to "1" → still refuses (regression check).
    std::env::set_var("CHORUS_WERK_ENABLE", "1");
    let input = write_input("/Users/jeff/CascadeProjects/chorus/platform/scripts/foo.sh");
    let r = canonical_write_guard::check(&input);
    assert!(
        r.stdout.is_some(),
        "with CHORUS_WERK_ENABLE=1, guard refuses (regression check)"
    );

    std::env::remove_var("CHORUS_HOME");
    std::env::remove_var("CHORUS_ROLE");
    std::env::remove_var("KADE_WERK");
    std::env::remove_var("CHORUS_WERK_BASE");
    std::env::remove_var("CHORUS_WERK_ENABLE");
}

// --- #3003: no-WIP /reboot role-state allowlist -----------------------------
//
// /reboot with no WIP card writes role-state files to canonical
// (roles/<role>/next-session.md, current-work.md, tech-debt.md, briefs/*).
// Before #3003 the guard refused these writes with a fabricated redirect path
// (#2913 retired the persistent per-role werk model, so there is no werk to
// redirect to). The fix: allowlist these specific role-state paths when
// <ROLE>_WERK is empty. When <ROLE>_WERK IS set (WIP in flight), existing
// redirect behavior is preserved — the werk's role-state mirror is the right
// target during an active card.

/// Helper for no-WIP cases: set CHORUS_HOME + CHORUS_ROLE + CHORUS_WERK_BASE
/// but NOT <ROLE>_WERK. Mirrors what chorus-env-setup.sh does when zero werks
/// match `<role>-*` (no-WIP /reboot or no card pulled this session).
fn with_no_wip_env<F: FnOnce()>(canonical: &str, role: &str, werk_base: &str, body: F) {
    let role_var = format!("{}_WERK", role.to_uppercase());
    std::env::set_var("CHORUS_HOME", canonical);
    std::env::set_var("CHORUS_ROLE", role);
    std::env::set_var("CHORUS_WERK_BASE", werk_base);
    std::env::remove_var(&role_var);
    body();
    std::env::remove_var("CHORUS_HOME");
    std::env::remove_var("CHORUS_ROLE");
    std::env::remove_var("CHORUS_WERK_BASE");
}

#[test]
fn no_wip_reboot_allows_next_session_md_to_canonical() {
    with_no_wip_env(
        "/Users/jeff/CascadeProjects/chorus",
        "silas",
        "/Users/jeff/CascadeProjects/chorus-werk",
        || {
            let input =
                write_input("/Users/jeff/CascadeProjects/chorus/roles/silas/next-session.md");
            let r = canonical_write_guard::check(&input);
            assert!(
                r.stdout.is_none(),
                "no-WIP /reboot must allow role-state write to canonical; got refusal: {:?}",
                r.stdout
            );
        },
    );
}

#[test]
fn no_wip_reboot_allows_full_role_state_allowlist() {
    let cases = [
        "roles/kade/next-session.md",
        "roles/kade/next-session.md.consumed",
        "roles/kade/current-work.md",
        "roles/kade/tech-debt.md",
        "roles/kade/stories.md",
        "roles/kade/decisions.md",
        "roles/kade/briefs/incoming-from-wren.md",
        "roles/kade/briefs/sub/dir/note.md",
    ];
    for rel in cases {
        with_no_wip_env(
            "/Users/jeff/CascadeProjects/chorus",
            "kade",
            "/Users/jeff/CascadeProjects/chorus-werk",
            || {
                let path = format!("/Users/jeff/CascadeProjects/chorus/{rel}");
                let input = write_input(&path);
                let r = canonical_write_guard::check(&input);
                assert!(
                    r.stdout.is_none(),
                    "no-WIP allowlist file {rel} must allow; got refusal: {:?}",
                    r.stdout
                );
            },
        );
    }
}

#[test]
fn no_wip_reboot_refuses_non_allowlist_canonical_write() {
    with_no_wip_env(
        "/Users/jeff/CascadeProjects/chorus",
        "kade",
        "/Users/jeff/CascadeProjects/chorus-werk",
        || {
            let input =
                write_input("/Users/jeff/CascadeProjects/chorus/platform/scripts/foo.sh");
            let r = canonical_write_guard::check(&input);
            assert!(
                r.stdout.is_some(),
                "no-WIP must STILL refuse arbitrary canonical writes — allowlist is narrow"
            );
        },
    );
}

#[test]
fn no_wip_reboot_refuses_other_role_state_write() {
    with_no_wip_env(
        "/Users/jeff/CascadeProjects/chorus",
        "kade",
        "/Users/jeff/CascadeProjects/chorus-werk",
        || {
            let input =
                write_input("/Users/jeff/CascadeProjects/chorus/roles/wren/next-session.md");
            let r = canonical_write_guard::check(&input);
            assert!(
                r.stdout.is_some(),
                "kade writing wren's role-state must refuse — allowlist is per-role"
            );
        },
    );
}

#[test]
fn no_wip_reboot_refuses_non_allowlist_file_under_own_role_dir() {
    with_no_wip_env(
        "/Users/jeff/CascadeProjects/chorus",
        "kade",
        "/Users/jeff/CascadeProjects/chorus-werk",
        || {
            let input =
                write_input("/Users/jeff/CascadeProjects/chorus/roles/kade/scratch.md");
            let r = canonical_write_guard::check(&input);
            assert!(
                r.stdout.is_some(),
                "non-allowlisted file under role dir must refuse — allowlist is filename-strict"
            );
        },
    );
}

#[test]
fn wip_in_flight_role_state_still_redirects_to_werk() {
    // AC3 regression check: when <ROLE>_WERK is set, writes to canonical
    // role-state still redirect to the werk's mirror.
    with_env(
        "/Users/jeff/CascadeProjects/chorus",
        "kade",
        "/Users/jeff/CascadeProjects/chorus-werk/kade-3003",
        || {
            let input =
                write_input("/Users/jeff/CascadeProjects/chorus/roles/kade/next-session.md");
            let r = canonical_write_guard::check(&input);
            assert!(
                r.stdout.is_some(),
                "WIP-in-flight role-state write must still refuse + redirect; got allow"
            );
            let msg = r.stdout.unwrap_or_default();
            assert!(
                msg.contains("/chorus-werk/kade-3003/roles/kade/next-session.md"),
                "refusal must redirect to werk mirror; got: {msg}"
            );
        },
    );
}

#[test]
fn cross_role_write_refused_regardless_of_flag() {
    // Companion to guard_active_regardless_of_feature_flag — cross-role
    // writes also refuse without depending on the flag. Ephemeral path shape.
    std::env::set_var("CHORUS_HOME", "/Users/jeff/CascadeProjects/chorus");
    std::env::set_var("CHORUS_ROLE", "kade");
    std::env::set_var("KADE_WERK", "/Users/jeff/CascadeProjects/chorus-werk/kade-2913");
    std::env::set_var("CHORUS_WERK_BASE", "/Users/jeff/CascadeProjects/chorus-werk");
    std::env::remove_var("CHORUS_WERK_ENABLE");

    let cross = write_input("/Users/jeff/CascadeProjects/chorus-werk/wren-3000/foo.md");
    let r = canonical_write_guard::check(&cross);
    assert!(
        r.stdout.is_some(),
        "cross-role write must refuse without flag dependency"
    );
    let msg = r.stdout.unwrap_or_default();
    assert!(msg.contains("cross-role"), "refusal must name cross-role: {msg}");

    std::env::remove_var("CHORUS_HOME");
    std::env::remove_var("CHORUS_ROLE");
    std::env::remove_var("KADE_WERK");
    std::env::remove_var("CHORUS_WERK_BASE");
}
