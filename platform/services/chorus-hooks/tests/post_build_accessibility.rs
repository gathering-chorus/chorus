//! #2059 — Post-build must detect Accessibility permission status.
//!
//! Bug: cargo build --release changes binary hash, macOS revokes
//! Accessibility permission, osascript inject silently fails.
//! Fix: post-build script detects and warns.

use std::process::Command;

const SHIM: &str = "/Users/jeffbridwell/CascadeProjects/chorus/platform/services/chorus-hooks/target/release/chorus-hook-shim";
const POST_BUILD: &str = "/Users/jeffbridwell/CascadeProjects/chorus/scripts/post-cargo-build.sh";

#[test]
fn post_build_script_exists_and_is_executable() {
    let exists = std::path::Path::new(POST_BUILD).exists();
    assert!(exists, "post-cargo-build.sh must exist at {}", POST_BUILD);

    let metadata = std::fs::metadata(POST_BUILD).expect("should read metadata");
    use std::os::unix::fs::PermissionsExt;
    let mode = metadata.permissions().mode();
    assert!(mode & 0o111 != 0, "post-cargo-build.sh must be executable, mode: {:o}", mode);
}

#[test]
fn post_build_script_passes_when_accessibility_granted() {
    // When Accessibility is granted (current state after Jeff toggled),
    // the script should exit 0 with success message
    let output = Command::new("bash")
        .arg(POST_BUILD)
        .output()
        .expect("failed to run post-cargo-build.sh");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "should pass when Accessibility is granted, got: {}", stdout);
    assert!(stdout.contains("Accessibility permission OK"), "should print OK message, got: {}", stdout);
}

#[test]
fn shim_nudge_inject_works_after_build() {
    // AC2: nudge --force inject works immediately after rebuild
    let output = Command::new(SHIM)
        .args(["nudge", "silas", "post-build accessibility test", "--force"])
        .output()
        .expect("failed to run nudge");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should deliver, not fail with inject error
    assert!(stdout.contains("DELIVERED"), "nudge should deliver after build, got: {}", stdout);
    assert!(!stdout.contains("INJECT FAILED"), "inject should not fail, got: {}", stdout);
}
