//! #4817 — a logged-out Claude session ran turns for two hours unseen
//! (2026-10-30 18:05–19:54, wren + silas): prompts and nudges arrived, turns
//! started and ended, and every answer was "Not logged in · Please run /login".
//! Each such turn leaves a synthetic assistant entry with
//! `"error":"authentication_failed"` in the role's transcript, and the Stop hook
//! still fires. This reads it there: every logged-out turn emits
//! `session.auth.failed`; the first one of a run alerts Jeff and the other roles
//! (data, not diagnosis), and a later real answer announces the recovery once.
//! The per-role state file under `<state>/session-auth/` is what any account
//! reads to see who is logged out.

use std::path::PathBuf;

/// A run of logged-out turns at the end of a transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct AuthFailure {
    /// timestamp of the first failed turn of the run
    pub since: String,
    /// timestamp of the newest failed turn
    pub last: String,
}

/// What this turn changes for the role.
#[derive(Debug, PartialEq)]
pub enum Transition {
    LoggedOut(AuthFailure),
    StillOut,
    Recovered { since: String },
    None,
}

/// The trailing run of `authentication_failed` assistant entries in `lines`,
/// or None when the newest assistant entry is a real answer (or there is none).
pub fn failure_in(lines: &[String]) -> Option<AuthFailure> {
    let mut run: Option<AuthFailure> = None;
    for line in lines {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if v.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        let ts = v.get("timestamp").and_then(|t| t.as_str()).unwrap_or("").to_string();
        if v.get("error").and_then(|e| e.as_str()) == Some("authentication_failed") {
            match run.as_mut() {
                Some(r) => r.last = ts,
                None => run = Some(AuthFailure { since: ts.clone(), last: ts }),
            }
        } else {
            run = None;
        }
    }
    run
}

/// `failure_in` over the transcript's tail (8 MB holds hours of turns).
pub fn failure(transcript_path: &str) -> Option<AuthFailure> {
    let mut f = std::fs::File::open(transcript_path).ok()?;
    let lines = crate::session_cache::tail_lines_within(&mut f, usize::MAX, 8 * 1024 * 1024);
    failure_in(&lines)
}

fn state_file(state_dir: &str, role: &str) -> PathBuf {
    PathBuf::from(state_dir).join("session-auth").join(format!("{role}.json"))
}

/// Compare this turn with the role's state file and update it. The file exists
/// only while the role is logged out (0644, so every account can read it).
pub fn transition(state_dir: &str, role: &str, now: Option<&AuthFailure>) -> Transition {
    let path = state_file(state_dir, role);
    let prev_since = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("since").and_then(|s| s.as_str()).map(str::to_string));
    match (now, prev_since) {
        (Some(f), prev) => {
            let body = serde_json::json!({
                "role": role, "status": "logged-out", "since": f.since, "last": f.last,
            });
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(&path, body.to_string());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
            }
            if prev.as_deref() == Some(f.since.as_str()) { Transition::StillOut } else { Transition::LoggedOut(f.clone()) }
        }
        (None, Some(since)) => {
            let _ = std::fs::remove_file(&path);
            Transition::Recovered { since }
        }
        (None, None) => Transition::None,
    }
}

/// The alert text: who, which account, since when. No cause, no fix.
pub fn alert_text(role: &str, account: &str, t: &Transition, now: &str) -> Option<String> {
    match t {
        Transition::LoggedOut(f) => Some(format!(
            "session-login: {role} logged out — account {account}, since {}, last failed turn {}",
            f.since, f.last
        )),
        Transition::Recovered { since } => {
            Some(format!("session-login: {role} logged in again — logged out {since} to {now}"))
        }
        _ => None,
    }
}

/// The agent roles, from the roles door; the five known ones when it does not answer.
pub fn agent_roles() -> Vec<String> {
    let url = std::env::var("CHORUS_ROLES_URL").unwrap_or_else(|_| "http://127.0.0.1:3360/v1/roles/roles?limit=500".into());
    let from_door = ureq::get(&url)
        .timeout(std::time::Duration::from_millis(1500))
        .call()
        .ok()
        .and_then(|r| r.into_json::<serde_json::Value>().ok())
        .and_then(|v| v.get("data").and_then(|d| d.as_array()).cloned())
        .map(|rows| {
            rows.iter()
                .filter(|r| r.get("roleKind").and_then(|k| k.as_str()) == Some("agent"))
                .filter_map(|r| r.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty());
    from_door.unwrap_or_else(|| ["wren", "silas", "kade", "borg-queen", "abby-normal"].map(String::from).to_vec())
}

/// The account a transcript belongs to: `/Users/<account>/...`.
pub fn account_of(transcript_path: &str) -> String {
    transcript_path.strip_prefix("/Users/").and_then(|r| r.split('/').next()).unwrap_or("unknown").to_string()
}

/// Who hears about it: Jeff and every other agent role.
pub fn recipients(role: &str, roles: &[&str]) -> Vec<String> {
    std::iter::once("jeff").chain(roles.iter().copied().filter(|r| *r != role)).map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(ts: &str) -> String {
        format!(r#"{{"type":"assistant","timestamp":"{ts}","message":{{"model":"claude-opus","content":[{{"type":"text","text":"ok"}}]}}}}"#)
    }
    fn out(ts: &str) -> String {
        format!(r#"{{"type":"assistant","timestamp":"{ts}","message":{{"model":"<synthetic>","content":[{{"type":"text","text":"Not logged in · Please run /login"}}]}},"error":"authentication_failed","isApiErrorMessage":true}}"#)
    }
    fn prompt(ts: &str) -> String {
        format!(r#"{{"type":"user","timestamp":"{ts}","message":{{"role":"user","content":"hi"}}}}"#)
    }
    fn tmp(tag: &str) -> String {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let d = std::env::temp_dir().join(format!("sa-4817-{tag}-{n}"));
        std::fs::create_dir_all(&d).unwrap();
        d.to_string_lossy().to_string()
    }

    #[test]
    fn a_trailing_run_of_auth_failures_is_logged_out_since_its_first_turn() {
        let lines = vec![answer("T1"), prompt("T2"), out("T3"), prompt("T4"), out("T5")];
        assert_eq!(failure_in(&lines), Some(AuthFailure { since: "T3".into(), last: "T5".into() }));
    }

    #[test]
    fn negative_proof_an_old_failure_followed_by_an_answer_is_logged_in() {
        let lines = vec![out("T1"), prompt("T2"), answer("T3")];
        assert_eq!(failure_in(&lines), None);
        assert_eq!(failure_in(&[prompt("T1")]), None, "no assistant entry is not a failure");
    }

    #[test]
    fn the_first_failed_turn_alerts_once_and_a_real_answer_announces_recovery_once() {
        let dir = tmp("trans");
        let f = AuthFailure { since: "T3".into(), last: "T3".into() };
        let t = transition(&dir, "silas", Some(&f));
        assert_eq!(t, Transition::LoggedOut(f.clone()));
        let text = alert_text("silas", "chorus-silas", &t, "T9").unwrap();
        assert_eq!(text, "session-login: silas logged out — account chorus-silas, since T3, last failed turn T3");
        let path = state_file(&dir, "silas");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o644, "every account reads it");

        let later = AuthFailure { since: "T3".into(), last: "T5".into() };
        assert_eq!(transition(&dir, "silas", Some(&later)), Transition::StillOut);
        assert_eq!(alert_text("silas", "chorus-silas", &Transition::StillOut, "T9"), None, "no repeat alert");

        let back = transition(&dir, "silas", None);
        assert_eq!(back, Transition::Recovered { since: "T3".into() });
        assert!(!path.exists(), "the state file goes when the role answers again");
        assert_eq!(transition(&dir, "silas", None), Transition::None, "recovery is announced once");
    }

    #[test]
    fn negative_proof_a_logged_in_role_never_alerts() {
        let dir = tmp("ok");
        let t = transition(&dir, "wren", failure_in(&[answer("T1")]).as_ref());
        assert_eq!(t, Transition::None);
        assert_eq!(alert_text("wren", "chorus-wren", &t, "T2"), None);
    }

    #[test]
    fn jeff_and_the_other_roles_hear_it_never_the_logged_out_role() {
        let r = recipients("silas", &["wren", "silas", "kade"]);
        assert_eq!(r, vec!["jeff", "wren", "kade"]);
    }

    #[test]
    fn the_alert_reports_data_not_diagnosis() {
        let t = Transition::LoggedOut(AuthFailure { since: "T1".into(), last: "T2".into() });
        let text = alert_text("wren", "chorus-wren", &t, "T3").unwrap().to_lowercase();
        for w in ["because", "probably", "restart", "/login", "please"] {
            assert!(!text.contains(w), "diagnosis word {w:?} in {text}");
        }
    }

    #[test]
    fn the_account_is_the_transcripts_home() {
        assert_eq!(account_of("/Users/chorus-wren/.claude/projects/x/s.jsonl"), "chorus-wren");
        assert_eq!(account_of("/tmp/s.jsonl"), "unknown");
    }

    #[test]
    fn failure_reads_the_transcript_file() {
        let dir = tmp("file");
        let p = format!("{dir}/s.jsonl");
        std::fs::write(&p, [answer("T1"), prompt("T2"), out("T3")].join("\n")).unwrap();
        assert_eq!(failure(&p), Some(AuthFailure { since: "T3".into(), last: "T3".into() }));
        assert_eq!(failure(&format!("{dir}/missing.jsonl")), None);
    }
}
