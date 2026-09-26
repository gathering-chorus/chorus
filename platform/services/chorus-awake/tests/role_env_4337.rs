//! #4337 — a role's process must carry its own role. Kade ran as Wren on
//! 2026-09-26 because his session lived in a daemon spare started from her pane.
use chorus_awake::rows::env_value;

#[test]
fn reads_the_role_a_process_carries() {
    let ps = "/h/.local/bin/claude -c PWD=/x/roles/wren CHORUS_ROLE=wren DEPLOY_ROLE=wren";
    assert_eq!(env_value(ps, "CHORUS_ROLE").as_deref(), Some("wren"));
}

#[test]
fn a_prefix_of_another_name_is_not_the_variable() {
    assert_eq!(env_value("X_CHORUS_ROLE=kade CHORUS_ROLE_OLD=silas", "CHORUS_ROLE"), None);
}

#[test]
fn absent_or_empty_is_none_so_an_unreadable_process_is_never_called_wrong() {
    assert_eq!(env_value("claude -c", "CHORUS_ROLE"), None);
    assert_eq!(env_value("CHORUS_ROLE=", "CHORUS_ROLE"), None);
}
