// @domain: spine — placed by chorus-crawl's domain rules (#4538)
//! #2270: hook-shim demo gate removed from PreToolUse chain.
//!
//! The hook-shim was blocking `cards done` without demo evidence, duplicating
//! a check that the cards CLI SDK already owns. This caused false blocks even
//! when card.demo.started was in the log (the hook checked brief files,
//! the CLI checked spine events — they disagreed).
//!
//! After #2270: demo_gate::check is NOT called from pre_tool_use_inner for
//! Bash or Skill tool inputs. cards CLI is single enforcement point.

use std::process::Command;
use chorus_hooks::shared::state_paths::chorus_root;

/// Before #2270: `cards done` on a card without a brief file was blocked by
/// the hook-shim via done-gate.sh, even if card.demo.started spine event exists.
/// After #2270: hook-shim allows all `cards done` — CLI gate is the only check.
///
/// This test verifies the hook-shim binary allows the command without blocking.
/// It will FAIL before the fix (hook blocks) and PASS after (hook allows).
#[test]
fn hook_shim_allows_cards_done_without_hook_gate() {
    let shim = env!("CARGO_BIN_EXE_chorus-hook-shim");
    let daemon = env!("CARGO_BIN_EXE_chorus-hooks");

    let tmp = tempfile::tempdir().expect("tempdir");
    let tmp_path = tmp.path().to_str().expect("tmp path");

    // Spawn isolated daemon in tmp dir for hermetic testing (#4785)
    let mut daemon_child = Command::new(daemon)
        .env("CHORUS_ROOT", chorus_root())
        .env("CHORUS_HOOKS_RUN_DIR", tmp_path)
        .env("CHORUS_LOG_FILE", tmp.path().join("chorus.log"))
        .env("HOME", tmp_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn chorus-hooks daemon");

    let sock_path = tmp.path().join("chorus-hooks.sock");
    let started = std::time::Instant::now();
    let mut socket_ready = false;
    while started.elapsed() < std::time::Duration::from_secs(5) {
        if sock_path.exists() {
            #[cfg(unix)]
            {
                use std::os::unix::net::UnixStream;
                if UnixStream::connect(&sock_path).is_ok() {
                    socket_ready = true;
                    break;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(socket_ready, "chorus-hooks daemon failed to create socket in {tmp_path}");

    // Simulate a PreToolUse call for `cards done 2270`
    let input = serde_json::json!({
        "tool_name": "Bash",
        "tool_input": { "command": "bash /path/to/cards done 2270" },
        "session_id": "test-session",
        "cwd": format!("{}/roles/silas", chorus_root()),
        "deploy_role": "silas"
    });

    let mut cmd = Command::new(shim);
    cmd.args(["pre-tool-use"])
        .env("CHORUS_ROOT", chorus_root())
        .env("CHORUS_HOOKS_RUN_DIR", tmp_path)
        .env("CHORUS_LOG_FILE", tmp.path().join("chorus.log"))
        .env("HOME", tmp_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().expect("spawn chorus-hook-shim");
    use std::io::Write;
    if let Some(mut stdin) = child.stdin.take() {
        writeln!(stdin, "{}", input).expect("write to shim stdin");
    }
    let result = child.wait_with_output().expect("wait for shim");
    let stdout = String::from_utf8_lossy(&result.stdout);

    let _ = daemon_child.kill();
    let _ = daemon_child.wait();

    // After #2270: hook must NOT contain a deny decision for cards done
    assert!(
        !stdout.contains("\"deny\"") && !stdout.contains("Demo gate"),
        "hook-shim should not block cards done after #2270 removal. Got: {}",
        stdout
    );
}
