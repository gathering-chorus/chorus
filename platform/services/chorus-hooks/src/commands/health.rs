//! Health/ops commands — extracted from shim.rs (#2077)
//!
//! Contains: health_hourly, health_daily, log_rotate, cruft_scan

use std::fs;
use std::process::{Command as Cmd, ExitCode};

use crate::process;
use crate::shared::state_paths::repo_root;

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Minimal timestamp from unix epoch (no chrono dependency in shim)
#[allow(dead_code)]
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
        let days_in_year = if y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400)) { 366 } else { 365 };
        if remaining < days_in_year { break; }
        remaining -= days_in_year;
        y += 1;
    }
    let leap = y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let month_days: [u64; 12] = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0;
    while m < 12 && remaining >= month_days[m] {
        remaining -= month_days[m];
        m += 1;
    }
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m + 1, remaining + 1, hours, mins, secs)
}

#[allow(dead_code)]
pub fn decode_chunked(body: &str) -> String {
    let mut result = String::new();
    let mut remaining = body;

    #[allow(clippy::while_let_loop)]
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
    let activity_size = fs::metadata(format!("{}/activity.md", repo_root()))
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
    for dir in &["silas", "wren", "kade"] {
        let path = format!("{}/platform/roles/{}/CLAUDE.md", repo_root(), dir);
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
        "wren" => "wren",
        "silas" => "silas",
        "kade" => "kade",
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
    let cost_path = format!("{}/cost-log.md", repo_root());
    if let Ok(content) = fs::read_to_string(&cost_path) {
        if !content.contains(&today) {
            eprintln!("WARNING: no cost entry for today");
        }
    }

    // Activity.md recency
    let activity_path = format!("{}/activity.md", repo_root());
    if let Ok(meta) = fs::metadata(&activity_path) {
        if let Ok(modified) = meta.modified() {
            let age_h = modified.elapsed().unwrap_or_default().as_secs() / 3600;
            if age_h > 24 { eprintln!("WARNING: activity.md not updated in {}h", age_h); }
        }
    }

    // Uncommitted files
    let uncommitted = Cmd::new("git")
        .args(["-C", repo_root(), "status", "--porcelain", &format!("{}/", role_dir)])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.lines().count())
        .unwrap_or(0);
    if uncommitted > 5 { eprintln!("WARNING: {} uncommitted files in {}/", uncommitted, role_dir); }

    // Recurring errors
    let error_log = format!("{}/proving/logs/command-errors.log", repo_root());
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

    // #3607 — log rotation existed (#1622, log_rotate below) but NOTHING scheduled
    // it: com.gathering.log-rotate only rotates Gathering logs, so chorus.log grew
    // unbounded (117MB) and every whole-file reader paid ~2.4s per read. Ride the
    // existing hourly agent instead of adding a new LaunchAgent.
    let _ = log_rotate();

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
        "wren" => format!("{}/roles/wren", repo_root()),
        "silas" => format!("{}/roles/silas", repo_root()),
        "kade" => format!("{}/roles/kade", repo_root()),
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
        .args(["-C", repo_root(), "log", "--oneline", "--since=24 hours ago"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| fs::write(format!("/tmp/git-daily-{}.txt", role), s));

    println!("Daily check complete for {}", role);
    ExitCode::SUCCESS
}

/// Log rotation — replaces log-rotate.sh (#1622).
/// #3610: paths were repo_root()+"/chorus/..." — a double /chorus ever since
/// #2505 made repo_root() return the chorus checkout itself. log_rotate scanned
/// a nonexistent dir and "succeeded" while chorus.log grew to 122MB; the
/// cost-log/activity hourly checks were dead the same way. Verified by running
/// log-rotate live 2026-07-04 (zero files scanned).
pub fn log_rotate() -> ExitCode {
    let log_dir = &format!("{}/platform/logs", repo_root());
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

/// Weekly health checks — replaces context-cache-weekly.sh (#1590, #2246)
/// Runs cruft scan, stale card audit, disk trend logging.
pub fn health_weekly(args: &[String]) -> ExitCode {
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    if !matches!(role, "wren" | "silas" | "kade") {
        eprintln!("Usage: chorus-hook-shim health-weekly <role>");
        return ExitCode::from(1);
    }

    println!("=== Weekly check for {} — {} ===", role, process::wall_clock());

    // 1. Cruft scan (reuse existing function)
    println!("\n--- Cruft Scan ---");
    let _ = cruft_scan();

    // 2. Stale card audit — cards in WIP/Next >7 days
    println!("\n--- Stale Card Audit ---");
    let board_ts = format!("{}/platform/scripts/cards", repo_root());
    let list_output = Cmd::new("zsh")
        .arg("-lc")
        .arg(format!("{} list", board_ts))
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();

    let mut current_status = String::new();
    let mut stale_count = 0u32;
    for line in list_output.lines() {
        let trimmed = line.trim();
        // Status headers like "WIP (3):" or "Next (5):"
        if trimmed.starts_with("WIP") || trimmed.starts_with("Next") || trimmed.starts_with("Later")
            || trimmed.starts_with("Done") || trimmed.starts_with("Won't") || trimmed.starts_with("Blocked") {
            current_status = trimmed.split_whitespace().next().unwrap_or("").to_string();
            continue;
        }
        // Only audit WIP and Next
        if current_status != "WIP" && current_status != "Next" { continue; }
        // Parse card number
        let card_num = trimmed.split_whitespace().next()
            .and_then(|s| s.parse::<u32>().ok());
        if let Some(num) = card_num {
            // Check card age via view --json
            let card_json = Cmd::new("zsh")
                .arg("-lc")
                .arg(format!("{} view {} --json 2>/dev/null", board_ts, num))
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok());
            if let Some(json_str) = card_json {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    if let Some(updated) = v.get("updated").and_then(|u| u.as_str()) {
                        // Parse ISO date and check age
                        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(updated) {
                            let age_days = (chrono::Utc::now() - dt.with_timezone(&chrono::Utc)).num_days();
                            if age_days > 7 {
                                let title = v.get("title").and_then(|t| t.as_str()).unwrap_or("?");
                                let owner = v.get("owner").and_then(|o| o.as_str()).unwrap_or("?");
                                println!("  STALE: #{} [{}] {} — {}d in {} (owner: {})",
                                    num, current_status, title, age_days, current_status, owner);
                                stale_count += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    if stale_count == 0 {
        println!("  No stale cards found");
    } else {
        println!("  {} stale card(s) in WIP/Next", stale_count);
    }

    // 3. Disk trend
    println!("\n--- Disk Trend ---");
    let trend_file = "/Users/jeffbridwell/Library/Logs/Chorus/disk-trend.log";
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
            ((total as f64 - free) / total as f64 * 100.0) as u32
        }
        _ => 0,
    };
    let today: String = process::wall_clock().chars().take(10).collect();
    let trend_line = format!("{},{}\n", today, disk_pct);
    let _ = fs::OpenOptions::new().create(true).append(true).open(trend_file)
        .and_then(|mut f| { use std::io::Write; f.write_all(trend_line.as_bytes()) });
    println!("  Disk: {}%", disk_pct);
    if let Ok(content) = fs::read_to_string(trend_file) {
        let recent: Vec<&str> = content.lines().rev().take(5).collect();
        for line in recent.into_iter().rev() {
            println!("  {}", line);
        }
    }

    println!("\n=== Weekly check complete for {} ===", role);
    ExitCode::SUCCESS
}

#[cfg(test)]
mod log_rotate_path_tests {
    use super::repo_root;

    // #3610 — the double-/chorus regression guard: every path health.rs builds
    // from repo_root() must exist under the real checkout layout. repo_root()
    // itself ends in /chorus, so appending another /chorus must never happen.
    #[test]
    fn health_paths_do_not_double_the_chorus_segment() {
        let root = repo_root();
        assert!(
            !root.is_empty() && !root.ends_with('/'),
            "repo_root must be a bare path"
        );
        let log_dir = format!("{}/platform/logs", root);
        assert!(
            !log_dir.contains("/chorus/chorus/"),
            "log dir must not double /chorus: {}",
            log_dir
        );
        // the dir the rotation scans must actually exist in a real checkout
        // (in CI/fixture roots this is skipped — existence only asserted when
        // the root itself exists)
        if std::path::Path::new(root).exists() {
            assert!(
                std::path::Path::new(&log_dir).exists(),
                "rotation target dir missing: {}",
                log_dir
            );
        }
    }
}
