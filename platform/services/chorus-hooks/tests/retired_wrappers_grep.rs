//! #2311 rescope AC#6/#7 — Wrapper retirement grep.
//!
//! The shell wrappers `session-start.sh` and `chorus-prompt.sh` are
//! retired in favor of the single Rust entry points
//! `chorus-hook-shim session-start <role>` and `chorus-hook-shim
//! session-close <role>`. This test enforces the retirement by walking
//! active production code, scripts, CLAUDE.md fragments, shared skills,
//! and top-level architecture docs for any remaining reference.
//!
//! Note: `werk-init.sh` is NOT retired (Jeff 2026-04-28). Only the
//! `--session` interactive mode was deprecated in #2311; `--scan` and
//! `--close` modes remain load-bearing (per-turn nudge drain, close-out
//! introspection, test-staleness-detection.sh fixture). The earlier
//! grep-and-file assertions for werk-init.sh were removed under #2540
//! / #2556.
//!
//! Exemptions (historical / meta):
//! - Commit history + journal logs (not scanned here — we scan files)
//! - `activity.md`, `roles/**/stories.md`, `roles/**/decisions.md`,
//!   `roles/wren/chorus-consolidation-proposal.md` — historical role
//!   prose referencing prior architecture
//! - `**/briefs-archive/**`, `**/briefs/archive/**`, `**/proving/workflows/archive/**`
//! - Per-role architecture docs at `roles/<role>/docs/` and
//!   `roles/<role>/spine-*.md`, `roles/<role>/chorus-method-map.md` —
//!   role-private snapshots that role owners update on their own cadence
//! - `**/logs/**` and board-snapshot JSONs — runtime data
//! - The binary-gate test file itself (`session_init_gate_binary.rs`) —
//!   deliberately probes these exact names to prove the gate denies them

use std::path::Path;
use std::process::Command;

const REPO_ROOT: &str = "/Users/jeffbridwell/CascadeProjects/chorus";

fn repo_scan(pattern: &str) -> Vec<String> {
    let out = Command::new("grep")
        .args([
            "-rn",
            pattern,
            "--include=*.md",
            "--include=*.rs",
            "--include=*.ts",
            "--include=*.sh",
            "--include=*.conf",
            "--include=*.json",
            "--exclude-dir=.git",
            "--exclude-dir=briefs-archive",
            "--exclude-dir=archive",
            "--exclude-dir=logs",
            "--exclude-dir=target",
            "--exclude-dir=node_modules",
            "--exclude=activity.md",
            "--exclude=stories.md",
            "--exclude=decisions.md",
            "--exclude=chorus-consolidation-proposal.md",
            "--exclude=chorus-method-map.md",
            "--exclude=spine-architecture.md",
            "--exclude=spine-emitter-inventory.md",
            "--exclude=CONCEPTUAL_ARCHITECTURE.md",
            "--exclude=session_init_gate_binary.rs",
            "--exclude=retired_wrappers_grep.rs",
            "--exclude=2311-*.md",
            "--exclude=board-snapshot-*.json",
            "--exclude=pair-*.md",
            // #2311 AC#7 exemptions — historical records, not active callers:
            "--exclude=metrics-manifest.json",    // "resolved" entries describe past work
            "--exclude=roadmap-mapping.json",      // historical roadmap reference
            "--exclude=brief-pipeline-flow.test.ts", // test comment only (blocked by tsc gate)
            REPO_ROOT,
        ])
        .output()
        .expect("grep should run");

    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.contains("proving/workflows/archive"))
        .filter(|l| !l.contains("briefs/archive"))
        .filter(|l| !l.contains("products/logs"))
        .map(|s| s.to_string())
        .collect()
}

fn assert_retired(name: &str) {
    let hits = repo_scan(name);
    assert!(
        hits.is_empty(),
        "retirement grep for '{}' must be zero in active code/docs \
         (historical prose exempt). Hits:\n{}",
        name,
        hits.join("\n")
    );
}

#[test]
fn session_start_sh_has_zero_active_hits() {
    assert_retired("session-start\\.sh");
}

#[test]
fn chorus_prompt_sh_has_zero_active_hits() {
    assert_retired("chorus-prompt\\.sh");
}

#[test]
fn session_start_script_file_does_not_exist() {
    let candidates = [
        "platform/scripts/session-start.sh",
        "messages/scripts/session-start.sh",
    ];
    for c in candidates {
        let p = format!("{}/{}", REPO_ROOT, c);
        assert!(
            !Path::new(&p).exists(),
            "session-start.sh must be retired (#2311 AC#6): {}",
            p
        );
    }
}

#[test]
fn chorus_prompt_script_file_does_not_exist() {
    let candidates = [
        "platform/scripts/chorus-prompt.sh",
        "messages/scripts/chorus-prompt.sh",
    ];
    for c in candidates {
        let p = format!("{}/{}", REPO_ROOT, c);
        assert!(
            !Path::new(&p).exists(),
            "chorus-prompt.sh must be retired (#2311 AC#6): {}",
            p
        );
    }
}
