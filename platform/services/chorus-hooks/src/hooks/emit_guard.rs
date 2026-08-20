// #2645 — the outbound twin of write_scrubber.
//
// write_scrubber stops a secret being WRITTEN to a shared file. Nothing stopped
// one being SAID: a token pasted into a nudge, a chat message, a card comment,
// or a commit message left by a door with no guard, and the receiver rendered
// it straight into their context. Proven live 2026-08-16 when a room secret
// reached a session transcript with no refusal anywhere on the path.
//
// This hook scans EMIT-shaped tool calls before they run. Detection reuses the
// #3387/#3940 log_redact signatures (one signature set, two doors) plus the
// AC1 additions that only make sense outbound (ssh-key paths, env-var dumps).

use crate::shared::log_redact;
use regex::Regex;
use std::sync::LazyLock;

// AC1 additions beyond log_redact's set.
static SSH_PATH_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(~|/Users/[^/\s]+)/\.ssh/[^\s"']+"#).unwrap());
static ENV_DUMP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\b[A-Z][A-Z0-9_]{2,}=[A-Za-z0-9+/._-]{32,}"#).unwrap());

/// Which tool inputs count as EMITTED text, per tool. Pure — testable alone.
pub fn emitted_text(tool_name: &str, command: &str, message: &str) -> Option<String> {
    if tool_name == "mcp__chorus-api__chorus_nudge_message" && !message.is_empty() {
        return Some(message.to_string());
    }
    if tool_name == "Bash" {
        let emitty = ["chat.sh say", "cards comment", "git commit", "cards add", "cards update"];
        if emitty.iter().any(|e| command.contains(e)) {
            return Some(command.to_string());
        }
    }
    None
}

/// Does this outbound text carry a credential? Name the pattern for the refusal.
pub fn detect(text: &str) -> Option<&'static str> {
    if log_redact::would_redact(text) {
        return Some("credential pattern (curl-auth / Bearer / key=value / token shape)");
    }
    if SSH_PATH_RE.is_match(text) {
        return Some("ssh private-key path");
    }
    if ENV_DUMP_RE.is_match(text) {
        return Some("env-var with long value (possible secret dump)");
    }
    None
}

/// AC3 — session-scoped bypass, ALWAYS audited. No silent allow.
pub fn bypass_allows(text: &str, allow_pattern: Option<&str>) -> bool {
    match allow_pattern {
        Some(p) if !p.is_empty() => Regex::new(p).map(|re| re.is_match(text)).unwrap_or(false),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(prefix: &str) -> String {
        format!("{prefix}{}{}", "abcdefghijklmnopqrstuvwx", "yz1234567890")
    }

    /// AC1/AC2 positive cases — each signature refuses on an emit surface.
    #[test]
    fn each_signature_triggers_on_emitted_text() {
        let gh = tok("ghp_");
        for text in [
            format!("nudge body with {gh}"),
            {
                let ak = ["AK", "IA"].concat();
                format!("git commit -m 'add key {}{}'", ak, tok("BCDEFGHIJKLMNOP="))
            },
            "see ~/.ssh/id_ed25519 for the key".to_string(),
            format!("SECRET_THING={}", tok("x")),
        ] {
            assert!(detect(&text).is_some(), "must detect: {text}");
        }
    }

    /// NEGATIVE (#3734): ordinary team traffic passes. A guard that refuses
    /// normal nudges gets bypassed within a day, and bypassed is worse than absent.
    #[test]
    fn ordinary_traffic_is_not_refused() {
        for text in [
            "Jeff says pull next in sequence; #2645 refused at the door",
            "git commit -m 'silas: #2645: emit guard'",
            "keyId is an ENV-VAR NAME — SILAS_NOSTR_KEY",
            "cards comment 2645 'AC1 done, patterns in emit_guard.rs'",
            "curl -sf http://localhost:3340/api/chorus/health",
        ] {
            assert!(detect(text).is_none(), "false positive on: {text}");
        }
    }

    /// Only emit-shaped calls are scanned — a grep mentioning a token shape in
    /// a search pattern is NOT an emission.
    #[test]
    fn non_emit_tools_are_out_of_scope() {
        assert!(emitted_text("Bash", "grep -rn 'ghp_' src/", "").is_none());
        assert!(emitted_text("Read", "", "").is_none());
        assert!(emitted_text("mcp__chorus-api__chorus_nudge_message", "", "hello").is_some());
    }

    /// AC3 — bypass is scoped to the matching pattern, nothing wider.
    #[test]
    fn bypass_is_narrow() {
        let text = format!("rotate {}", tok("ghp_"));
        assert!(bypass_allows(&text, Some("ghp_")));
        assert!(!bypass_allows(&text, Some("AKIA")));
        assert!(!bypass_allows(&text, None));
        assert!(!bypass_allows(&text, Some("")));
    }
}
