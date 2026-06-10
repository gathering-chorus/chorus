//! Pure-helper unit tests (#3300) — the CLI seam, the porcelain stash-set parse
//! (Bash `grep -E '^[ MAD]'` parity incl. the deliberate untracked exclusion),
//! the manifest row format, and the chorus-log positional contract.

use werk_sync::{dirty_paths, manifest_line, parse_sync_args, spine_args, Mode, USAGE};

fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn parse_recognizes_repair_and_recover_and_nothing_else() {
    assert_eq!(parse_sync_args(&args(&["repair"])).unwrap(), Mode::Repair);
    assert_eq!(parse_sync_args(&args(&["recover"])).unwrap(), Mode::Recover);
    // No default mode: sync-proper was retired by #2863 — bare invocation is usage.
    assert_eq!(parse_sync_args(&args(&[])).unwrap_err(), USAGE);
    assert!(parse_sync_args(&args(&["sync"])).is_err(), "no resurrected sync verb");
    assert!(parse_sync_args(&args(&["--help"])).is_err());
}

#[test]
fn dirty_paths_takes_m_a_d_and_excludes_untracked() {
    let porcelain = " M activity.md\nM  staged.md\nA  added.md\n D deleted.md\n?? brand-new.txt\n?? ops/\n";
    let got = dirty_paths(porcelain);
    let paths: Vec<&str> = got.iter().map(|(_, p)| p.as_str()).collect();
    assert_eq!(paths, vec!["activity.md", "staged.md", "added.md", "deleted.md"]);
    // untracked ?? is DELIBERATELY excluded (Bash parity): untracked files survive
    // an ff-merge untouched; stashing them would churn work for nothing.
    assert!(!paths.contains(&"brand-new.txt"));
}

#[test]
fn dirty_paths_ignores_short_and_empty_lines() {
    assert!(dirty_paths("").is_empty());
    assert!(dirty_paths("M\n \n").is_empty());
}

#[test]
fn manifest_row_is_hash_tab_path_newline() {
    assert_eq!(manifest_line("abc123def456", "roles/kade/x.md"), "abc123def456\troles/kade/x.md\n");
}

#[test]
fn spine_args_carry_event_system_role_and_extras() {
    // Bash emit() parity: role is always `system`, extras are k=v.
    let a = spine_args("canonical.recovery.completed", &[("recovered", "3"), ("to", "abc1234")]);
    assert_eq!(a, vec!["canonical.recovery.completed", "system", "recovered=3", "to=abc1234"]);
}
