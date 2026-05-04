//! Pulse service — event-bus cache over team-state producers (#1881, #2280).
//!
//! Each section in pulse-latest.json is owned by a separate upstream producer
//! writing on its own cadence. Pulse reads, annotates with age, and exposes.
//! It does not filter, suppress, or synthesize. Consumers decide what to act on
//! given raw state and freshness.
//!
//! Producers: role_state.rs (declared), observer.rs (inferred), chorus-log
//! (events), alert scripts (cooldown files), nudge CLI (voice-inbox),
//! deep-health.sh (health), cards CLI (board snapshot), /api/chorus/freshness
//! (index_freshness). See designing/docs/pulse-service-design.md for the full
//! producer/consumer inventory and the reframe from aggregator → event-bus.
//!
//! Target: <200ms. All file reads, no shell spawning.

use std::fs;
use std::process::ExitCode;

use crate::shared::state_paths::repo_root;

/// Assemble team state snapshot and write to /tmp/pulse-latest.json.
/// Returns compact JSON string. Callers that need only the side-effect
/// (session-start under #2311 rescope — pulse file refresh without
/// stdout pollution) use this; hook callers that pipe JSON to stdout
/// use `run()`.
pub fn assemble() -> String {
    let start = std::time::Instant::now();
    let mut pulse = serde_json::Map::new();

    // Timestamp
    let clock = crate::process::wall_clock();
    let clock_short: String = clock.chars().take(19).collect();
    pulse.insert("timestamp".into(), serde_json::Value::String(clock_short));

    // #2168 AC-12 — per-section timings for profiling. Each assemble_* is
    // wrapped so we can see where the pulse-assembly budget is spent.
    let mut timings = serde_json::Map::new();
    macro_rules! timed {
        ($name:expr, $expr:expr) => {{
            let t0 = std::time::Instant::now();
            let v = $expr;
            timings.insert($name.to_string(),
                serde_json::Value::Number(serde_json::Number::from(t0.elapsed().as_millis() as u64)));
            v
        }};
    }

    // 1. Role states — read 3 JSON files from /tmp/claude-team-scan/
    let roles = timed!("roles_ms", assemble_roles());
    pulse.insert("roles".into(), roles);

    // 2. Spine events — last 60s from chorus.log
    let events = timed!("events_ms", assemble_recent_events());
    pulse.insert("events".into(), events);

    // 3. Index freshness — compute early so alerts can cross-reference
    let freshness = timed!("freshness_ms", assemble_freshness());
    pulse.insert("index_freshness".into(), freshness.clone());

    // 4. Alerts — check cooldown files, filter resolved freshness alerts
    let alerts = timed!("alerts_ms", assemble_alerts(&freshness));
    pulse.insert("alerts".into(), alerts);

    // 5. Nudges — pending counts per role
    let nudges = timed!("nudges_ms", assemble_nudges());
    pulse.insert("nudges".into(), nudges);

    // 6. Health — service endpoints (cached, not live)
    let health = timed!("health_ms", assemble_health());
    pulse.insert("health".into(), health);

    // 7. Board — WIP from cached snapshot
    let board = timed!("board_ms", assemble_board());
    pulse.insert("board".into(), board);

    let elapsed_ms = start.elapsed().as_millis();
    pulse.insert("elapsed_ms".into(), serde_json::Value::Number(serde_json::Number::from(elapsed_ms as u64)));
    pulse.insert("timings".into(), serde_json::Value::Object(timings));

    let json = serde_json::Value::Object(pulse);
    let out = serde_json::to_string_pretty(&json).unwrap_or_default();
    let _ = fs::write("/tmp/pulse-latest.json", &out);

    serde_json::to_string(&json).unwrap_or_default()
}

/// Hook-invocation entry point. Writes pulse file and prints compact JSON
/// to stdout for hook injection.
pub fn run(_args: &[String]) -> ExitCode {
    let compact = assemble();
    println!("{}", compact);
    ExitCode::SUCCESS
}

fn assemble_roles() -> serde_json::Value {
    // #2168 AC-9: compose declared + inferred into one flat role entry.
    // declared.json is primary (top-level state, card, ts, source="declared"
    // — backward-compat for tiles.ts). inferred.json surfaces as
    // card_inferred + divergent + inferred_stale flags.
    // Freshness window is 5 min — enforced here, not in the observer writer.
    const INFERRED_TTL_SECS: u64 = 300;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut roles = serde_json::Map::new();
    for role in &["wren", "silas", "kade"] {
        let decl_path = format!("/tmp/claude-team-scan/{}-declared.json", role);
        let declared = fs::read_to_string(&decl_path).ok()
            .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
            .unwrap_or_else(|| serde_json::json!({"state": "unknown"}));

        let inf_path = format!("/tmp/claude-team-scan/{}-inferred.json", role);
        let inferred = fs::read_to_string(&inf_path).ok()
            .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok());

        let declared_card = declared.get("card").cloned();
        let (card_inferred, inferred_stale) = match inferred.as_ref() {
            Some(v) => {
                let ts = v.get("ts").and_then(|t| t.as_u64()).unwrap_or(0);
                let stale = now.saturating_sub(ts) > INFERRED_TTL_SECS;
                (v.get("card").cloned(), stale)
            }
            None => (None, true),
        };
        let divergent = match (&declared_card, &card_inferred, inferred_stale) {
            (Some(d), Some(i), false) => d != i,
            _ => false,
        };

        let mut composed = declared.clone();
        if let Some(obj) = composed.as_object_mut() {
            if let Some(c) = declared_card {
                obj.insert("card_declared".into(), c);
            }
            if let Some(c) = card_inferred {
                obj.insert("card_inferred".into(), c);
            }
            obj.insert("divergent".into(), serde_json::Value::Bool(divergent));
            obj.insert("inferred_stale".into(), serde_json::Value::Bool(inferred_stale));
        }
        roles.insert(role.to_string(), composed);
    }
    serde_json::Value::Object(roles)
}

fn assemble_recent_events() -> serde_json::Value {
    let log_path = format!("{}/platform/logs/chorus.log", repo_root());
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
    // #2664: source from the spine fold (nudge.emitted minus nudge.surfaced)
    // instead of the retired voice-inbox file path. The voice-inbox writer
    // (inject-watcher) was retired by #2435 along with the LaunchAgent;
    // the path-check was the source of the dead nudge-stale alert that
    // fired 13+/day. Reuses nudge_poll's fold semantics inline (shim crate
    // doesn't have hooks::nudge_poll in scope; this is a minimal duplicate
    // of the substring filter — full parsing lives in nudge_poll::fetch_unread).
    let log_path = format!("{}/platform/logs/chorus.log", repo_root());
    let content = fs::read_to_string(&log_path).unwrap_or_default();
    let lines: Vec<&str> = content.lines().collect();
    let window = 5000usize;
    let start = lines.len().saturating_sub(window);
    let recent = &lines[start..];

    let mut nudges = serde_json::Map::new();
    for role in &["wren", "silas", "kade"] {
        let to_marker = format!("to={}", role);
        let role_marker = format!(r#""role":"{}""#, role);
        let mut emitted_traces: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut surfaced_traces: std::collections::HashSet<String> = std::collections::HashSet::new();
        for line in recent {
            let is_emitted = line.contains(r#""event":"nudge.emitted""#) && line.contains(&to_marker);
            let is_surfaced = line.contains(r#""event":"nudge.surfaced""#) && line.contains(&role_marker);
            if !is_emitted && !is_surfaced { continue; }
            // Trace id key: `trace=ntr-...,` — bounded by `,` or end of value.
            let Some(idx) = line.find("trace=") else { continue; };
            let tail = &line[idx + 6..];
            let end = tail.find([',', '"']).unwrap_or(tail.len());
            let trace = tail[..end].to_string();
            if is_surfaced { surfaced_traces.insert(trace); }
            else { emitted_traces.insert(trace); }
        }
        let pending = emitted_traces.difference(&surfaced_traces).count();
        nudges.insert(role.to_string(), serde_json::json!({
            "pending": pending,
            "age_secs": 0,
            "stale": false,
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
    // #2168 AC-12 — cache-primary read path. Shell-out to `cards list` is
    // ~390ms (Vikunja round-trip), far over the sub-100ms budget. Read
    // snapshot first; only refresh when stale. Cards CLI also writes the
    // snapshot on its own mutations, so staleness is bounded by pulse cadence.
    const SNAPSHOT_TTL_SECS: u64 = 10;
    let snapshot_file = "/tmp/board-wip-snapshot.json";

    // Fast path: fresh snapshot.
    let snapshot_age = fs::metadata(snapshot_file).ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.elapsed().ok())
        .map(|d| d.as_secs())
        .unwrap_or(u64::MAX);

    if snapshot_age <= SNAPSHOT_TTL_SECS {
        if let Ok(content) = fs::read_to_string(snapshot_file) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(obj) = v.as_object() {
                    // New snapshot format: {wip_cards, swat_cards, next_cards}.
                    // next_cards added #2252 — symmetric with wip+swat, read
                    // path for /api/chorus/context/board/next.
                    let wip = obj.get("wip_cards").and_then(|c| c.as_array()).cloned().unwrap_or_default();
                    let swat = obj.get("swat_cards").and_then(|c| c.as_array()).cloned().unwrap_or_default();
                    let next = obj.get("next_cards").and_then(|c| c.as_array()).cloned().unwrap_or_default();
                    return serde_json::json!({
                        "wip_count": wip.len(),
                        "wip_cards": wip,
                        "swat_cards": swat,
                        "next_cards": next,
                    });
                } else if let Some(cards) = v.as_array() {
                    // Legacy snapshot format: flat array of wip_cards only
                    return serde_json::json!({
                        "wip_count": cards.len(),
                        "wip_cards": cards,
                        "swat_cards": [],
                        "next_cards": [],
                    });
                }
            }
        }
    }

    // Stale path: refresh via cards CLI, update snapshot as side effect.
    let board_ts = format!("{}/platform/scripts/cards", repo_root());
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
    if let Ok(output) = std::process::Command::new("bash")
        .args(["-l", "-c", &format!("{} list 2>/dev/null", board_ts)])
        .env("CHORUS_ROOT", repo_root())
        .env("HOME", &home)
        .env("PATH", format!("{}/CascadeProjects/chorus/platform/scripts:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin", home))
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let wip_cards = parse_wip_list(&stdout);
            let swat_cards = parse_section_list(&stdout, "SWAT");
            let next_cards = parse_section_list(&stdout, "Next");
            if !wip_cards.is_empty() || !swat_cards.is_empty() || !next_cards.is_empty() {
                let snapshot = serde_json::json!({
                    "wip_cards": &wip_cards,
                    "swat_cards": &swat_cards,
                    "next_cards": &next_cards,
                });
                let _ = fs::write(snapshot_file, serde_json::to_string(&snapshot).unwrap_or_default());
                return serde_json::json!({
                    "wip_count": wip_cards.len(),
                    "wip_cards": wip_cards,
                    "swat_cards": swat_cards,
                    "next_cards": next_cards,
                });
            }
        }
    }

    // Last resort: stale-but-present snapshot is better than "unknown."
    if let Ok(content) = fs::read_to_string(snapshot_file) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(cards) = v.as_array() {
                return serde_json::json!({
                    "wip_count": cards.len(),
                    "wip_cards": cards,
                    "swat_cards": [],
                    "snapshot_stale": true,
                });
            }
        }
    }

    serde_json::json!({"wip_count": "unknown", "swat_cards": [], "note": "board snapshot not found"})
}

/// Parse `cards list` stdout into WIP card records.
/// Line shape: `  <id>  <title> [<owner>|<prio>|<tags>...]`
/// Title may itself start with `[tag]` (e.g. `[swat] ...`), so the metadata
/// block must be split on the *last* `[`, not the first.
fn parse_wip_list(stdout: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let mut in_wip = false;
    for line in stdout.lines() {
        if line.starts_with("WIP") || line.contains("[WIP]") || line.contains("In Progress") {
            in_wip = true;
            continue;
        }
        if in_wip && line.starts_with("  ") && !line.trim().is_empty() {
            let trimmed = line.trim();
            let Some(id_end) = trimmed.find(|c: char| !c.is_ascii_digit()) else { continue };
            let id = &trimmed[..id_end];
            if id.is_empty() { continue }
            let rest = trimmed[id_end..].trim();
            let (title, owner, domain) = match rest.rsplit_once('[') {
                Some((title_part, meta)) => {
                    let meta = meta.trim_end_matches(']');
                    let mut parts = meta.split('|').map(|s| s.trim());
                    let owner = parts.next().unwrap_or("").to_string();
                    let domain = parts
                        .find_map(|p| p.strip_prefix("domain:"))
                        .unwrap_or("")
                        .to_string();
                    (title_part.trim().to_string(), owner, domain)
                }
                None => (rest.to_string(), String::new(), String::new()),
            };
            out.push(serde_json::json!({
                "id": id.parse::<u64>().unwrap_or(0),
                "title": title,
                "owner": owner,
                "domain": domain,
                "status": "WIP",
            }));
        } else if in_wip && !line.starts_with("  ") {
            in_wip = false;
        }
    }
    out
}

/// Parse a named section (e.g. "SWAT") from `cards list` stdout.
/// Same line shape as parse_wip_list but parameterized on section header.
fn parse_section_list(stdout: &str, section: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let mut in_section = false;
    for line in stdout.lines() {
        if line.starts_with(section) {
            in_section = true;
            continue;
        }
        if in_section && line.starts_with("  ") && !line.trim().is_empty() {
            let trimmed = line.trim();
            let Some(id_end) = trimmed.find(|c: char| !c.is_ascii_digit()) else { continue };
            let id = &trimmed[..id_end];
            if id.is_empty() { continue }
            let rest = trimmed[id_end..].trim();
            let (title, owner, domain) = match rest.rsplit_once('[') {
                Some((title_part, meta)) => {
                    let meta = meta.trim_end_matches(']');
                    let mut parts = meta.split('|').map(|s| s.trim());
                    let owner = parts.next().unwrap_or("").to_string();
                    let domain = parts
                        .find_map(|p| p.strip_prefix("domain:"))
                        .unwrap_or("")
                        .to_string();
                    (title_part.trim().to_string(), owner, domain)
                }
                None => (rest.to_string(), String::new(), String::new()),
            };
            out.push(serde_json::json!({
                "id": id.parse::<u64>().unwrap_or(0),
                "title": title,
                "owner": owner,
                "domain": domain,
                "status": section,
            }));
        } else if in_section && !line.starts_with("  ") {
            in_section = false;
        }
    }
    out
}

fn assemble_freshness() -> serde_json::Value {
    // #2168 AC-12 — cache-primary read path, 30s TTL. The /api/chorus/freshness
    // endpoint itself is 259-418ms per call; freshness data updates on indexing
    // cadence (minutes/hours), so a 30s stale snapshot is well within tolerance.
    // Same pattern as assemble_board. Endpoint-side latency fix is out of scope.
    const FRESHNESS_TTL_SECS: u64 = 30;
    let snapshot_file = "/tmp/freshness-snapshot.json";

    let snapshot_age = fs::metadata(snapshot_file).ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.elapsed().ok())
        .map(|d| d.as_secs())
        .unwrap_or(u64::MAX);

    // Fast path: fresh snapshot.
    if snapshot_age <= FRESHNESS_TTL_SECS {
        if let Ok(content) = fs::read_to_string(snapshot_file) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                return v;
            }
        }
    }

    // Stale path: refresh via endpoint, cache the summary.
    let result = std::process::Command::new("curl")
        .args(["-sf", "--max-time", "0.5", "http://localhost:3340/api/chorus/freshness"])
        .output().ok()
        .and_then(|o| if o.status.success() { String::from_utf8(o.stdout).ok() } else { None })
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok());

    if let Some(v) = result {
        if let Some(summary) = v.get("summary") {
            let summary_clone = summary.clone();
            if let Ok(out) = serde_json::to_string(&summary_clone) {
                let _ = fs::write(snapshot_file, out);
            }
            return summary_clone;
        }
    }

    // Last resort: stale snapshot is better than "unavailable".
    if let Ok(content) = fs::read_to_string(snapshot_file) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            return v;
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
    if month > 2 && year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) { days += 1; }
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

#[cfg(test)]
mod tests {
    use super::parse_wip_list;

    const SAMPLE: &str = concat!(
        "WIP (4):\n",
        "  2151  Stand up loom-policies sub-domain — policies layer of roles-dependency chain [Wren|P2|domain:chorus|type:new]\n",
        "  2154  [swat] Migrate platform/pulse store.test.ts from custom Node runner to jest [Silas|P1|chunk:ops|domain:chorus]\n",
        "  2167  Wire coverage tooling across chorus + push to 80% [Kade|P1|chunk:ops|domain:chorus|type:enhance]\n",
        "  2168  [swat] Wire pulse+spine+athena into per-prompt context-synthesis envelope [Silas|P1|chorus|chunk:ops|domain:chorus|type:swat]\n",
        "\n",
        "Next (1):\n",
        "  950  iOS app [Jeff|P2]\n",
    );

    #[test]
    fn plain_title() {
        let cards = parse_wip_list(SAMPLE);
        let c = cards.iter().find(|c| c["id"] == 2151).unwrap();
        assert_eq!(c["owner"], "Wren");
        assert!(c["title"].as_str().unwrap().starts_with("Stand up loom-policies"));
    }

    #[test]
    fn bracketed_title_parses_owner_correctly() {
        let cards = parse_wip_list(SAMPLE);
        let c = cards.iter().find(|c| c["id"] == 2168).unwrap();
        assert_eq!(c["owner"], "Silas", "owner must be Silas, not 'swat] ...'");
        assert!(
            c["title"].as_str().unwrap().starts_with("[swat] Wire pulse"),
            "title must retain the [swat] tag, got: {}",
            c["title"]
        );
    }

    #[test]
    fn bracketed_title_second_card() {
        let cards = parse_wip_list(SAMPLE);
        let c = cards.iter().find(|c| c["id"] == 2154).unwrap();
        assert_eq!(c["owner"], "Silas");
        assert!(c["title"].as_str().unwrap().starts_with("[swat] Migrate"));
    }

    #[test]
    fn stops_at_next_section() {
        let cards = parse_wip_list(SAMPLE);
        assert_eq!(cards.len(), 4, "must not spill into Next section");
        assert!(cards.iter().all(|c| c["id"] != 950));
    }

    #[test]
    fn extracts_domain_from_metadata() {
        let cards = parse_wip_list(SAMPLE);
        let c2168 = cards.iter().find(|c| c["id"] == 2168).unwrap();
        assert_eq!(c2168["domain"], "chorus", "domain must be extracted from `domain:chorus` tag");
        let c2167 = cards.iter().find(|c| c["id"] == 2167).unwrap();
        assert_eq!(c2167["domain"], "chorus");
    }

    #[test]
    fn no_metadata_block() {
        let cards = parse_wip_list("WIP (1):\n  9999  Bare title no brackets\n");
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0]["id"], 9999);
        assert_eq!(cards[0]["owner"], "");
        assert_eq!(cards[0]["title"], "Bare title no brackets");
    }
}
