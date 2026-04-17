//! Test: preflight.sh needs PATH to run the cards CLI.
//!
//! Bug: demo_preflight.rs spawns preflight.sh with only CHORUS_ROOT in env.
//! The cards CLI is a bash wrapper around TypeScript — needs node in PATH.
//! Without PATH, `cards view` fails, preflight reads failure as "card not found",
//! and blocks every /demo invocation. 31 consecutive false denials on real cards.
//!
//! Fix: add .env("PATH", ...) to the Command spawn, matching search_hierarchy.rs.

use std::process::Command;

fn chorus_root() -> String {
    std::env::var("CHORUS_ROOT")
        .unwrap_or_else(|_| "/Users/jeffbridwell/CascadeProjects/chorus".to_string())
}

fn home() -> String {
    std::env::var("HOME")
        .unwrap_or_else(|_| "/Users/jeffbridwell".to_string())
}

fn full_path() -> String {
    format!(
        "{}/CascadeProjects/chorus/platform/scripts:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin",
        home()
    )
}

/// Pick the first currently-WIP card id dynamically (#2130 fix).
/// Hardcoded card ids go stale the moment their card accepts — the old
/// test blocked on "#1995 is in Done". Calling the cards CLI here is
/// slow but deterministic and never ages out.
fn first_wip_card_id() -> Option<String> {
    let output = Command::new("bash")
        .args([&format!("{}/platform/scripts/cards", chorus_root()), "list", "--status", "WIP"])
        .env("HOME", home())
        .env("PATH", full_path())
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Lines look like "  2149  Clear all errors..." — grab the first numeric token.
    for line in stdout.lines() {
        let trimmed = line.trim_start();
        if let Some(first_tok) = trimmed.split_whitespace().next() {
            if first_tok.chars().all(|c| c.is_ascii_digit()) && first_tok.len() >= 3 {
                return Some(first_tok.to_string());
            }
        }
    }
    None
}

#[test]
fn preflight_fails_without_path() {
    let wip = match first_wip_card_id() {
        Some(id) => id,
        None => {
            eprintln!("SKIP: no WIP card available — cannot run preflight against a moving target");
            return;
        }
    };
    let script = format!("{}/skills/demo/gates/preflight.sh", chorus_root());
    let output = Command::new("bash")
        .args([&script, &wip])
        .env("CHORUS_ROOT", chorus_root())
        .env_remove("PATH")
        .output()
        .expect("failed to run preflight.sh");

    // Without PATH, this should fail — proving the bug
    assert!(
        !output.status.success(),
        "preflight.sh should fail without PATH — this proves the bug. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn preflight_passes_with_path() {
    let wip = match first_wip_card_id() {
        Some(id) => id,
        None => {
            eprintln!("SKIP: no WIP card available — cannot run preflight against a moving target");
            return;
        }
    };
    let script = format!("{}/skills/demo/gates/preflight.sh", chorus_root());
    let output = Command::new("bash")
        .args([&script, &wip])
        .env("CHORUS_ROOT", chorus_root())
        .env("HOME", home())
        .env("PATH", full_path())
        .output()
        .expect("failed to run preflight.sh");

    assert!(
        output.status.success(),
        "preflight.sh should pass with PATH set for WIP card #{}. stderr: {}",
        wip,
        String::from_utf8_lossy(&output.stderr)
    );
}
