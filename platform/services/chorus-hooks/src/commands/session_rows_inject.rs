//! #4360 — SessionStart reads the role's own rows: the open Session, the run
//! that ended last (where we left off), and the /sup walk.
//!
//! Jeff runs `chorus-principal login wren`, and wren's first line tells him
//! where they left off and what's open. Before this the boot context was
//! markdown files and a chat search.
//!
//! Sources, each with a 3s ceiling so a slow door never holds the boot:
//!   athena-make  /v1/identity/sessions     open row that actsAs role-<role>, newest startedAt
//!   athena-make  /v1/identity/sessionruns  newest <role>-run-* with a runEndedAt
//!   chorus-api   /api/chorus/context/priorities?role=  top 5 chunks, 3 cards each
//!
//! A read that fails says which row and why. No open Session says so and
//! prints no Session line: the boot never invents a thread.
//!
//! This loosens #2940's static-JSON-only boot invariant for these three
//! reads (flagged for gate:arch): they are the role's own rows, and a boot
//! that cannot name them is the silent-empty boot this card removes.
//!
//! Test seam: CHORUS_SESSION_ROWS_FIXTURE_DIR holds sessions.json,
//! sessionruns.json and priorities.json (a missing file reads as a failure).
//! CHORUS_ATHENA_MAKE_URL and CHORUS_API_URL override the doors.

use serde_json::Value;

const ATHENA_MAKE: &str = "http://localhost:3360";
const CHORUS_API: &str = "http://localhost:3340";

/// One source's answer: the parsed body, or why it could not be read.
pub type Read = Result<Value, String>;

pub struct Reads {
    pub sessions: Read,
    pub runs: Read,
    pub priorities: Read,
}

fn fetch_url(url: &str) -> Read {
    let out = std::process::Command::new("curl")
        .args(["-s", "-f", "--max-time", "3", url])
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if !out.status.success() {
        return Err(format!("{url} {}", curl_failure(out.status.code().unwrap_or(-1))));
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("{url} answered with unreadable JSON: {e}"))
}

/// curl's exit code in words, so the boot line says what happened.
fn curl_failure(code: i32) -> String {
    match code {
        7 => "refused the connection (curl exit 7)".to_string(),
        22 => "answered with an HTTP error (curl exit 22)".to_string(),
        28 => "timed out after 3s (curl exit 28)".to_string(),
        6 => "host not found (curl exit 6)".to_string(),
        n => format!("did not answer (curl exit {n})"),
    }
}

fn fetch_fixture(dir: &str, file: &str) -> Read {
    let path = format!("{dir}/{file}");
    let body = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    serde_json::from_str(&body).map_err(|e| format!("{path}: unreadable JSON: {e}"))
}

pub fn read_all(role: &str) -> Reads {
    if let Ok(dir) = std::env::var("CHORUS_SESSION_ROWS_FIXTURE_DIR") {
        return Reads {
            sessions: fetch_fixture(&dir, "sessions.json"),
            runs: fetch_fixture(&dir, "sessionruns.json"),
            priorities: fetch_fixture(&dir, "priorities.json"),
        };
    }
    let make = std::env::var("CHORUS_ATHENA_MAKE_URL").unwrap_or_else(|_| ATHENA_MAKE.to_string());
    let api = std::env::var("CHORUS_API_URL").unwrap_or_else(|_| CHORUS_API.to_string());
    Reads {
        sessions: fetch_url(&format!("{make}/v1/identity/sessions?limit=500")),
        runs: fetch_url(&format!("{make}/v1/identity/sessionruns?limit=500")),
        priorities: fetch_url(&format!("{api}/api/chorus/context/priorities?role={role}")),
    }
}

fn rows(v: &Value) -> Vec<&Value> {
    v.get("data").and_then(|d| d.as_array()).map(|a| a.iter().collect()).unwrap_or_default()
}

fn s<'a>(r: &'a Value, k: &str) -> &'a str {
    r.get(k).and_then(|x| x.as_str()).unwrap_or("")
}

/// The open Session that acts as this role, newest start first.
pub fn open_session<'a>(sessions: &'a Value, role: &str) -> Option<&'a Value> {
    let acts = format!("role-{role}");
    rows(sessions)
        .into_iter()
        .filter(|r| s(r, "sessionState") == "open" && s(r, "actsAs") == acts)
        .max_by(|a, b| s(a, "startedAt").cmp(s(b, "startedAt")))
}

/// The newest run of this role that has ended: where we left off.
pub fn last_ended_run<'a>(runs: &'a Value, role: &str) -> Option<&'a Value> {
    let prefix = format!("{role}-run-");
    rows(runs)
        .into_iter()
        .filter(|r| s(r, "name").starts_with(&prefix) && !s(r, "runEndedAt").is_empty())
        .max_by(|a, b| s(a, "runEndedAt").cmp(s(b, "runEndedAt")))
}

pub fn render(role: &str, reads: &Reads) -> String {
    let mut out = String::from("\n## This session (read from the graph)\n\n");
    match &reads.sessions {
        Err(why) => out.push_str(&format!("⚠ Session row not read: {why}\n")),
        Ok(v) => match open_session(v, role) {
            None => out.push_str(&format!(
                "No open Session row acts as role-{role}. This boot has no session to name.\n"
            )),
            Some(r) => {
                out.push_str(&format!("Session: {} · started {}\n", s(r, "name"), s(r, "startedAt")));
                let by = s(r, "attendedBy");
                if !by.is_empty() {
                    out.push_str(&format!("Attended by {by} · last {}\n", s(r, "lastAttendedAt")));
                }
            }
        },
    }
    match &reads.runs {
        Err(why) => out.push_str(&format!("⚠ SessionRun rows not read: {why}\n")),
        Ok(v) => match last_ended_run(v, role) {
            None => out.push_str("No earlier run of this role has ended: nothing to pick up from.\n"),
            Some(r) => {
                let reason = s(r, "endReason");
                out.push_str(&format!(
                    "Left off: {} ended {}{} · conversation {}\n",
                    s(r, "name"),
                    s(r, "runEndedAt"),
                    if reason.is_empty() { String::new() } else { format!(" ({reason})") },
                    s(r, "conversationId")
                ));
            }
        },
    }
    match &reads.priorities {
        Err(why) => out.push_str(&format!("⚠ Priorities walk not read: {why}\n")),
        Ok(v) => {
            let chunks = v
                .get("data")
                .and_then(|d| d.get("chunks"))
                .and_then(|c| c.as_array())
                .cloned()
                .unwrap_or_default();
            if chunks.is_empty() {
                out.push_str("Priorities: no chunks sequenced for this role.\n");
            } else {
                out.push_str("Open work (the /sup walk):\n");
                for c in chunks.iter().take(5) {
                    out.push_str(&format!("- {}\n", s(c, "chunk")));
                    for card in c.get("cards").and_then(|x| x.as_array()).into_iter().flatten().take(3) {
                        out.push_str(&format!(
                            "  - #{} {}\n",
                            card.get("id").and_then(|i| i.as_i64()).unwrap_or(0),
                            s(card, "title")
                        ));
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn reads(sessions: Read, runs: Read, priorities: Read) -> Reads {
        Reads { sessions, runs, priorities }
    }

    fn live_like() -> Reads {
        reads(
            Ok(json!({"data":[
                {"name":"wren-old","actsAs":"role-wren","sessionState":"closed","startedAt":"2026-09-20T10:00:00Z"},
                {"name":"wren-0tl9","actsAs":"role-wren","sessionState":"open","startedAt":"2026-09-26T16:28:25Z",
                 "attendedBy":"principal-jeff","lastAttendedAt":"2026-09-27T13:08:09Z"},
                {"name":"kade-hchb","actsAs":"role-kade","sessionState":"open","startedAt":"2026-09-26T16:32:05Z"}
            ]})),
            Ok(json!({"data":[
                {"name":"wren-run-1","runEndedAt":"2026-09-26T16:20:00Z","endReason":"exit","conversationId":"c-old"},
                {"name":"wren-run-2","runEndedAt":"2026-09-26T20:00:00Z","endReason":"logout","conversationId":"c-new"},
                {"name":"wren-run-3","runEndedAt":"","conversationId":"c-live"},
                {"name":"kade-run-9","runEndedAt":"2026-09-27T01:00:00Z","conversationId":"c-kade"}
            ]})),
            Ok(json!({"data":{"chunks":[
                {"chunk":"messages","cards":[{"id":4360,"title":"boot from rows"},{"id":4361,"title":"pulse from Presence"},
                                             {"id":4362,"title":"wake line waits"},{"id":4363,"title":"one id"}]}
            ]}})),
        )
    }

    #[test]
    fn names_the_open_session_that_acts_as_the_role() {
        let out = render("wren", &live_like());
        assert!(out.contains("Session: wren-0tl9 · started 2026-09-26T16:28:25Z"), "{out}");
        assert!(out.contains("Attended by principal-jeff · last 2026-09-27T13:08:09Z"), "{out}");
        assert!(!out.contains("kade-hchb"), "another role's session leaked: {out}");
    }

    #[test]
    fn left_off_is_the_newest_ended_run_of_this_role() {
        let out = render("wren", &live_like());
        assert!(out.contains("Left off: wren-run-2 ended 2026-09-26T20:00:00Z (logout) · conversation c-new"), "{out}");
        assert!(!out.contains("c-live") && !out.contains("c-kade"), "{out}");
    }

    /// NEGATIVE PROOF: the run still going (no runEndedAt) is this session,
    /// not where we left off. With only that run, there is nothing to pick up.
    #[test]
    fn a_run_still_going_is_not_where_we_left_off() {
        let r = live_like();
        let only_live = reads(r.sessions, Ok(json!({"data":[{"name":"wren-run-3","runEndedAt":"","conversationId":"c-live"}]})), r.priorities);
        let out = render("wren", &only_live);
        assert!(out.contains("No earlier run of this role has ended"), "{out}");
        assert!(!out.contains("c-live"), "{out}");
    }

    #[test]
    fn the_walk_shows_chunks_and_at_most_three_cards_each() {
        let out = render("wren", &live_like());
        assert!(out.contains("- messages\n  - #4360 boot from rows"), "{out}");
        assert!(!out.contains("#4363"), "a fourth card leaked: {out}");
    }

    /// NEGATIVE PROOF (AC4): no open Session row for the role means the boot
    /// says so and prints no Session line — it never names another session.
    #[test]
    fn no_open_session_says_so_and_invents_nothing() {
        let r = live_like();
        let none = reads(Ok(json!({"data":[{"name":"wren-old","actsAs":"role-wren","sessionState":"closed","startedAt":"x"}]})), r.runs, r.priorities);
        let out = render("wren", &none);
        assert!(out.contains("No open Session row acts as role-wren"), "{out}");
        assert!(!out.contains("Session: "), "a session line was invented: {out}");
    }

    /// AC3: a failed read names the row and the reason; the other reads still render.
    #[test]
    fn a_failed_read_names_the_row_and_why() {
        let r = live_like();
        let failed = reads(r.sessions, Err(format!("http://x/v1/identity/sessionruns {}", curl_failure(28))), r.priorities);
        let out = render("wren", &failed);
        assert!(out.contains("⚠ SessionRun rows not read: http://x/v1/identity/sessionruns timed out after 3s (curl exit 28)"), "{out}");
        assert!(out.contains("Session: wren-0tl9"), "{out}");
        assert!(out.contains("- messages"), "{out}");
    }

    /// AC5: live is checked read-only — three GETs, nothing written.
    /// `cargo test live_read_only -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_read_only() {
        let out = render("kade", &read_all("kade"));
        println!("{out}");
        assert!(!out.contains("not read"), "{out}");
    }

    #[test]
    fn curl_exits_read_as_words() {
        assert_eq!(curl_failure(7), "refused the connection (curl exit 7)");
        assert_eq!(curl_failure(22), "answered with an HTTP error (curl exit 22)");
        assert_eq!(curl_failure(28), "timed out after 3s (curl exit 28)");
        assert_eq!(curl_failure(56), "did not answer (curl exit 56)");
    }

    /// Live door that refuses: the boot line says "refused", not a bare code.
    #[test]
    fn a_refused_door_says_refused() {
        let r = fetch_url("http://127.0.0.1:9/v1/identity/sessions");
        assert!(r.as_ref().unwrap_err().contains("refused the connection"), "{r:?}");
    }

    #[test]
    fn a_fixture_dir_missing_a_file_reads_as_a_failure_not_empty() {
        let dir = std::env::temp_dir().join(format!("rows4360-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let r = fetch_fixture(dir.to_str().unwrap(), "sessions.json");
        assert!(r.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
