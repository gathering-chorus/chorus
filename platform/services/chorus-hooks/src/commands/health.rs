//! Health/ops commands — extracted from shim.rs (#2077)
//!
//! Contains: health_hourly, health_daily, log_rotate, cruft_scan

use std::fs;
use std::process::{Command as Cmd, ExitCode};

use crate::process;
use crate::shared::state_paths::{self, REPO_ROOT};

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Minimal timestamp from unix epoch (no chrono dependency in shim)
pub fn chrono_lite_ts(epoch_secs: u64) -> String {
    // Approximate ISO timestamp — good enough for comparison
    let days = epoch_secs / 86400;
    let secs_in_day = epoch_secs % 86400;
    let hours = secs_in_day / 3600;
    let mins = (secs_in_day % 3600) / 60;
    let secs = secs_in_day % 60;

    // Days since epoch to Y-M-D (simplified, good enough for 2026)
    let mut y = 1970u64;
    let mut remaining = days;
    loop {
        let days_in_year = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 366 } else { 365 };
        if remaining < days_in_year { break; }
        remaining -= days_in_year;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let month_days: [u64; 12] = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0;
    while m < 12 && remaining >= month_days[m] {
        remaining -= month_days[m];
        m += 1;
    }
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m + 1, remaining + 1, hours, mins, secs)
}

pub fn decode_chunked(body: &str) -> String {
    let mut result = String::new();
    let mut remaining = body;

    loop {
        // Find chunk size line
        let size_end = match remaining.find("\r\n") {
            Some(pos) => pos,
            None => break,
        };
        let size_str = remaining[..size_end].trim();
        let size = match usize::from_str_radix(size_str, 16) {
            Ok(s) => s,
            Err(_) => break,
        };
        if size == 0 {
            break;
        }
        let chunk_start = size_end + 2;
        let chunk_end = chunk_start + size;
        if chunk_end > remaining.len() {
            break;
        }
        result.push_str(&remaining[chunk_start..chunk_end]);
        remaining = &remaining[chunk_end..];
        if remaining.starts_with("\r\n") {
            remaining = &remaining[2..];
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Public commands
// ---------------------------------------------------------------------------

/// Cruft scan — replaces cruft-scan.sh (#1622)
pub fn cruft_scan() -> ExitCode {
    let out_path = "/tmp/cruft-scan-latest.md";
    let clock = process::wall_clock();
    let clock_short: String = clock.chars().take(16).collect();

    let mut out = String::with_capacity(4096);
    out.push_str(&format!("# Cruft Scan — {} Boston\n\n", clock_short));

    // Activity log size
    out.push_str("## Activity Log\n");
    let activity_size = fs::metadata(&format!("{}/chorus/activity.md", REPO_ROOT))
        .map(|m| m.len()).unwrap_or(0);
    out.push_str(&format!("Size: {} bytes\n\n", activity_size));

    // Disk check — APFS-aware via Finder free space (includes purgeable, matches Finder)
    out.push_str("## Disk\n");
    let finder_free = Cmd::new("osascript")
        .args(["-e", "tell application \"Finder\" to get free space of startup disk"])
        .output().ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<f64>().ok());
    let container_total = Cmd::new("diskutil").args(["info", "/"])
        .output().ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|text| {
            text.lines()
                .find(|l| l.contains("Container Total Space"))
                .and_then(|l| l.split('(').nth(1))
                .and_then(|s| s.split_whitespace().next())
                .and_then(|n| n.parse::<u64>().ok())
        });
    let disk_pct = match (container_total, finder_free) {
        (Some(total), Some(free)) if total > 0 => {
            format!("{}%", ((total as f64 - free) / total as f64 * 100.0) as u64)
        }
        _ => "unknown".to_string(),
    };
    out.push_str(&format!("Library: {}\n", disk_pct));
    if disk_pct.starts_with("9") { out.push_str("WARNING: disk above 90%\n"); }
    out.push('\n');

    // Session-start file sizes
    out.push_str("## Session Start Files\n");
    for role in &["silas", "wren", "kade"] {
        let path = format!("/tmp/session-start-{}.md", role);
        if let Ok(meta) = fs::metadata(&path) {
            let size = meta.len();
            let lines = fs::read_to_string(&path).map(|c| c.lines().count()).unwrap_or(0);
            let age_h = meta.modified().ok()
                .map(|t| t.elapsed().unwrap_or_default().as_secs() / 3600).unwrap_or(0);
            let mut flag = String::new();
            if size > 50000 { flag.push_str(" ⚠ BLOATED"); }
            if age_h > 24 { flag.push_str(&format!(" ⚠ STALE ({}h)", age_h)); }
            out.push_str(&format!("  {}: {} bytes, {} lines{}\n", role, size, lines, flag));
        } else {
            out.push_str(&format!("  {}: not found\n", role));
        }
    }
    out.push('\n');

    // Memory file sizes
    out.push_str("## Memory Files (>10KB)\n");
    let mem_dir = "/Users/jeffbridwell/.claude/projects/-Users-jeffbridwell-CascadeProjects/memory";
    if let Ok(entries) = fs::read_dir(mem_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().map(|e| e == "md").unwrap_or(false) {
                if let Ok(meta) = entry.metadata() {
                    if meta.len() > 10240 {
                        out.push_str(&format!("  {}: {} bytes\n",
                            entry.file_name().to_string_lossy(), meta.len()));
                    }
                }
            }
        }
    }
    out.push('\n');

    // CLAUDE.md sizes
    out.push_str("## CLAUDE.md Sizes\n");
    for dir in &["architect", "product-manager", "engineer"] {
        let path = format!("{}/{}/CLAUDE.md", REPO_ROOT, dir);
        if let Ok(meta) = fs::metadata(&path) {
            let flag = if meta.len() > 30000 { " ⚠ HEAVY" } else { "" };
            out.push_str(&format!("  {}: {} bytes{}\n", dir, meta.len(), flag));
        }
    }
    out.push('\n');

    // Disk trend
    out.push_str("## Disk Trend\n");
    let trend_file = "/Users/jeffbridwell/Library/Logs/Chorus/disk-trend.log";
    let disk_num = disk_pct.trim_end_matches('%');
    let today: String = clock.chars().take(10).collect();
    let trend_line = format!("{},{}\n", today, disk_num);
    let _ = fs::OpenOptions::new().create(true).append(true).open(trend_file)
        .and_then(|mut f| { use std::io::Write; f.write_all(trend_line.as_bytes()) });
    if let Ok(content) = fs::read_to_string(trend_file) {
        let recent: Vec<&str> = content.lines().rev().take(5).collect();
        out.push_str(&format!("Recent entries: {}\n", recent.into_iter().rev().collect::<Vec<_>>().join(" ")));
    }
    out.push('\n');

    out.push_str("---\n");
    // Next scan date — 3 days from now
    out.push_str("Next scan: +3 days\n");

    let _ = fs::write(out_path, &out);
    eprintln!("Cruft scan complete → {}", out_path);
    print!("{}", out);
    ExitCode::SUCCESS
}

/// Hourly health checks — replaces context-cache-hourly.sh (#1622)
pub fn health_hourly(args: &[String]) -> ExitCode {
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    if !matches!(role, "wren" | "silas" | "kade") {
        eprintln!("Usage: chorus-hook-shim health-hourly <role>");
        return ExitCode::from(1);
    }

    let role_dir = match role {
        "wren" => "product-manager",
        "silas" => "architect",
        "kade" => "engineer",
        _ => unreachable!(),
    };

    // Disk check — APFS-aware via Finder free space (includes purgeable, matches Finder)
    let disk_pct: u32 = {
        let finder_free = Cmd::new("osascript")
            .args(["-e", "tell application \"Finder\" to get free space of startup disk"])
            .output().ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse::<f64>().ok());
        let container_total = Cmd::new("diskutil").args(["info", "/"])
            .output().ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|text| {
                text.lines()
                    .find(|l| l.contains("Container Total Space"))
                    .and_then(|l| l.split('(').nth(1))
                    .and_then(|s| s.split_whitespace().next())
                    .and_then(|n| n.parse::<u64>().ok())
            });
        match (container_total, finder_free) {
            (Some(total), Some(free)) if total > 0 => {
                ((total as f64 - free) / total as f64 * 100.0) as u32
            }
            _ => 0,
        }
    };
    if disk_pct > 95 { eprintln!("CRITICAL: disk at {}%", disk_pct); }
    else if disk_pct > 90 { eprintln!("WARNING: disk at {}%", disk_pct); }

    // Cost log check
    let today = process::wall_clock().chars().take(10).collect::<String>();
    let cost_path = format!("{}/chorus/cost-log.md", REPO_ROOT);
    if let Ok(content) = fs::read_to_string(&cost_path) {
        if !content.contains(&today) {
            eprintln!("WARNING: no cost entry for today");
        }
    }

    // Activity.md recency
    let activity_path = format!("{}/chorus/activity.md", REPO_ROOT);
    if let Ok(meta) = fs::metadata(&activity_path) {
        if let Ok(modified) = meta.modified() {
            let age_h = modified.elapsed().unwrap_or_default().as_secs() / 3600;
            if age_h > 24 { eprintln!("WARNING: activity.md not updated in {}h", age_h); }
        }
    }

    // Uncommitted files
    let uncommitted = Cmd::new("git")
        .args(["-C", REPO_ROOT, "status", "--porcelain", &format!("{}/", role_dir)])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.lines().count())
        .unwrap_or(0);
    if uncommitted > 5 { eprintln!("WARNING: {} uncommitted files in {}/", uncommitted, role_dir); }

    // Recurring errors
    let error_log = format!("{}/chorus/proving/logs/command-errors.log", REPO_ROOT);
    if let Ok(content) = fs::read_to_string(&error_log) {
        let mut fps: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for line in content.lines() {
            if !line.contains(&format!("\"date\":\"{}\"", today)) { continue; }
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(fp) = v.get("fingerprint").and_then(|f| f.as_str()) {
                    *fps.entry(fp.to_string()).or_insert(0) += 1;
                }
            }
        }
        for (fp, count) in &fps {
            if *count >= 3 { eprintln!("WARNING: error {} repeated {}x today", fp, count); }
        }
    }

    println!("Hourly check complete: disk={}% uncommitted={}", disk_pct, uncommitted);
    ExitCode::SUCCESS
}

/// Daily health checks — replaces context-cache-daily.sh (#1622)
pub fn health_daily(args: &[String]) -> ExitCode {
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    if !matches!(role, "wren" | "silas" | "kade") {
        eprintln!("Usage: chorus-hook-shim health-daily <role>");
        return ExitCode::from(1);
    }

    let role_dir_path = match role {
        "wren" => format!("{}/product-manager", REPO_ROOT),
        "silas" => format!("{}/architect", REPO_ROOT),
        "kade" => format!("{}/engineer", REPO_ROOT),
        _ => unreachable!(),
    };

    // Doc freshness (4h window)
    let session_window = 14400u64;
    let check_fresh = |path: &str, label: &str| {
        if let Ok(meta) = fs::metadata(path) {
            if let Ok(modified) = meta.modified() {
                let age = modified.elapsed().unwrap_or_default().as_secs();
                if age > session_window {
                    eprintln!("WARNING: {} stale ({}h)", label, age / 3600);
                }
            }
        }
    };
    match role {
        "silas" => {
            check_fresh(&format!("{}/system-architecture.md", role_dir_path), "system-architecture.md");
            check_fresh(&format!("{}/ontology-status.md", role_dir_path), "ontology-status.md");
        }
        "wren" => {
            check_fresh(&format!("{}/decisions.md", role_dir_path), "decisions.md");
            check_fresh(&format!("{}/backlog.md", role_dir_path), "backlog.md");
        }
        "kade" => {
            check_fresh(&format!("{}/current-work.md", role_dir_path), "current-work.md");
            check_fresh(&format!("{}/tech-debt.md", role_dir_path), "tech-debt.md");
        }
        _ => {}
    }

    // CLAUDE.md staleness
    let claude_path = format!("{}/CLAUDE.md", role_dir_path);
    if let Ok(meta) = fs::metadata(&claude_path) {
        if let Ok(modified) = meta.modified() {
            let age_days = modified.elapsed().unwrap_or_default().as_secs() / 86400;
            if age_days > 3 { eprintln!("WARNING: CLAUDE.md stale ({} days)", age_days); }
        }
    }

    // Stale briefs
    let briefs_dir = format!("{}/briefs", role_dir_path);
    if let Ok(entries) = fs::read_dir(&briefs_dir) {
        let stale_count = entries.flatten().filter(|e| {
            e.path().extension().map(|ext| ext == "md").unwrap_or(false)
                && e.metadata().ok()
                    .and_then(|m| m.modified().ok())
                    .map(|t| t.elapsed().unwrap_or_default().as_secs() > 7 * 86400)
                    .unwrap_or(false)
        }).count();
        if stale_count > 0 { eprintln!("INFO: {} briefs older than 7 days in {} inbox", stale_count, role); }
    }

    // Git log summary
    let _ = Cmd::new("git")
        .args(["-C", REPO_ROOT, "log", "--oneline", "--since=24 hours ago"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| fs::write(format!("/tmp/git-daily-{}.txt", role), s));

    println!("Daily check complete for {}", role);
    ExitCode::SUCCESS
}

/// Log rotation — replaces log-rotate.sh (#1622)
pub fn log_rotate() -> ExitCode {
    let log_dir = &format!("{}/chorus/platform/logs", REPO_ROOT);
    let max_size: u64 = 10 * 1024 * 1024; // 10MB
    let keep_rotations = 3u32;

    let logs = ["chorus.log", "permission-prompts.log", "command-errors.log", "handoffs.log"];

    println!("=== Log rotation {} ===", process::wall_clock().chars().take(16).collect::<String>());

    for name in &logs {
        let path = format!("{}/{}", log_dir, name);
        let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let size_mb = size / 1_048_576;

        if size < max_size { continue; }

        println!("Rotating {} ({}MB > {}MB threshold)", name, size_mb, max_size / 1_048_576);

        // Shift existing rotations
        for i in (1..keep_rotations).rev() {
            let from = format!("{}.{}.gz", path, i);
            let to = format!("{}.{}.gz", path, i + 1);
            let _ = fs::rename(&from, &to);
        }

        // Compress current as .1.gz
        let gz_path = format!("{}.1.gz", path);
        let _ = Cmd::new("gzip").args(["-c", &path]).output().and_then(|o| {
            fs::write(&gz_path, &o.stdout)
        });

        // Truncate original
        let _ = fs::write(&path, "");
        println!("  → Compressed to {}.1.gz, truncated original", name);

        // Delete old rotations beyond limit
        for i in (keep_rotations + 1)..10 {
            let old = format!("{}.{}.gz", path, i);
            if fs::metadata(&old).is_ok() {
                let _ = fs::remove_file(&old);
                println!("  → Deleted old rotation: {}.{}.gz", name, i);
            }
        }
    }

    // Report sizes
    println!("Current log sizes:");
    if let Ok(entries) = fs::read_dir(log_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().map(|e| e == "log").unwrap_or(false) {
                let size = fs::metadata(entry.path()).map(|m| m.len()).unwrap_or(0);
                println!("  {}: {}KB", entry.file_name().to_string_lossy(), size / 1024);
            }
        }
    }
    println!("=== Done ===");
    ExitCode::SUCCESS
}
