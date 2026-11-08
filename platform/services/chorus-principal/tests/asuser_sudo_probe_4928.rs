// @domain: identity
//! #4928 — asuser probe and launch commands must match platform/config/chorus-role-asuser.sudoers line by line.
//! Pinned: /usr/bin/sudo after the uid, never bare sudo or ctx.sudo.
use chorus_principal::asuser_probe_args;
use std::fs;
use std::path::PathBuf;

#[test]
fn asuser_probe_pins_usr_bin_sudo_after_uid() {
    let args = asuser_probe_args("502", "chorus-silas");
    assert_eq!(args[0], "-n");
    assert_eq!(args[1], "/bin/launchctl");
    assert_eq!(args[2], "asuser");
    assert_eq!(args[3], "502");
    assert_eq!(
        args[4], "/usr/bin/sudo",
        "inner sudo must be /usr/bin/sudo, not bare sudo or ctx.sudo (#4928)"
    );
    assert_eq!(args[5], "-n");
    assert_eq!(args[6], "-u");
    assert_eq!(args[7], "chorus-silas");
    assert_eq!(args[8], "-H");
    assert_eq!(args[9], "bash");
    assert_eq!(args[10], "-c");
    assert_eq!(args[11], "true");
}

fn matches_sudoers_rule(probe_args: &[&str], sudoers: &str) -> bool {
    let probe_cmd = probe_args[1..11].join(" ");
    let expected_rule = format!("jeffbridwell ALL=(root) NOPASSWD: {} *", probe_cmd);
    sudoers.lines().any(|l| l.trim() == expected_rule)
}

#[test]
fn asuser_probe_matches_repo_sudoers_line_by_line() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let sudoers_path = repo_root.join("platform/config/chorus-role-asuser.sudoers");
    let sudoers = fs::read_to_string(&sudoers_path)
        .expect("should read platform/config/chorus-role-asuser.sudoers");

    let roles = [
        ("502", "chorus-silas"),
        ("503", "chorus-wren"),
        ("504", "chorus-kade"),
        ("505", "chorus-abby-normal"),
        ("507", "chorus-borg-queen"),
    ];

    for (uid, acct) in roles {
        let args = asuser_probe_args(uid, acct);
        assert!(
            matches_sudoers_rule(&args, &sudoers),
            "sudoers missing exact line for {} ({})",
            acct,
            uid
        );
    }
}

#[test]
fn negative_proof_bare_sudo_does_not_match_sudoers() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let sudoers_path = repo_root.join("platform/config/chorus-role-asuser.sudoers");
    let sudoers = fs::read_to_string(&sudoers_path)
        .expect("should read platform/config/chorus-role-asuser.sudoers");

    // Silas review #4928: take real function output, mutate [4] to "sudo",
    // and verify the sudoers matcher rejects it (negative proof).
    let mut args = asuser_probe_args("502", "chorus-silas");
    args[4] = "sudo";

    assert!(
        !matches_sudoers_rule(&args, &sudoers),
        "asuser_probe_args with bare sudo must NOT match sudoers"
    );
}
