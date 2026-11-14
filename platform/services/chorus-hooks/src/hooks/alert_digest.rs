//! #4957 AC2 — a role's open alert cases, shown in its own context on its next turn.
//! The 5-minute alert-digest schedule writes `<dir>/<role>.md`; this reads it. Nothing
//! is typed into a pane (Jeff 2026-11-11 09:50: "if the alerts come to a queue and you
//! check alerts every five minutes via dagu… I could accept that").
//!
//! A digest with no open cases adds nothing. A digest older than three schedule passes
//! is still shown, marked stale, so a dead schedule never reads as "nothing is wrong";
//! a missing digest says so for the same reason.
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Three missed 5-minute passes.
const STALE_AFTER: Duration = Duration::from_secs(15 * 60);

/// The digest directory: ALERT_DIGEST_DIR, else `alerts` under the team's shared
/// state dir (#4535: a role on its own account finds the team's digests).
pub fn digest_dir() -> std::path::PathBuf {
    std::env::var("ALERT_DIGEST_DIR").map(Into::into).unwrap_or_else(|_| {
        Path::new(&crate::shared::state_paths::chorus_state_dir()).join("alerts")
    })
}

/// `envelope` plus an "Open alert cases" block for `role`, read from `dir` as of `now`.
pub fn augment_envelope_with_alert_digest(role: &str, envelope: &str, dir: &Path, now: SystemTime) -> String {
    let path = dir.join(format!("{role}.md"));
    let Ok(text) = std::fs::read_to_string(&path) else {
        return format!("{envelope}\n\n## Open alert cases\nno alert digest at {} — the alert-digest schedule has not written one\n", path.display());
    };
    let cases: Vec<&str> = text.lines().filter(|l| l.starts_with("- ")).collect();
    let age = std::fs::metadata(&path).and_then(|m| m.modified()).ok()
        .and_then(|m| now.duration_since(m).ok()).unwrap_or_default();
    let stale = if age > STALE_AFTER {
        format!(" — stale: the schedule last wrote this {} min ago", age.as_secs() / 60)
    } else {
        String::new()
    };
    // an empty digest is "nothing is wrong" only while the schedule keeps writing it
    if cases.is_empty() {
        return if stale.is_empty() { envelope.to_string() } else { format!("{envelope}\n\n## Open alert cases (0){stale}\n") };
    }
    format!("{envelope}\n\n## Open alert cases ({}){stale}\n{}\n", cases.len(), cases.join("\n"))
}
