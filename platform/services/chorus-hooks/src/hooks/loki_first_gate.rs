//! Loki-first gate (#4149)
//!
//! PreToolUse on Bash: refuses a text read of a LIVE log file and hands back
//! the LogQL that answers the same question.
//!
//! Why a deny and not a rule. "Loki first" has been in every role's CLAUDE.md
//! for months, and Jeff has said it three times — most recently "i would fire
//! any operations who ignored splunk". The measured result over the seven days
//! to 2026-09-12: 44 Loki queries against 369 greps of the flat log. A rule a
//! role can read and skip is not a control; the refusal is the control.
//!
//! It is deliberately narrow. Only the live spine and service logs are
//! protected — the files Loki actually indexes, where a grep gives a worse
//! answer than a query (no labels, no time window, one machine, 2 GB of it).
//! A log in /tmp, a scratch tree, a werk, or a test fixture is untouched: those
//! are not in Loki, so there is nothing better to send the caller to.

use crate::types::{permission_deny_json, HookInput, HookResponse};
use regex::Regex;
use std::sync::LazyLock;

/// Commands that read a file as text. `wc`, `ls`, `stat`, `rm` and friends are
/// absent on purpose: asking how big chorus.log is, or deleting a rotation, is
/// not the investigation this gate is about.
static READ_VERB_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[|;&]\s*|\s)(grep|egrep|fgrep|rg|ag|tail|head|cat|less|more|awk|sed)\s").unwrap()
});

/// The live logs Loki indexes. Each maps to the label that replaces it.
const PROTECTED: &[(&str, &str)] = &[
    (".chorus/chorus.log", "{job=\"platform-chorus\"}"),
    ("platform/logs/chorus.log", "{job=\"platform-chorus\"}"),
    (".chorus/hooks.log", "{job=\"chorus-hooks\"}"),
    ("platform/logs/hooks.log", "{job=\"chorus-hooks\"}"),
    ("Library/Logs/Chorus/chorus-api.log", "{job=\"chorus-api\"}"),
    ("Library/Logs/Chorus/deep-health.log", "{job=\"deep-health\"}"),
    ("Library/Logs/Chorus/heartbeat", "{job=\"heartbeat\"}"),
];

/// Paths where a same-named log is NOT the live one — a copy in a scratch
/// tree, a werk, a fixture. Nothing in Loki covers these, so let them through.
const NOT_LIVE: &[&str] = &[
    "/tmp/",
    "/var/folders/",
    "claude-501/",
    "chorus-werk/",
    "fixtures/",
    "testdata/",
];

/// The one thing a caller can say to mean "I know, and I want the file": a
/// deliberate byte-level read (checksum, rotation size, corruption check).
/// It must be explicit, because the point is that skipping is a choice made
/// out loud rather than by habit.
const OVERRIDE_MARKER: &str = "LOKI_FIRST_OVERRIDE";

/// What the gate decided, and why. Pure — this is the whole testable surface.
#[derive(Debug, PartialEq)]
pub enum Verdict {
    Allow,
    /// (the protected path that was matched, the LogQL to run instead)
    Deny(String, String),
}

pub fn classify(command: &str) -> Verdict {
    if command.is_empty() || command.contains(OVERRIDE_MARKER) {
        return Verdict::Allow;
    }
    // Writing a log line is not reading one.
    if command.contains("chorus-log ") || command.contains(">> ") || command.contains("> /") {
        return Verdict::Allow;
    }
    if !READ_VERB_RE.is_match(command) {
        return Verdict::Allow;
    }
    if NOT_LIVE.iter().any(|p| command.contains(p)) {
        return Verdict::Allow;
    }
    for (path, logql) in PROTECTED {
        if command.contains(path) {
            return Verdict::Deny((*path).to_string(), (*logql).to_string());
        }
    }
    Verdict::Allow
}

/// The refusal text. Names the file, the query, and the tool that runs it —
/// a refusal that does not say what to do instead is just an obstacle.
pub fn refusal(path: &str, logql: &str) -> String {
    format!(
        "Loki-first (#4149): reading {} as text is refused. Ask Loki instead — \
         it has the labels, a time window, and every machine; the flat file has none of that.\n\
         \n\
         Named question?  chorus_logs_for_card / _for_trace / _for_branch / _recent_errors\n\
         Raw LogQL?       the grafana MCP server: query_loki_logs  {}\n\
         Unknown labels?  list_loki_label_names, then list_loki_label_values\n\
         \n\
         If you truly need the bytes (checksum, rotation size, corruption), say so by \
         putting {} in the command.",
        path, logql, OVERRIDE_MARKER
    )
}

pub fn check(input: &HookInput) -> HookResponse {
    if input.tool_name_str() != "Bash" {
        return HookResponse::allow();
    }
    match classify(&input.get_tool_input_str("command")) {
        Verdict::Allow => HookResponse::allow(),
        Verdict::Deny(path, logql) => {
            HookResponse::deny(&permission_deny_json(&refusal(&path, &logql)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn denied(cmd: &str) -> bool {
        matches!(classify(cmd), Verdict::Deny(_, _))
    }

    #[test]
    fn negative_proof_the_exact_greps_we_actually_ran_are_refused() {
        // Every one of these is a command from a real session. If the gate
        // cannot go red on these it cannot go red at all.
        assert!(denied("grep 'nudge.surfaced' ~/.chorus/chorus.log | tail -5"));
        assert!(denied("tail -200 ~/.chorus/chorus.log"));
        assert!(denied("rg 'test.failed' /Users/jeffbridwell/CascadeProjects/chorus/platform/logs/chorus.log"));
        assert!(denied("cat ~/Library/Logs/Chorus/chorus-api.log"));
        assert!(denied("awk '/werk.phase/' ~/.chorus/hooks.log"));
    }

    #[test]
    fn the_refusal_names_the_file_the_query_and_the_way_out() {
        let Verdict::Deny(path, logql) = classify("grep x ~/.chorus/chorus.log") else {
            panic!("expected a denial");
        };
        let msg = refusal(&path, &logql);
        assert!(msg.contains("chorus.log"));
        assert!(msg.contains("{job=\"platform-chorus\"}"));
        assert!(msg.contains("query_loki_logs"));
        assert!(msg.contains(OVERRIDE_MARKER));
    }

    #[test]
    fn a_copy_outside_loki_is_not_the_live_log() {
        assert!(!denied("grep boot /tmp/chorus.log"));
        assert!(!denied("tail /var/folders/4j/xyz/chorus.log"));
        assert!(!denied("grep x ~/CascadeProjects/chorus-werk/silas-4149/platform/logs/chorus.log"));
    }

    #[test]
    fn asking_about_the_file_rather_than_its_contents_is_fine() {
        assert!(!denied("wc -l ~/.chorus/chorus.log"));
        assert!(!denied("ls -l ~/.chorus/chorus.log"));
        assert!(!denied("du -h ~/.chorus/chorus.log"));
    }

    #[test]
    fn writing_the_spine_is_not_reading_it() {
        assert!(!denied("bash platform/scripts/chorus-log test.started silas card=4149"));
        assert!(!denied("echo '{}' >> ~/.chorus/chorus.log"));
    }

    #[test]
    fn the_override_is_honored_and_must_be_said_out_loud() {
        assert!(denied("cat ~/.chorus/chorus.log"));
        assert!(!denied("cat ~/.chorus/chorus.log # LOKI_FIRST_OVERRIDE rotation size check"));
    }

    #[test]
    fn unrelated_greps_are_untouched() {
        assert!(!denied("grep -rn 'fn check' platform/services/chorus-hooks/src"));
        assert!(!denied("tail -20 platform/api/src/server.ts"));
        assert!(!denied(""));
    }
}
