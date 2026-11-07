// @domain: spine — placed by chorus-crawl's domain rules (#4538)
//! #4337 — a role's process must carry its own role. Kade ran as Wren on
//! 2026-09-26 because his session lived in a daemon spare started from her pane.
use chorus_principal::rows::env_value;

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

// #4807 AC6 — a role account's pane is started by a shell line that sets CHORUS_ROLE='silas'.
// Jeff's login cannot read another account's environment, so `ps eww` shows only that command
// line, quotes included; login read 'silas', called it a different role and refused Silas
// (2026-10-30 16:00, "WRONG ROLE — pid 55482 still runs as 'silas'").
#[test]
fn a_quoted_value_from_the_launch_line_is_the_role_without_its_quotes() {
    let ps = "bash -c umask 002 && cd '/x/roles/silas' && export CHORUS_ROLE='silas' GIT_CONFIG_COUNT='1' && claude -c";
    assert_eq!(env_value(ps, "CHORUS_ROLE").as_deref(), Some("silas"));
    assert_eq!(env_value("export CHORUS_ROLE=\"wren\" X=1", "CHORUS_ROLE").as_deref(), Some("wren"));
}

#[test]
fn only_a_matched_pair_of_quotes_is_stripped() {
    assert_eq!(env_value("CHORUS_ROLE='silas", "CHORUS_ROLE").as_deref(), Some("'silas"));
    assert_eq!(env_value("CHORUS_ROLE=''", "CHORUS_ROLE"), None);
}

// NEGATIVE PROOF (Wren): when the environment is readable it wins over the launch line, so a
// process launched as silas but carrying kade is kade.
#[test]
fn a_readable_environment_wins_over_the_launch_line() {
    let ps = "bash -c export CHORUS_ROLE='silas' && claude -c PWD=/x CHORUS_ROLE=kade HOME=/h";
    assert_eq!(env_value(ps, "CHORUS_ROLE").as_deref(), Some("kade"));
}
