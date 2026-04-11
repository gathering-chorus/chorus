//! Pulse service — structured team state JSON on every prompt cycle (#1881)
//! Assembles: role states, spine events, alerts, nudges, health, board, index freshness.
//! Target: <200ms. All file reads, no shell spawning.

use std::fs;
use std::process::ExitCode;

use crate::shared::state_paths::REPO_ROOT;

/// Assemble team state snapshot and write to /tmp/pulse-latest.json
pub fn run(_args: &[String]) -> ExitCode {
    let start = std::time::Instant::now();
    let mut pulse = serde_json::Map::new();

    // Timestamp
    let clock = crate::process::wall_clock();
    let clock_short: String = clock.chars().take(19).collect();
    pulse.insert("timestamp".into(), serde_json::Value::String(clock_short));

    // 1. Role states — read 3 JSON files from /tmp/claude-team-scan/
    let roles = assemble_roles();
    pulse.insert("roles".into(), roles);

    // 2. Spine events — last 60s from chorus.log
    let events = assemble_recent_events();
    pulse.insert("events".into(), events);

    // 3. Index freshness — compute early so alerts can cross-reference
    let freshness = assemble_freshness();
    pulse.insert("index_freshness".into(), freshness.clone());

    // 4. Alerts — check cooldown files, filter resolved freshness alerts
    let alerts = assemble_alerts(&freshness);
    pulse.insert("alerts".into(), alerts);

    // 5. Nudges — pending counts per role
    let nudges = assemble_nudges();
    pulse.insert("nudges".into(), nudges);

    // 6. Health — service endpoints (cached, not live)
    let health = assemble_health();
    pulse.insert("health".into(), health);

    // 7. Board — WIP from cached snapshot
    let board = assemble_board();
    pulse.insert("board".into(), board);

    let elapsed_ms = start.elapsed().as_millis();
    pulse.insert("elapsed_ms".into(), serde_json::Value::Number(serde_json::Number::from(elapsed_ms as u64)));

    let json = serde_json::Value::Object(pulse);
    let out = serde_json::to_string_pretty(&json).unwrap_or_default();
    let _ = fs::write("/tmp/pulse-latest.json", &out);

    // Compact one-liner to stdout for hook injection
    let compact = serde_json::to_string(&json).unwrap_or_default();
    println!("{}", compact);

    ExitCode::SUCCESS
}

fn assemble_roles() -> serde_json::Value {
    let mut roles = serde_json::Map::new();
    for role in &["wren", "silas", "kade"] {
        let path = format!("/tmp/claude-team-scan/{}-declared.json", role);
        let state = fs::read_to_string(&path).ok()
            .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
            .unwrap_or_else(|| serde_json::json!({"state": "unknown"}));
        roles.insert(role.to_string(), state);
    }
    serde_json::Value::Object(roles)
}

fn assemble_recent_events() -> serde_json::Value {
    let log_path = format!("{}/platform/logs/chorus.log", REPO_ROOT);
    let content = fs::read_to_string(&log_path).unwrap_or_default();

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();

    let mut recent = Vec::new();
    let mut by_role: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut by_type: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

    // Read last 200 lines, filter to last 60s
    for line in content.lines().rev().take(200) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            let ts_str = v.get("timestamp").and_then(|t| t.as_str()).unwrap_or("");
            // Quick age check — parse hour:minute from timestamp
            if let Some(event_epoch) = parse_epoch_approx(ts_str) {
                if now.saturating_sub(event_epoch) <= 60 {
                    let role = v.get("role").and_then(|r| r.as_str()).unwrap_or("system").to_string();
                    let event = v.get("event").and_then(|e| e.as_str()).unwrap_or("unknown").to_string();
                    *by_role.entry(role).or_default() += 1;
                    *by_type.entry(event).or_default() += 1;
                    recent.push(v);
                }
            }
        }
    }

    serde_json::json!({
        "last_60s_count": recent.len(),
        "by_role": by_role,
        "by_type": by_type,
    })
}

fn assemble_alerts(freshness: &serde_json::Value) -> serde_json::Value {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let mut fired_today = Vec::new();

    // Scan /tmp for alert cooldown files matching today
    if let Ok(entries) = fs::read_dir("/tmp") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("alert-") && name.contains(&today) {
                // Extract alert name: alert-{name}-{date}
                let alert_name = name
                    .trim_start_matches("alert-")
                    .rsplit_once('-').map(|(n, _)| n) // remove date suffix
                    .or_else(|| name.strip_prefix("alert-"))
                    .unwrap_or(&name)
                    .to_string();
                if !alert_name.is_empty() && alert_name != today {
                    fired_today.push(alert_name);
                }
            }
        }
    }
    fired_today.sort();
    fired_today.dedup();

    // Filter out resolved freshness alerts when index is healthy (#1889)
    let dead = freshness.get("dead").and_then(|d| d.as_u64()).unwrap_or(1);
    let critical = freshness.get("critical").and_then(|d| d.as_u64()).unwrap_or(1);
    if dead == 0 && critical == 0 {
        fired_today.retain(|name| {
            !name.contains("index-freshness")
                && !name.contains("fuseki-harvest-stale")
                && !name.contains("lancedb-stale")
        });
    }

    serde_json::json!({
        "fired_today": fired_today,
        "count": fired_today.len(),
    })
}

fn assemble_nudges() -> serde_json::Value {
    let mut nudges = serde_json::Map::new();
    for role in &["wren", "silas", "kade"] {
        let path = format!("/tmp/voice-inbox/{}/pending-inject.txt", role);
        let count = fs::read_to_string(&path).ok()
            .map(|c| c.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0);
        let age_secs = fs::metadata(&path).ok()
            .and_then(|m| m.modified().ok())
            .map(|t| t.elapsed().unwrap_or_default().as_secs())
            .unwrap_or(0);
        nudges.insert(role.to_string(), serde_json::json!({
            "pending": count,
            "age_secs": if count > 0 { age_secs } else { 0 },
            "stale": count > 0 && age_secs > 600,
        }));
    }
    serde_json::Value::Object(nudges)
}

fn assemble_health() -> serde_json::Value {
    // Read cached deep-health result (updated every 5min by LaunchAgent)
    let deep_health = fs::read_to_string("/tmp/deep-health-latest.json").ok()
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok());

    if let Some(h) = deep_health {
        h
    } else {
        serde_json::json!({"status": "unknown", "note": "deep-health cache missing"})
    }
}

fn assemble_board() -> serde_json::Value {
    // Live query via cards CLI, fall back to cached snapshot (#1889)
    let snapshot_file = "/tmp/board-wip-snapshot.json";
    let board_ts = format!("{}/platform/scripts/cards", REPO_ROOT);

    // Try live query first
    if let Ok(output) = std::process::Command::new("bash")
        .args(["-l", "-c", &format!("{} list 2>/dev/null", board_ts)])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut wip_cards = Vec::new();
            let mut in_wip = false;
            for line in stdout.lines() {
                if line.starts_with("WIP") || line.contains("[WIP]") || line.contains("In Progress") {
                    in_wip = true;
                    continue;
                }
                if in_wip && line.starts_with("  ") && !line.trim().is_empty() {
                    // Parse: "  1234  Title here [P1]"
                    let trimmed = line.trim();
                    if let Some(id_end) = trimmed.find(|c: char| !c.is_ascii_digit()) {
                        let id = &trimmed[..id_end];
                        let rest = trimmed[id_end..].trim();
                        let title = rest.split('[').next().unwrap_or(rest).trim();
                        let owner = rest.split('[').nth(1)
                            .and_then(|s| s.split('|').next())
                            .unwrap_or("").trim().to_string();
                        if !id.is_empty() {
                            wip_cards.push(serde_json::json!({
                                "id": id.parse::<u64>().unwrap_or(0),
                                "title": title,
                                "owner": if owner.is_empty() { "".to_string() } else { owner },
                                "status": "WIP",
                            }));
                        }
                    }
                } else if in_wip && !line.starts_with("  ") {
                    in_wip = false;
                }
            }
            if !wip_cards.is_empty() {
                // Update cache for other consumers
                let _ = fs::write(snapshot_file, serde_json::to_string(&wip_cards).unwrap_or_default());
                return serde_json::json!({
                    "wip_count": wip_cards.len(),
                    "wip_cards": wip_cards,
                });
            }
        }
    }

    // Fall back to cached snapshot
    if let Ok(content) = fs::read_to_string(snapshot_file) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(cards) = v.as_array() {
                return serde_json::json!({
                    "wip_count": cards.len(),
                    "wip_cards": cards,
                });
            }
        }
    }

    serde_json::json!({"wip_count": "unknown", "note": "board snapshot not found"})
}

fn assemble_freshness() -> serde_json::Value {
    // Quick HTTP fetch from Chorus API — timeout 500ms
    let result = std::process::Command::new("curl")
        .args(["-sf", "--max-time", "0.5", "http://localhost:3340/api/chorus/freshness"])
        .output().ok()
        .and_then(|o| if o.status.success() { String::from_utf8(o.stdout).ok() } else { None })
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());

    if let Some(v) = result {
        if let Some(summary) = v.get("summary") {
            return summary.clone();
        }
    }
    serde_json::json!({"status": "unavailable"})
}

/// Approximate epoch from ISO timestamp string (fast, no full parse)
fn parse_epoch_approx(ts: &str) -> Option<u64> {
    // "2026-04-11T08:04:21-0400" or "2026-04-11T08:04:21.123-0400"
    if ts.len() < 19 { return None; }
    let year: u32 = ts[0..4].parse().ok()?;
    let month: u32 = ts[5..7].parse().ok()?;
    let day: u32 = ts[8..10].parse().ok()?;
    let hour: u32 = ts[11..13].parse().ok()?;
    let min: u32 = ts[14..16].parse().ok()?;
    let sec: u32 = ts[17..19].parse().ok()?;

    let mut days: u64 = 0;
    for y in 1970..year { days += if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 366 } else { 365 }; }
    let month_days = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month { days += month_days[m as usize] as u64; }
    if month > 2 && year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) { days += 1; }
    days += (day - 1) as u64;

    // Adjust for timezone offset if present (e.g., -0400)
    let epoch = days * 86400 + hour as u64 * 3600 + min as u64 * 60 + sec as u64;
    if ts.len() >= 24 {
        let tz_part = &ts[ts.len()-5..];
        if let (Ok(tz_h), Ok(tz_m)) = (tz_part[1..3].parse::<i64>(), tz_part[3..5].parse::<i64>()) {
            let offset = tz_h * 3600 + tz_m * 60;
            if tz_part.starts_with('-') {
                return Some((epoch as i64 + offset) as u64);
            } else {
                return Some((epoch as i64 - offset) as u64);
            }
        }
    }
    Some(epoch)
}
