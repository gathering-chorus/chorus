//! Tests for #2113 — brief pending scanner reads filesystem, not handoffs.log.
//!
//! AC:
//! - Scanner reports 0 pending when briefs/ has no non-card files
//! - Scanner reports N pending when N non-card briefs exist in briefs/
//! - Moving a brief to briefs/archive/ drops it from pending on next scan
//! - Card-movement and card-done files are filtered as noise
//!
//! Covers: empty dir, pending-only, archived-only, mixed.
//!
//! NOTE: chorus-hooks is currently a binary crate (no lib.rs). #2115 converts
//! to lib+bin so tests can import directly. Until then, is_real_brief and
//! scan_briefs_pending are duplicated here.
//!
//! KEEP IN SYNC WITH src/commands/context_cache.rs::{is_real_brief, scan_briefs_pending}

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::tempdir;

fn touch(dir: &Path, name: &str) {
    fs::write(dir.join(name), "content").unwrap();
}

// -- replicas of production fns ---------------------------------------------

fn strip_date_prefix(name: &str) -> &str {
    let b = name.as_bytes();
    if b.len() >= 11
        && b[0].is_ascii_digit() && b[1].is_ascii_digit() && b[2].is_ascii_digit() && b[3].is_ascii_digit()
        && b[4] == b'-'
        && b[5].is_ascii_digit() && b[6].is_ascii_digit()
        && b[7] == b'-'
        && b[8].is_ascii_digit() && b[9].is_ascii_digit()
        && b[10] == b'-'
    {
        &name[11..]
    } else {
        name
    }
}

fn is_real_brief(name: &str) -> bool {
    if !name.ends_with(".md") { return false; }
    let body = strip_date_prefix(name);
    if let Some(rest) = body.strip_prefix("card-") {
        if rest.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            return false;
        }
    }
    true
}

fn scan_briefs_pending(briefs_dir: &Path) -> String {
    let entries = match fs::read_dir(briefs_dir) {
        Ok(e) => e,
        Err(_) => return String::new(),
    };
    let now = SystemTime::now();
    let mut items: Vec<(String, u64)> = Vec::new();
    for entry in entries.flatten() {
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if !file_type.is_file() { continue; }
        let name = entry.file_name().to_string_lossy().to_string();
        if !is_real_brief(&name) { continue; }
        let mtime = entry.metadata().ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(UNIX_EPOCH);
        let age_hours = now.duration_since(mtime)
            .map(|d| d.as_secs() / 3600)
            .unwrap_or(0);
        items.push((name, age_hours));
    }
    if items.is_empty() { return String::new(); }
    items.sort_by_key(|(_, age)| *age);
    let mut output: Vec<String> = items.iter()
        .take(10)
        .map(|(name, age)| format!("- {} ({}h)", name, age))
        .collect();
    output.push(format!("SUMMARY:{} pending", items.len()));
    output.join("\n")
}

// -- tests ------------------------------------------------------------------

#[test]
fn empty_dir_returns_empty_string() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("archive")).unwrap();
    let out = scan_briefs_pending(dir.path());
    assert_eq!(out, "", "empty briefs/ should yield no output");
}

#[test]
fn card_movement_files_are_noise() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("archive")).unwrap();
    touch(dir.path(), "2026-04-11-card-1814-moved-to-Done.md");
    touch(dir.path(), "2026-04-11-card-1832-done.md");
    touch(dir.path(), "2026-04-11-card-1860-moved-to-WIP.md");
    touch(dir.path(), "card-999-done.md");
    let out = scan_briefs_pending(dir.path());
    assert_eq!(out, "", "card movement/done files must not count as pending");
}

#[test]
fn real_briefs_count_as_pending() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("archive")).unwrap();
    touch(dir.path(), "2026-04-12-reindex-gap.md");
    touch(dir.path(), "namespace-move-silas.md");
    touch(dir.path(), "spike-jena-text-index.md");
    let out = scan_briefs_pending(dir.path());
    assert!(out.contains("SUMMARY:3 pending"), "expected 3 pending, got: {}", out);
}

#[test]
fn archived_briefs_do_not_count() {
    let dir = tempdir().unwrap();
    let archive = dir.path().join("archive");
    fs::create_dir_all(&archive).unwrap();
    fs::write(archive.join("namespace-move-silas.md"), "x").unwrap();
    fs::write(archive.join("2026-04-10-git-queue-dirty-tree.md"), "x").unwrap();
    let out = scan_briefs_pending(dir.path());
    assert_eq!(out, "", "briefs in archive/ must not appear as pending");
}

#[test]
fn mixed_pending_and_archived() {
    let dir = tempdir().unwrap();
    let archive = dir.path().join("archive");
    fs::create_dir_all(&archive).unwrap();
    touch(dir.path(), "2026-04-15-live-brief.md");
    touch(dir.path(), "2026-04-11-card-1814-moved-to-Done.md");
    fs::write(archive.join("namespace-move-silas.md"), "x").unwrap();
    let out = scan_briefs_pending(dir.path());
    assert!(out.contains("SUMMARY:1 pending"), "expected 1 pending, got: {}", out);
    assert!(out.contains("2026-04-15-live-brief.md"), "expected live brief listed: {}", out);
    assert!(!out.contains("namespace-move-silas.md"), "archived brief must not appear: {}", out);
}

#[test]
fn moving_to_archive_drops_from_pending() {
    let dir = tempdir().unwrap();
    let archive = dir.path().join("archive");
    fs::create_dir_all(&archive).unwrap();
    touch(dir.path(), "real-brief.md");
    let before = scan_briefs_pending(dir.path());
    assert!(before.contains("SUMMARY:1 pending"), "expected 1 pending before, got: {}", before);

    fs::rename(dir.path().join("real-brief.md"), archive.join("real-brief.md")).unwrap();

    let after = scan_briefs_pending(dir.path());
    assert_eq!(after, "", "after archive move, pending must be 0; got: {}", after);
}

#[test]
fn is_real_brief_classifier() {
    // Noise — card movement/done notifications
    assert!(!is_real_brief("2026-04-11-card-1814-moved-to-Done.md"));
    assert!(!is_real_brief("2026-04-11-card-1832-done.md"));
    assert!(!is_real_brief("2026-04-11-card-1860-moved-to-WIP.md"));
    assert!(!is_real_brief("card-999-done.md"));
    // Noise — non-md
    assert!(!is_real_brief("chorus-activity-dashboard.html"));
    assert!(!is_real_brief("screenshot.png"));
    assert!(!is_real_brief("notes.txt"));
    // Noise — workflow step marker (.md.done suffix does not end in .md)
    assert!(!is_real_brief("2026-02-21-wf-002-step2.md.done"));
    // Real
    assert!(is_real_brief("namespace-move-silas.md"));
    assert!(is_real_brief("2026-04-12-reindex-gap.md"));
    assert!(is_real_brief("spike-jena-text-index.md"));
    assert!(is_real_brief("demo-1807.md"));
}

#[test]
fn non_md_files_are_ignored() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("archive")).unwrap();
    touch(dir.path(), "dashboard.html");
    touch(dir.path(), "screenshot.png");
    touch(dir.path(), "notes.txt");
    let out = scan_briefs_pending(dir.path());
    assert_eq!(out, "", "non-md files must be ignored");
}
