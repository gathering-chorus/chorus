//! Context cache command — extracted from shim.rs
//! Builds /tmp/session-context-<role>.md with board state, briefs, health, memory.

use std::fs;
use std::process::ExitCode;
use std::process::Command as Cmd;

use crate::process;
use crate::shared::state_paths::REPO_ROOT;

/// Builds /tmp/session-context-<role>.md with board state, briefs, health, memory.
pub fn run(args: &[String]) -> ExitCode {
    let role = args.first().map(|s| s.as_str()).unwrap_or("");
    if !matches!(role, "wren" | "silas" | "kade") {
        eprintln!("Usage: chorus-hook-shim context-cache <role>");
        return ExitCode::from(1);
    }

    let role_dir_name = crate::shared::state_paths::role_dir(role).unwrap();
    let role_dir = format!("{}/{}", REPO_ROOT, role_dir_name);
    let board_ts = format!("{}/chorus/platform/scripts/cards", REPO_ROOT);
    let out_path = format!("/tmp/session-context-{}.md", role);

    // Werk version
    let manifest_path = format!("{}/chorus/designing/claudemd/manifest.json", REPO_ROOT);
    let werk_version = fs::read_to_string(&manifest_path).ok()
        .and_then(|c| c.lines().find(|l| l.contains("\"version\"")).map(|l| {
            l.split('"').nth(3).unwrap_or("unknown").to_string()
        }))
        .unwrap_or_else(|| "unknown".to_string());

    // --- Parallel data gathering via threads ---
    let board_ts_c = board_ts.clone();
    let role_s = role.to_string();
    let board_mine = std::thread::spawn(move || -> (bool, String) {
        match Cmd::new("zsh").arg("-lc").arg(format!("{} mine {}", board_ts_c, role_s)).output() {
            Ok(o) if o.status.success() => (true, String::from_utf8(o.stdout).unwrap_or_default()),
            _ => (false, String::new()),
        }
    });

    let board_ts_c = board_ts.clone();
    let board_list = std::thread::spawn(move || -> (bool, String) {
        match Cmd::new("zsh").arg("-lc").arg(format!("{} list", board_ts_c)).output() {
            Ok(o) if o.status.success() => (true, String::from_utf8(o.stdout).unwrap_or_default()),
            _ => (false, String::new()),
        }
    });

    let board_ts_c = board_ts.clone();
    let role_s = role.to_string();
    let board_audit = std::thread::spawn(move || {
        Cmd::new("zsh").arg("-lc").arg(format!("{} audit-start {}", board_ts_c, role_s))
            .output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default()
    });

    // Recent decisions
    let decisions_path = format!("{}/roles/wren/decisions.md", REPO_ROOT);
    let decisions = fs::read_to_string(&decisions_path).ok()
        .map(|c| c.lines().filter(|l| l.starts_with("## DEC-"))
            .collect::<Vec<_>>().into_iter().rev().take(10).collect::<Vec<_>>()
            .into_iter().rev().collect::<Vec<_>>().join("\n"))
        .unwrap_or_default();

    // Recent briefs
    let briefs_dir = format!("{}/briefs", role_dir);
    let briefs = if let Ok(mut entries) = fs::read_dir(&briefs_dir) {
        let mut files: Vec<_> = entries.by_ref()
            .flatten()
            .filter(|e| e.path().extension().map(|x| x == "md").unwrap_or(false))
            .collect();
        files.sort_by(|a, b| b.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            .cmp(&a.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH)));
        files.iter().take(5)
            .map(|e| format!("- {}", e.file_name().to_string_lossy()))
            .collect::<Vec<_>>().join("\n")
    } else { String::new() };

    // Handoff check
    let handoff_log = format!("{}/chorus/proving/logs/handoffs.log", REPO_ROOT);
    let role_s = role.to_string();
    let briefs_dir_c = briefs_dir.clone();
    let archive_dir = format!("{}/chorus/proving/workflows/archive", REPO_ROOT);
    let handoff_check = std::thread::spawn(move || {
        check_handoffs(&handoff_log, &role_s, &briefs_dir_c, &archive_dir)
    });

    // Memory context from Chorus API
    let board_ts_c = board_ts.clone();
    let role_s = role.to_string();
    let memory_ctx = std::thread::spawn(move || {
        fetch_memory_context(&board_ts_c, &role_s)
    });

    // Wait for threads — track failures for spine event
    let mut failed_sources: Vec<&str> = Vec::new();
    let mut ok_sources: Vec<&str> = Vec::new();

    let (mine_ok, mine_text) = board_mine.join().unwrap_or((false, String::new()));
    if mine_ok { ok_sources.push("board_mine"); } else { failed_sources.push("board_mine"); }

    let (list_ok, list_text) = board_list.join().unwrap_or((false, String::new()));
    if list_ok { ok_sources.push("board_list"); } else { failed_sources.push("board_list"); }

    let audit_text = board_audit.join().unwrap_or_default();
    if !audit_text.is_empty() { ok_sources.push("board_audit"); } else { failed_sources.push("board_audit"); }

    let handoff_text = handoff_check.join().unwrap_or_default();
    ok_sources.push("handoffs"); // handoffs returning empty is normal (no pending)

    let memory_text = memory_ctx.join().unwrap_or_default();
    if !memory_text.is_empty() { ok_sources.push("memory"); } else { failed_sources.push("memory"); }

    // Health checks — APFS disk via Finder free space (includes purgeable, matches Finder)
    let disk_pct = {
        // osascript returns Finder's free space which includes purgeable — matches what Jeff sees
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
                format!("{}%", ((total as f64 - free) / total as f64 * 100.0) as u64)
            }
            _ => "?%".to_string(),
        }
    };
    let disk_pct = disk_pct.as_str();

    let uncommitted = Cmd::new("git").args(["-C", REPO_ROOT, "status", "--porcelain", &format!("{}/", role_dir_name)])
        .output().ok().and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.lines().count()).unwrap_or(0);

    let activity_path = format!("{}/chorus/activity.md", REPO_ROOT);
    let activity_age = fs::metadata(&activity_path).ok()
        .and_then(|m| m.modified().ok())
        .map(|t| format!("{}h", t.elapsed().unwrap_or_default().as_secs() / 3600))
        .unwrap_or_else(|| "unknown".to_string());

    let claude_path = format!("{}/CLAUDE.md", role_dir);
    let claude_status = fs::metadata(&claude_path).ok()
        .and_then(|m| m.modified().ok())
        .map(|t| {
            let days = t.elapsed().unwrap_or_default().as_secs() / 86400;
            if days > 3 { format!("stale ({}d)", days) } else { "clean".to_string() }
        })
        .unwrap_or_else(|| "missing".to_string());

    // Capitalize role name
    let role_cap = format!("{}{}", &role[..1].to_uppercase(), &role[1..]);
    let clock = process::wall_clock();
    let clock_short: String = clock.chars().take(16).collect();

    // --- Assemble ---
    let mut out = String::with_capacity(8192);
    out.push_str(&format!("# {} — Session Context (Werk v{})\n", role_cap, werk_version));
    out.push_str(&format!("Generated: {} Boston | Werk v{}\n\n", clock_short, werk_version));

    out.push_str("## Your Active Cards\n");
    out.push_str(if !mine_ok { "(board unreachable)" } else if mine_text.is_empty() { "(none)" } else { &mine_text });
    out.push_str("\n\n## Boards\n\n");
    out.push_str(if !list_ok { "(board unreachable)" } else if list_text.is_empty() { "(none)" } else { &list_text });
    out.push_str("\n\n## Board Audit\n\n");
    out.push_str(if audit_text.is_empty() { "(none)" } else { &audit_text });

    out.push_str("\n\n## Workflow Steps Waiting\n");
    out.push_str("(none)\n"); // workflow dispatches to TS engine (#1775)

    out.push_str("\n## Recent Briefs\n");
    out.push_str(if briefs.is_empty() { "(none)" } else { &briefs });

    out.push_str("\n\n## Recent Decisions\n");
    out.push_str(if decisions.is_empty() { "(decisions.md not found)" } else { &decisions });

    out.push_str("\n\n## Health\n\n");
    out.push_str(&format!("- Disk: {}\n", disk_pct));
    out.push_str(&format!("- Uncommitted in {}/: {}\n", role_dir_name, uncommitted));
    out.push_str(&format!("- Activity.md: updated {} ago\n", activity_age));
    out.push_str(&format!("- CLAUDE.md: {}\n", claude_status));

    out.push_str("\n## Memory Context\n");
    if memory_text.is_empty() {
        out.push_str("(none)\n");
    } else {
        let line_count = memory_text.lines().count();
        out.push_str(&format!("Related memories for WIP cards ({} found):\n\n", line_count));
        out.push_str(&memory_text);
    }

    out.push_str("\n## Handoff Check\n");
    out.push_str(if handoff_text.is_empty() { "(clean)" } else { &handoff_text });
    out.push('\n');

    let _ = fs::write(&out_path, &out);
    let lines = out.lines().count();
    println!("Context cached: {} ({} lines)", out_path, lines);

    // Spine events — AC for #1808
    let log_path = format!("{}/chorus/platform/logs/chorus.log", REPO_ROOT);
    let eastern_offset = {
        let out = std::process::Command::new("date").args(["+%z"]).env("TZ", "America/New_York").output();
        out.ok().and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| {
                let s = s.trim();
                if s.len() >= 5 {
                    let sign = if s.starts_with('-') { -1 } else { 1 };
                    let h: i32 = s[1..3].parse().unwrap_or(5);
                    let m: i32 = s[3..5].parse().unwrap_or(0);
                    chrono::FixedOffset::east_opt(sign * (h * 3600 + m * 60))
                } else { None }
            })
            .unwrap_or_else(|| chrono::FixedOffset::west_opt(5 * 3600).unwrap())
    };
    let ts = chrono::Utc::now().with_timezone(&eastern_offset).format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();

    if !failed_sources.is_empty() {
        let event = serde_json::json!({
            "timestamp": ts,
            "level": "warn",
            "appName": "chorus-events",
            "component": "context-cache",
            "event": "session.context.error",
            "role": role,
            "failed_sources": failed_sources.join(","),
            "ok_sources": ok_sources.join(","),
            "lines": lines,
        });
        eprintln!("session.context.error | {} — failed: {}", role, failed_sources.join(", "));
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(&log_path)
            .and_then(|mut f| { use std::io::Write; writeln!(f, "{}", event) });
    }

    // Always emit success with source inventory
    let event = serde_json::json!({
        "timestamp": ts,
        "level": "info",
        "appName": "chorus-events",
        "component": "context-cache",
        "event": "session.context.built",
        "role": role,
        "sources": ok_sources.join(","),
        "failed": failed_sources.join(","),
        "lines": lines,
    });
    let _ = std::fs::OpenOptions::new().create(true).append(true).open(&log_path)
        .and_then(|mut f| { use std::io::Write; writeln!(f, "{}", event) });

    ExitCode::SUCCESS
}

/// Fetch memory context from Chorus API for WIP card domains
fn fetch_memory_context(board_ts: &str, role: &str) -> String {
    // Get WIP card IDs
    let mine_out = Cmd::new("zsh").arg("-lc").arg(format!("{} mine {}", board_ts, role))
        .output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();

    let wip_ids: Vec<String> = mine_out.lines()
        .filter(|l| l.to_lowercase().contains("[wip]"))
        .filter_map(|l| l.split_whitespace().find(|w| w.parse::<u32>().is_ok()).map(|s| s.to_string()))
        .take(3).collect();

    if wip_ids.is_empty() {
        return "No WIP cards — no memory context to load.".to_string();
    }

    // Get domains from WIP cards
    let mut domains = std::collections::HashSet::new();
    for cid in &wip_ids {
        let info = Cmd::new("zsh").arg("-lc").arg(format!("{} view {}", board_ts, cid))
            .output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();
        for part in info.split_whitespace() {
            if part.starts_with("domain:") {
                domains.insert(part.replace("domain:", ""));
            }
        }
    }

    if domains.is_empty() { return String::new(); }

    // Query Chorus API for each domain
    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for domain in &domains {
        let url = format!("http://localhost:3340/api/chorus/search?q={}&limit=5", domain);
        let resp = Cmd::new("curl").args(["-s", "--max-time", "3", &url])
            .output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();

        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&resp) {
            if let Some(arr) = v.get("results").and_then(|r| r.as_array()) {
                for r in arr.iter().take(5) {
                    let src = r.get("source").and_then(|s| s.as_str()).unwrap_or("");
                    let content = r.get("content").and_then(|s| s.as_str()).unwrap_or("");
                    let ts = r.get("timestamp").and_then(|s| s.as_str()).unwrap_or("");
                    let key: String = content.chars().take(60).collect();
                    if seen.contains(&key) { continue; }
                    seen.insert(key);

                    let ts_short: String = ts.chars().take(10).collect();
                    let content_short: String = content.chars().take(120).collect();
                    let content_clean = content_short.replace('\n', " ");

                    match src {
                        "memory" | "state" | "story" | "decision" | "brief" | "adr" | "artifact" | "spine" => {
                            results.push(format!("  - [{}] [{}] {}", ts_short, src, content_clean));
                        }
                        "claude" => {
                            let author = r.get("author").and_then(|a| a.as_str()).unwrap_or("");
                            if author == "assistant" && content.len() > 40
                                && !content.starts_with('<') && !content.starts_with('{')
                                && !content.starts_with('[') && !content.starts_with('/')
                                && !content.starts_with("bash ") && !content.starts_with("curl ")
                            {
                                results.push(format!("  - [{}] [session] {}", ts_short, content_clean));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    results.sort();
    results.dedup();
    results.truncate(10);
    results.join("\n")
}

/// Check handoff staleness from handoffs.log
fn check_handoffs(log_path: &str, role: &str, briefs_dir: &str, archive_dir: &str) -> String {
    let content = match fs::read_to_string(log_path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    let mut received_ids = std::collections::HashSet::new();
    let mut events = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            if v.get("status").and_then(|s| s.as_str()) == Some("received") {
                if let Some(id) = v.get("id").and_then(|i| i.as_str()) {
                    received_ids.insert(id.to_string());
                }
            }
            events.push(v);
        }
    }

    let mut output = Vec::new();
    let mut pending_count = 0u32;
    let mut stale_count = 0u32;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();

    for e in &events {
        if e.get("status").and_then(|s| s.as_str()) != Some("sent") { continue; }
        if e.get("to").and_then(|t| t.as_str()) != Some(role) { continue; }
        let id = match e.get("id").and_then(|i| i.as_str()) { Some(i) => i, None => continue };
        if received_ids.contains(id) { continue; }

        pending_count += 1;

        if let Some(ts_str) = e.get("timestamp").and_then(|t| t.as_str()) {
            // Simple age check — parse ISO timestamp
            if let Ok(dt) = chrono_parse_approx(ts_str) {
                let age_hours = (now.saturating_sub(dt)) / 3600;
                if age_hours > 4 {
                    stale_count += 1;
                    let from = e.get("from").and_then(|f| f.as_str()).unwrap_or("?");
                    let artifact = e.get("artifact").and_then(|a| a.as_str())
                        .map(|p| p.rsplit('/').next().unwrap_or(p)).unwrap_or("?");
                    output.push(format!("STALE ({}h): {} from {} - {}", age_hours,
                        e.get("type").and_then(|t| t.as_str()).unwrap_or("?"), from, artifact));
                }
            }
        }
    }

    // Stale workflow briefs
    let mut stale_brief_count = 0u32;
    if let Ok(entries) = std::fs::read_dir(briefs_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.contains("wf-") && name.contains("step") {
                // Check if workflow is archived
                if let Some(wf_num) = name.split("wf-").nth(1).and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next()) {
                    let archive_path = format!("{}/WF-{}.json", archive_dir, wf_num.trim_start_matches('0'));
                    if std::path::Path::new(&archive_path).exists() {
                        stale_brief_count += 1;
                    }
                }
            }
        }
    }

    if pending_count == 0 && stale_brief_count == 0 {
        return String::new();
    }

    let mut parts = Vec::new();
    if pending_count > 0 { parts.push(format!("{} pending", pending_count)); }
    if stale_count > 0 { parts.push(format!("{} stale handoff(s)", stale_count)); }
    if stale_brief_count > 0 { parts.push(format!("{} stale brief(s)", stale_brief_count)); }
    output.push(format!("SUMMARY:{}", parts.join(", ")));
    output.join("\n")
}

/// Approximate ISO timestamp parse -> unix seconds (no chrono crate)
fn chrono_parse_approx(ts: &str) -> Result<u64, ()> {
    // Parse "2026-03-22T21:29:55Z" or similar
    let clean = ts.replace('Z', "+00:00");
    let parts: Vec<&str> = clean.split('T').collect();
    if parts.len() < 2 { return Err(()); }
    let date_parts: Vec<u32> = parts[0].split('-').filter_map(|s| s.parse().ok()).collect();
    let time_str = parts[1].split('+').next().unwrap_or("00:00:00");
    let time_parts: Vec<u32> = time_str.split(':').filter_map(|s| s.parse().ok()).collect();
    if date_parts.len() < 3 || time_parts.len() < 2 { return Err(()); }

    // Rough unix timestamp calculation (good enough for age comparison)
    let year = date_parts[0];
    let month = date_parts[1];
    let day = date_parts[2];
    let hour = time_parts[0];
    let min = time_parts[1];

    // Days since epoch (rough)
    let mut days: u64 = 0;
    for y in 1970..year { days += if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 366 } else { 365 }; }
    let month_days = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month { days += month_days[m as usize] as u64; }
    if month > 2 && year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) { days += 1; }
    days += (day - 1) as u64;

    Ok(days * 86400 + hour as u64 * 3600 + min as u64 * 60)
}
