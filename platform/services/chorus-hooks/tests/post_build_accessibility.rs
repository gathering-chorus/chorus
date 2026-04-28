//! #2059 — Post-build must detect Accessibility permission status.
//!
//! Bug: cargo build --release changes binary hash, macOS revokes
//! Accessibility permission, osascript inject silently fails.
//! Fix: post-build script detects and warns.

use std::process::Command;
use chorus_hooks::shared::state_paths::chorus_root;

fn post_build() -> String { format!("{}/proving/scripts/post-cargo-build.sh", chorus_root()) }
#[test]
fn post_build_script_exists_and_is_executable() {
    let exists = std::path::Path::new(&post_build()).exists();
    assert!(exists, "post-cargo-build.sh must exist at {}", post_build());

    let metadata = std::fs::metadata(&post_build()).expect("should read metadata");
    use std::os::unix::fs::PermissionsExt;
    let mode = metadata.permissions().mode();
    assert!(mode & 0o111 != 0, "post-cargo-build.sh must be executable, mode: {:o}", mode);
}

// macOS-only: Accessibility permission is a macOS concept (System Settings →
// Privacy & Security → Accessibility) used by osascript to drive Terminal.app.
// Linux has no equivalent; the script's accessibility-detection branch only
// makes sense on Mac.
#[cfg(target_os = "macos")]
#[test]
fn post_build_script_passes_when_accessibility_granted() {
    // When Accessibility is granted (current state after Jeff toggled),
    // the script should exit 0 with success message
    let output = Command::new("bash")
        .arg(post_build())
        .output()
        .expect("failed to run post-cargo-build.sh");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "should pass when Accessibility is granted, got: {}", stdout);
    assert!(stdout.contains("Accessibility permission OK"), "should print OK message, got: {}", stdout);
}

// (`shim_nudge_inject_works_after_build` removed per #2166 — the accessibility
// regression from #2059 is caught by `post_build_script_passes_when_accessibility_granted`
// above without firing a live inject. Previously gated behind HERMETIC_TEST_MODE.)
