// @test-type: unit — runs verify-install.sh against a user, binary and log that do not exist; read-only, no sudo prompt.
// @domain: security
// @card: #4652
// @owner: wren
//! #4652 — chorus-jeff-signer/verify-install.sh is the check that Jeff's signer install is a real wall.
//! It must never read PASS for a wall that is not there (#3734: no vacuous pass).
use std::process::Command;

fn run(user: &str, bin: &str, log: &str) -> (i32, String) {
    // Run time, never compile time (the 4030 rule).
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR for tests");
    let out = Command::new("bash").arg(format!("{dir}/verify-install.sh"))
        .env("CHORUS_SIGNER_USER", user).env("CHORUS_SIGNER_BIN", bin).env("CHORUS_SIGNER_LOG", log)
        .output().unwrap();
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).into_owned())
}

fn line<'a>(out: &'a str, check: &str) -> &'a str {
    out.lines().find(|l| l.ends_with(check)).unwrap_or_else(|| panic!("no line for {check:?} in:\n{out}"))
}

const NOBODY: &str = "chorus-no-such-user-4652";

#[test]
fn negative_proof_no_install_reads_fail_on_every_wall_line() {
    let (rc, out) = run(NOBODY, "/nonexistent/chorus-jeff-signer-4652", "/nonexistent/signer-4652.log");
    assert_ne!(rc, 0, "a missing install must exit non-zero:\n{out}");
    for check in ["the user exists", "a role cannot sudo to it without a password",
                  &format!("its home is {NOBODY}'s own, mode 700"), "a role cannot list its home",
                  "a role cannot read the key", "the binary is root-owned and not writable by us",
                  "the log shows it authenticated"] {
        assert!(line(&out, check).starts_with("FAIL "), "{check} must FAIL with nothing installed:\n{out}");
    }
}

#[test]
fn a_root_owned_binary_the_role_cannot_write_reads_pass() {
    // The same lines can go green: /bin/ls is root-owned and not ours to write.
    let (_, out) = run(NOBODY, "/bin/ls", "/nonexistent/signer-4652.log");
    assert!(line(&out, "the binary is root-owned and not writable by us").starts_with("PASS "), "{out}");
    assert!(line(&out, "its folder is root-owned").starts_with("PASS "), "{out}");
}
