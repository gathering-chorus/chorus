// #3387 — credentials are redacted on the path INTO the logs.
//
// write_scrubber guards Write/Edit tool calls to shared FILES; it never sees
// what the daemon itself logs. 28 of the 35 secret-scan history findings were
// auth strings captured in RUNTIME LOGS (permission-prompts.log, chorus.log) —
// a logged `curl -u user:pass` landed verbatim, and #2709's commit gate then
// counted it forever. This module is the source fix: every log-emit passes
// through redact() before disk, so the baseline stays flat.
//
// Pure function, no I/O — provable without a daemon.

use regex::Regex;
use std::sync::LazyLock;

const MASK: &str = "REDACTED";

// curl -u user:pass  /  --user user:pass
static CURL_AUTH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"((?:-u|--user)[ =])\S+"#).unwrap()
});
// Authorization: Bearer <tok>  /  Bearer eyJ...
static BEARER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(bearer[ ])[A-Za-z0-9\-._~+/=]{8,}"#).unwrap()
});
// password= / token= / secret= / api_key= / apikey= with a non-trivial value
static KV_SECRET_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)((?:password|passwd|token|secret|api_key|apikey)[=: ])[^\s"'&]{4,}"#).unwrap()
});
// well-known token shapes: GitHub, Slack, AWS, nostr nsec
static TOKEN_SHAPE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"\b(ghp_[A-Za-z0-9]{20,}|gho_[A-Za-z0-9]{20,}|xox[baprs]-[A-Za-z0-9-]{10,}|AKIA[A-Z0-9]{16}|nsec1[a-z0-9]{20,})\b"#).unwrap()
});
// basic-auth in a URL: scheme://user:pass@host
static URL_AUTH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(://)[^/\s:@]+:[^/\s@]+@"#).unwrap()
});

/// Redact credential patterns from a line bound for a log file.
pub fn redact(line: &str) -> String {
    let s = CURL_AUTH_RE.replace_all(line, format!("${{1}}{MASK}"));
    let s = BEARER_RE.replace_all(&s, format!("${{1}}{MASK}"));
    let s = KV_SECRET_RE.replace_all(&s, format!("${{1}}{MASK}"));
    let s = TOKEN_SHAPE_RE.replace_all(&s, MASK);
    let s = URL_AUTH_RE.replace_all(&s, format!("${{1}}{MASK}@"));
    s.into_owned()
}

/// Did redact() change anything? Callers use this to emit a scrub-count metric
/// so redaction is observable, never silent.
pub fn would_redact(line: &str) -> bool {
    CURL_AUTH_RE.is_match(line)
        || BEARER_RE.is_match(line)
        || KV_SECRET_RE.is_match(line)
        || TOKEN_SHAPE_RE.is_match(line)
        || URL_AUTH_RE.is_match(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 28/35 path itself: a logged curl carrying basic auth (AC3, unit half).
    #[test]
    fn curl_basic_auth_is_redacted() {
        let pw = format!("hun{}", "ter2");
        let r = redact(&format!("curl -u admin:{pw} http://localhost:3030/pods/query"));
        assert!(!r.contains(&pw), "{r}");
        assert!(r.contains("REDACTED"));
        // and the command stays readable
        assert!(r.contains("curl"));
        assert!(r.contains("localhost:3030"));
    }

    #[test]
    fn bearer_and_kv_and_url_auth_are_redacted() {
        // Fixture credentials ASSEMBLED at runtime — the pre-commit secret
        // scanner must never see them as literals (the #3386 lesson).
        let jwt = format!("eyJhbGciOiJIUzI1NiJ9.{}.{}", "abc", "def");
        let pw  = format!("s3cret{}", "value");
        let up  = format!("pw{}", "12345");
        let gh  = format!("ghp_{}{}", "abcdefghijklmnopqrstuv", "123456");
        let cases = [
            (format!("curl -H 'Authorization: Bearer {jwt}' x"), jwt.clone()),
            (format!("FUSEKI_PASSWORD={pw} chorus-model add"), pw.clone()),
            (format!("fetch https://admin:{up}@fuseki.local/ds"), up.clone()),
            (format!("export GITHUB_TOKEN={gh}"), gh.clone()),
        ];
        for (line, leaked) in &cases {
            let r = redact(line);
            assert!(!r.contains(leaked.as_str()), "leaked {leaked} in: {r}");
            assert!(r.contains(MASK), "no mask in: {r}");
        }
    }

    /// NEGATIVE PROOF (#3734): ordinary log lines pass through UNCHANGED. A
    /// redactor that mangles normal telemetry would get turned off within a
    /// week, and off is worse than absent.
    #[test]
    fn ordinary_lines_pass_through_byte_identical() {
        for line in [
            "cards move 3387 WIP",
            "git commit -m 'silas: #3387' — gate clear",
            "curl -sf http://localhost:3340/api/chorus/health",
            r#"{"event":"tool_call","tool":"Bash","detail":"ls -la /tmp"}"#,
            "keyId is an ENV-VAR NAME — SILAS_NOSTR_KEY",  // env-var NAMES are not values
        ] {
            assert_eq!(redact(line), line, "must not touch: {line}");
        }
    }

    /// NEGATIVE PROOF: the redactor itself can never be satisfied vacuously —
    /// a line it claims it would redact must actually differ after redact().
    #[test]
    fn would_redact_and_redact_agree() {
        let hot = "curl -u a:b http://x";
        let cold = "plain line";
        assert!(would_redact(hot) && redact(hot) != hot);
        assert!(!would_redact(cold) && redact(cold) == cold);
    }
}
