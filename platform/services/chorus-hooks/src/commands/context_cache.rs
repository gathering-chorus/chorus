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
    let board_ts = format!("{}/platform/scripts/cards", REPO_ROOT);
    let out_path = format!("/tmp/session-context-{}.md", role);

    // Werk version
    let manifest_path = format!("{}/designing/claudemd/manifest.json", REPO_ROOT);
    let werk_version = fs::read_to_string(&manifest_path).ok()
        .and_then(|c| c.lines().find(|l| l.contains("\"version\"")).map(|l| {
            l.split('"').nth(3).unwrap_or("unknown").to_string()
        }))
        .unwrap_or_else(|| "unknown".to_string());

    // --- Parallel data gathering via threads ---
    // Board: active cards only — filter out Done/Won't Do (#1781)
    let board_ts_c = board_ts.clone();
    let role_s = role.to_string();
    let board_mine = std::thread::spawn(move || -> (bool, String) {
        match Cmd::new("zsh").arg("-lc").arg(format!("{} mine {}", board_ts_c, role_s)).output() {
            Ok(o) if o.status.success() => {
                let full = String::from_utf8(o.stdout).unwrap_or_default();
                let filtered: Vec<&str> = full.lines()
                    .filter(|l| {
                        let lt = l.trim().to_lowercase();
                        !lt.contains("[done]") && !lt.contains("[won't do]")
                    })
                    .collect();
                (true, filtered.join("\n"))
            }
            _ => (false, String::new()),
        }
    });

    let board_ts_c = board_ts.clone();
    let role_s = role.to_string();
    let board_audit = std::thread::spawn(move || {
        Cmd::new("zsh").arg("-lc").arg(format!("{} audit-start {}", board_ts_c, role_s))
            .output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default()
    });

    // Last session context is now fetched by the role at boot via Chorus query (#1781)
    // No git log parsing needed — the role synthesizes from semantic search results.

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

    // Pending briefs — filesystem is truth (#2113): briefs/ top-level = pending, anything
    // moved out (briefs/archive/ subdir or briefs-archive/ sibling) is invisible here.
    let briefs_dir_c = briefs_dir.clone();
    let handoff_check = std::thread::spawn(move || {
        scan_briefs_pending(std::path::Path::new(&briefs_dir_c))
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

    let audit_text = board_audit.join().unwrap_or_default();
    if !audit_text.is_empty() { ok_sources.push("board_audit"); } else { failed_sources.push("board_audit"); }

    // last_session moved to Chorus query at boot — no thread to join

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

    let activity_path = format!("{}/activity.md", REPO_ROOT);
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

    // Open threads: read next-session.md (role's own synthesis of what's pending)
    let next_session_path = format!("{}/next-session.md", role_dir);
    let next_session_text = fs::read_to_string(&next_session_path).unwrap_or_default();

    // Recent activity.md entries for this role
    let activity_entries = {
        let content = fs::read_to_string(&activity_path).unwrap_or_default();
        let role_lower = role.to_lowercase();
        content.lines()
            .filter(|l| {
                let lt = l.to_lowercase();
                lt.contains(&format!("[{}]", role_lower))
            })
            .take(10)
            .collect::<Vec<_>>()
            .join("\n")
    };

    // --- Assemble — signal-first, ~80 lines target (#1781) ---
    let mut out = String::with_capacity(4096);
    out.push_str(&format!("# {} — Session Context (Werk v{})\n", role_cap, werk_version));
    out.push_str(&format!("Generated: {} Boston | Werk v{}\n\n", clock_short, werk_version));

    // 1. Session comprehension — Pulse + Chorus query (#1781, #1881, #1902)
    out.push_str("## Boot: Think, Then Speak\n\n");
    out.push_str(&format!(
        "Before your first response, read team state and query Chorus:\n\
         ```\n\
         cat /tmp/pulse-latest.json\n\
         curl -s \"http://localhost:3340/api/chorus/search?q={role}+last+session+shipped+friction+decisions&limit=10\"\n\
         ```\n\n\
         Then write a thesis-driven opening — not a status report. Five beats, prose only, no headers or bullets, ~200–300 words total:\n\n\
         1. **What you've been thinking about.** Lead with the thought, not the data. Name a threshold, shift, reframe, or meaning the day carries. One thesis sentence, then the evidence that earns it.\n\n\
         2. **Reframe an active card through the thesis.** \"That reframes #X. On the surface it's [mundane frame]. It's actually [deeper frame]. I want it done before Y because Z.\"\n\n\
         3. **Quieter, older friction — with a position.** Not the loud stuff — the second-order thing that's been sitting. Name what it actually is. State your position on what to do.\n\n\
         4. **The thing you keep flinching at.** A card, a pattern, an avoidance of your own. Name the flinch honestly — the avoidance itself is the signal.\n\n\
         5. **One-question close.** Offer directions without assuming one. \"Where do you want to start — A, B, or somewhere I'm not looking?\"\n\n\
         **Rules:**\n\
         - No card lists, no pulse bullets, no section headers from this file in your opening.\n\
         - Every problem named gets a position: \"X is stale — I'd do Y,\" not \"X is stale.\"\n\
         - If Pulse shows index_freshness critical/dead, note it naturally — your recall may be incomplete.\n\
         - If you cannot write the thesis sentence in beat 1, you have not synthesized. Read more before opening.\n\
         - Sound like a colleague who was here yesterday and has been thinking about the work overnight.\n\n\
         **Example shape** (illustrative — yours should come from today's actual state):\n\n\
         > Thinking overnight about what shipped last night: Borg crossed from \"reflection collection\" to a real product surface. Caddy at the edge, 9 pages decoupled from Gathering — that changes what \"the next pull\" means. It reframes #2116. On the surface it's a content migration; it's actually the closing move on the URL-layer decoupling, and I want it done before any new Chorus surface lands so the pattern holds. Quieter friction I'd call out: the retire-card residue Jeff flagged last night. #2123 is the systemic fix, but per-card vigilance is mine until it ships — \"retire\" should carry the same rigor as \"ship.\" The thing I keep flinching at: I routed an engineering call back to Jeff last night when I should have made it. That's #1158 at a new scale — still relaying instead of holding. Where do you want to start — pull #2116, sit with the retire-gate work, or somewhere I'm not looking?\n"
    , role=role));

    // 2. Active Work — WIP + Now + Ops + Later only (no Done wall)
    out.push_str("\n## Active Cards\n");
    out.push_str(if !mine_ok { "(board unreachable)" } else if mine_text.is_empty() { "(none)" } else { &mine_text });

    // 3. Open Threads — next-session.md + activity + handoffs + briefs
    out.push_str("\n\n## Open Threads\n\n");
    if !next_session_text.is_empty() {
        out.push_str("### Next Session Notes\n");
        out.push_str(&next_session_text);
        out.push('\n');
    }
    if !activity_entries.is_empty() {
        out.push_str("\n### Recent Activity\n");
        out.push_str(&activity_entries);
        out.push('\n');
    }
    if !handoff_text.is_empty() {
        out.push_str("\n### Pending Handoffs\n");
        out.push_str(&handoff_text);
        out.push('\n');
    }
    if !briefs.is_empty() {
        out.push_str("\n### Recent Briefs\n");
        out.push_str(&briefs);
        out.push('\n');
    }
    if next_session_text.is_empty() && activity_entries.is_empty()
        && handoff_text.is_empty() && briefs.is_empty() {
        out.push_str("(clean)\n");
    }

    // 4. Board Audit
    if !audit_text.is_empty() {
        out.push_str("\n## Board Audit\n\n");
        out.push_str(&audit_text);
    }

    // 5. Recent Decisions
    if !decisions.is_empty() {
        out.push_str("\n\n## Recent Decisions\n");
        out.push_str(&decisions);
    }

    // 6. Health — compact
    out.push_str("\n\n## Health\n\n");
    out.push_str(&format!("- Disk: {}\n", disk_pct));
    out.push_str(&format!("- Uncommitted in {}/: {}\n", role_dir_name, uncommitted));
    out.push_str(&format!("- Activity.md: updated {} ago\n", activity_age));
    out.push_str(&format!("- CLAUDE.md: {}\n", claude_status));

    // 7. Memory Context
    if !memory_text.is_empty() {
        out.push_str("\n## Memory Context\n");
        let line_count = memory_text.lines().count();
        out.push_str(&format!("Related memories for WIP cards ({} found):\n\n", line_count));
        out.push_str(&memory_text);
    }
    out.push('\n');

    let _ = fs::write(&out_path, &out);
    // Also write session-start file — roles read this, not the context cache.
    // Without this, session-start-<role>.md goes stale forever (#1781 bug).
    let start_path = format!("/tmp/session-start-{}.md", role);
    let _ = fs::write(&start_path, &out);
    let lines = out.lines().count();
    println!("Context cached: {} ({} lines)", out_path, lines);

    // Spine events — AC for #1808
    let log_path = format!("{}/platform/logs/chorus.log", REPO_ROOT);
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

/// Fetch last session git log per role — own commits + 1-liner for other roles (#1781)
/// Returns (own_commits_text, vec of (role, summary) for other roles)
fn fetch_last_session_log(role: &str) -> (String, Vec<(String, String)>) {
    // Find the date of the last reboot commit for this role to set the --since boundary
    // Use git directly with full path — zsh -lc quoting eats grep patterns
    // Use zsh -lc to get homebrew git in PATH — /usr/bin/git (Xcode shim) has date parsing issues
    // Find the SECOND-to-last reboot for this role — that's the session boundary.
    // Commits between reboot N-1 and reboot N are the last session's work.
    let git_log = Cmd::new("zsh").arg("-lc")
        .arg(format!(
            concat!(
                "PREV=$(git -C {} log --oneline --no-merges -n 2 ",
                "--grep=\"{}: session reboot\" --format=%aI 2>/dev/null | tail -1); ",
                "if [ -z \"$PREV\" ]; then PREV=\"3 days ago\"; fi; ",
                "git -C {} log --oneline --no-merges --since=\"$PREV\""
            ),
            REPO_ROOT, role, REPO_ROOT
        ))
        .output().ok()
        .and_then(|o| if o.status.success() {
            String::from_utf8(o.stdout).ok()
        } else { None })
        .unwrap_or_default();

    let mut own_commits = Vec::new();
    let mut other_roles: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

    for line in git_log.lines().take(50) {
        let line = line.trim();
        if line.is_empty() { continue; }
        // Format: "abc123 role: message"
        let after_hash = match line.split_whitespace().nth(1) {
            Some(w) => {
                let pos = line.find(w).unwrap_or(0);
                &line[pos..]
            }
            None => continue,
        };

        if after_hash.starts_with(&format!("{}:", role)) {
            own_commits.push(line.to_string());
        } else {
            if let Some(colon_pos) = after_hash.find(':') {
                let r = after_hash[..colon_pos].trim().to_string();
                if matches!(r.as_str(), "wren" | "silas" | "kade") {
                    other_roles.entry(r).or_default().push(line.to_string());
                }
            }
        }
    }

    // Cap own commits at 20 lines
    own_commits.truncate(20);

    let other_summaries: Vec<(String, String)> = other_roles
        .into_iter()
        .map(|(r, commits)| {
            let summary = if let Some(reboot) = commits.iter().find(|c| c.contains("reboot")) {
                // Reboot commit is already a synthesis — use its message
                let msg_start = reboot.find(&format!("{}:", r)).map(|p| p + r.len() + 2).unwrap_or(0);
                reboot[msg_start..].to_string()
            } else {
                format!("{} commits", commits.len())
            };
            (r, summary)
        })
        .collect();

    (own_commits.join("\n"), other_summaries)
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

// #2113: filesystem-as-truth brief scanner. handoffs.log retained for audit but
// no longer consulted — it had no "received" writer, so pending never cleared.
//
// KEEP IN SYNC WITH tests/briefs_scanner.rs::{is_real_brief, scan_briefs_pending}

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

fn scan_briefs_pending(briefs_dir: &std::path::Path) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let entries = match std::fs::read_dir(briefs_dir) {
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
