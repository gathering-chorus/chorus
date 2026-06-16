//! #3437 — the bin is a thin self-check/smoke over the lib. Red-first (DEC-1674):
//! the binary must run the cascade over a known fixture and report PASS, so a
//! deployed copy is provably runnable (not a no-op ceremony binary).

use std::process::Command;

#[test]
fn selfcheck_binary_resolves_and_reports_pass() {
    let bin = env!("CARGO_BIN_EXE_properties-resolver");
    let out = Command::new(bin).arg("--selfcheck").output().expect("run the bin");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "selfcheck must exit 0; stdout={stdout} stderr={stderr}");
    assert!(
        stdout.contains("PASS"),
        "selfcheck must report PASS (it ran the cascade over the fixture); got: {stdout}"
    );
}
