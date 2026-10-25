// @domain: spine — as its chorus-hooks siblings (#4670)
//! #4637 AC4 — every turn a Jeff message starts carries that message's id.
//!
//! Before this a turn had no link to what woke it: pulse delivered Jeff's line
//! into a role's pane (`jeff.input.surfaced`, id 57629), the role worked, and
//! nothing joined the two. Wren's #4638 tree (pulse-tree.ts) reads
//! `agent.turn.started` and maps `woken_by_line` to the prompt the line is in.
//!
//! On each prompt the hook asks pulse for that role's recent jeff-input rows
//! and picks the newest whose text the prompt carries. A nudge, a task
//! notification or Stop-hook feedback matches no row, so it has no line: a
//! turn without a line is a fact, never a guess. The current turn is also
//! kept in ~/.chorus/run/turn-<role>.json so pipeline-start can stamp the run
//! it starts (`started_by_line`).
//!
//! Off the request, like terminal_input: spawned, 1 s cap on the read, and a
//! pulse that does not answer is a logged `woken_by=unmeasured`, never an
//! absent field that reads like "a nudge did it".

use crate::state::chorus_log;
use serde_json::Value;

/// A line older than this did not wake this turn. Ten minutes covers a line
/// that queued behind a long turn (pulse delivers at idle); past that, a match
/// is more likely a repeat of old words than the line that woke this prompt.
pub const LINE_WINDOW_SECS: i64 = 600;

/// A line shorter than this ("go", "yes", "fuc") must be a whole line of the
/// prompt: as a substring it is in half the prompts the window holds (review, run 3).
pub const MIN_SUBSTRING_CHARS: usize = 12;

/// Whether the typed prompt carries this line of Jeff's.
fn carries(typed: &str, line: &str) -> bool {
    if line.chars().count() >= MIN_SUBSTRING_CHARS {
        return typed.contains(line);
    }
    typed.lines().any(|l| l.trim() == line)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: String,
    pub content: String,
    /// seconds since the epoch the line reached the pane (delivered_at, else created_at)
    pub at: i64,
}

/// pulse rows → Row, skipping any without an id or text
pub fn rows_from(v: &Value) -> Vec<Row> {
    let arr = v.as_array().cloned().unwrap_or_default();
    arr.iter()
        .filter_map(|r| {
            let id = r.get("id").map(|x| x.as_str().map(str::to_string).unwrap_or_else(|| x.to_string()))?;
            let content = r.get("content")?.as_str()?.trim().to_string();
            if id.is_empty() || id == "null" || content.is_empty() {
                return None;
            }
            let when = r.get("delivered_at").and_then(Value::as_str).filter(|s| !s.is_empty() && *s != "None")
                .or_else(|| r.get("created_at").and_then(Value::as_str))?;
            let at = chrono::NaiveDateTime::parse_from_str(when, "%Y-%m-%d %H:%M:%S").ok()?.and_utc().timestamp();
            Some(Row { id, content, at })
        })
        .collect()
}

/// The line that woke this prompt: the newest row, delivered in the window,
/// whose text the prompt carries. None when no row matches (a nudge, a task
/// notification, a turn the role started itself).
pub fn woken_by_line(prompt: &str, rows: &[Row], now: i64) -> Option<String> {
    // a nudge, a task notification, Stop-hook feedback: the harness put it
    // there, so no line of Jeff's woke it (terminal_input's one list)
    let typed = crate::hooks::terminal_input::typed_text(prompt)?;
    let p = typed.as_str();
    rows.iter()
        .filter(|r| now - r.at <= LINE_WINDOW_SECS && r.at <= now + 5)
        .filter(|r| carries(p, r.content.as_str()))
        .max_by_key(|r| (r.at, r.id.parse::<u64>().unwrap_or(0)))
        .map(|r| r.id.clone())
}

fn turn_file(role: &str) -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    std::path::PathBuf::from(std::env::var("CHORUS_TURN_DIR").unwrap_or_else(|_| format!("{home}/.chorus/run")))
        .join(format!("turn-{role}.json"))
}

/// Emit `agent.turn.started` for this prompt and keep it as the role's current turn.
pub fn record(role: &str, session_id: &str, turn_id: &str, prompt: &str) {
    let (role, session_id, turn_id, prompt) = (role.to_string(), session_id.to_string(), turn_id.to_string(), prompt.to_string());
    tokio::spawn(async move {
        let base = std::env::var("CHORUS_PULSE_MESSAGES_URL").unwrap_or_else(|_| "http://localhost:3475/api/messages".to_string());
        let now = chrono::Utc::now().timestamp();
        let since = chrono::DateTime::from_timestamp(now - LINE_WINDOW_SECS, 0).map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string()).unwrap_or_default();
        let url = format!("{base}?type=jeff-input&to={role}&since={since}&limit=50");
        let read = tokio::task::spawn_blocking(move || {
            ureq::get(&url).timeout(std::time::Duration::from_secs(1)).call()
                .map_err(|e| e.to_string())
                .and_then(|r| r.into_json::<Value>().map_err(|e| e.to_string()))
        })
        .await;
        let sid = if session_id.len() > 8 { &session_id[..8] } else { session_id.as_str() };
        let (line, how) = match read {
            Ok(Ok(v)) => match woken_by_line(&prompt, &rows_from(&v), chrono::Utc::now().timestamp()) {
                Some(id) => (id, "line"),
                None => (String::new(), "none"),
            },
            Ok(Err(e)) => { chorus_log("turn.line.unmeasured", &role, &[("error", e.as_str())]).await; (String::new(), "unmeasured") }
            Err(e) => { let e = e.to_string(); chorus_log("turn.line.unmeasured", &role, &[("error", e.as_str())]).await; (String::new(), "unmeasured") }
        };
        let mut kv: Vec<(&str, &str)> = vec![("session_id", sid), ("turn_id", turn_id.as_str()), ("woken_by", how)];
        if !line.is_empty() {
            kv.push(("woken_by_line", line.as_str()));
        }
        chorus_log("agent.turn.started", &role, &kv).await;
        let body = serde_json::json!({ "turn_id": turn_id, "session_id": sid, "woken_by_line": if line.is_empty() { Value::Null } else { Value::String(line.clone()) }, "at": now });
        // write then rename: pipeline-start never reads a half-written turn
        let path = turn_file(&role);
        let tmp = path.with_extension(format!("json.{}", std::process::id()));
        if std::fs::write(&tmp, body.to_string()).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, content: &str, at: i64) -> Row {
        Row { id: id.into(), content: content.into(), at }
    }

    #[test]
    fn a_pulse_delivered_line_is_the_line_that_woke_the_turn() {
        let rows = vec![row("57628", "older thing", 900), row("57629", "@silas r u working on 4637", 1000)];
        assert_eq!(woken_by_line("@silas r u working on 4637", &rows, 1003).as_deref(), Some("57629"));
    }

    #[test]
    fn a_burst_maps_to_its_newest_line() {
        // two lines pasted into one prompt: the newest is the waking line (the tree joins the burst)
        let rows = vec![row("1", "first", 1000), row("2", "second", 1002)];
        assert_eq!(woken_by_line("first\nsecond", &rows, 1004).as_deref(), Some("2"));
    }

    // NEGATIVE PROOF: a turn a nudge started carries no line, and neither does
    // a prompt whose matching line is outside the window — never a guess.
    #[test]
    fn negative_proof_a_nudge_or_a_stale_line_wakes_no_turn() {
        let rows = vec![row("57629", "go", 1000)];
        assert_eq!(woken_by_line("[nudge from kade | 19:39] who completed the go?", &rows, 1005), None,
            "a nudge that happens to contain 'go' is not Jeff's line");
        assert_eq!(woken_by_line("go", &rows, 1000 + LINE_WINDOW_SECS + 1), None, "a line older than the window");
        assert_eq!(woken_by_line("", &rows, 1001), None);
    }

    // NEGATIVE PROOF (review, run 3): a short line inside other words is not the
    // line; the same short line as a whole line of the prompt is.
    #[test]
    fn negative_proof_a_short_line_must_be_a_whole_line() {
        let rows = vec![row("7", "go", 1000), row("8", "yes", 1000)];
        assert_eq!(woken_by_line("let's go over the plan", &rows, 1002), None, "'go' inside other words");
        assert_eq!(woken_by_line("eyes on #4716", &rows, 1002), None, "'yes' inside a word");
        assert_eq!(woken_by_line("go", &rows, 1002).as_deref(), Some("7"));
        assert_eq!(woken_by_line("@silas r u there\nyes", &rows, 1002).as_deref(), Some("8"));
    }

    #[test]
    fn rows_parse_from_pulse_and_skip_empty_ones() {
        let v: Value = serde_json::json!([
            {"id": "57629", "content": "hi", "created_at": "2026-10-25 00:22:10", "delivered_at": "2026-10-25 00:22:11"},
            {"id": "57630", "content": "  ", "created_at": "2026-10-25 00:22:10"},
            {"id": 57631, "content": "num id", "created_at": "2026-10-25 00:23:00", "delivered_at": "None"}
        ]);
        let r = rows_from(&v);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].id, "57629");
        assert_eq!(r[1].id, "57631");
        assert_eq!(r[0].at, chrono::NaiveDateTime::parse_from_str("2026-10-25 00:22:11", "%Y-%m-%d %H:%M:%S").unwrap().and_utc().timestamp());
    }
}
