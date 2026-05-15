//! Card-approval responder — #2924 AC3/AC4.
//!
//! UserPromptSubmit hook that closes the bouncer-approval loop:
//!
//! 1. Bouncer in `directing/products/cards/src/sdk.ts` refuses an agent
//!    `cards add` and writes a pending payload at
//!    `~/.chorus/pending-approvals/<role>-<stamp>.argv.json`.
//! 2. This hook scans Jeff's prompts for `approve` / `deny` keywords.
//! 3. On `approve` → reads the most-recent pending payload for the role,
//!    replays `cards add` with `DEPLOY_ROLE=jeff` (bypasses the bouncer),
//!    emits `card.approval.granted`, cleans up the pending files.
//! 4. On `deny` → cleans up the pending files, emits
//!    `card.approval.denied`.
//! 5. Pending files older than 10 min are treated as timed-out — ignored
//!    on approve/deny match, swept on every invocation. Emits
//!    `card.approval.timeout` on sweep.
//!
//! This commit lands the keyword detector in isolation. File-scan, replay,
//! and main.rs wiring follow in subsequent commits — deploy of the wired
//! version is gated on Silas's #2925 (daemon-runtime deploy path).

use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ApprovalSignal {
    Approve,
    Deny,
}

/// Standalone-word match for `approve` / `approved`.
static APPROVE_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|\s)(?:approve|approved)(?:\s|$|[.,!?])").unwrap()
});

/// Standalone-word match for `deny` / `denied`.
static DENY_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|\s)(?:deny|denied)(?:\s|$|[.,!?])").unwrap()
});

/// Detect an approval/denial signal in a UserPromptSubmit prompt.
///
/// Guards:
/// - Long prompts (>200 chars) are ignored — `approve` / `deny` in a
///   paragraph is discussion, not signal. Mirrors the jdi_detector guard.
/// - Relayed nudge content (`[nudge from`) is ignored — injected messages,
///   not Jeff typing.
/// - Quoted/template content (`<…>`) is ignored.
/// - When BOTH `approve` and `deny` appear, returns `None` — ambiguous.
pub fn detect_approval_signal(prompt: &str) -> Option<ApprovalSignal> {
    if prompt.is_empty() || prompt.len() > 200 {
        return None;
    }
    let trimmed = prompt.trim();
    if trimmed.starts_with("[nudge from") || trimmed.starts_with('<') {
        return None;
    }
    let has_approve = APPROVE_PATTERN.is_match(prompt);
    let has_deny = DENY_PATTERN.is_match(prompt);
    match (has_approve, has_deny) {
        (true, false) => Some(ApprovalSignal::Approve),
        (false, true) => Some(ApprovalSignal::Deny),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approve_standalone() {
        assert_eq!(detect_approval_signal("approve"), Some(ApprovalSignal::Approve));
        assert_eq!(detect_approval_signal("Approve"), Some(ApprovalSignal::Approve));
        assert_eq!(detect_approval_signal("approve!"), Some(ApprovalSignal::Approve));
        assert_eq!(detect_approval_signal("ok approve"), Some(ApprovalSignal::Approve));
        assert_eq!(detect_approval_signal("approve."), Some(ApprovalSignal::Approve));
    }

    #[test]
    fn approved_form() {
        assert_eq!(detect_approval_signal("approved"), Some(ApprovalSignal::Approve));
        assert_eq!(detect_approval_signal("approved!"), Some(ApprovalSignal::Approve));
    }

    #[test]
    fn deny_standalone() {
        assert_eq!(detect_approval_signal("deny"), Some(ApprovalSignal::Deny));
        assert_eq!(detect_approval_signal("Deny"), Some(ApprovalSignal::Deny));
        assert_eq!(detect_approval_signal("no deny"), Some(ApprovalSignal::Deny));
        assert_eq!(detect_approval_signal("denied"), Some(ApprovalSignal::Deny));
    }

    #[test]
    fn no_match_on_empty() {
        assert_eq!(detect_approval_signal(""), None);
    }

    #[test]
    fn no_match_on_embedded_words() {
        // "approval" / "disapprove" / "approver" should NOT trigger
        assert_eq!(detect_approval_signal("approval flow looks good"), None);
        assert_eq!(detect_approval_signal("disapprove"), None);
        assert_eq!(detect_approval_signal("approver"), None);
        assert_eq!(detect_approval_signal("denying"), None);
    }

    #[test]
    fn no_match_on_long_prompts() {
        // 250 chars containing approve — treated as discussion, not signal
        let long = format!("{} approve {}", "x".repeat(120), "y".repeat(120));
        assert!(long.len() > 200);
        assert_eq!(detect_approval_signal(&long), None);
    }

    #[test]
    fn no_match_on_relayed_nudge() {
        assert_eq!(
            detect_approval_signal("[nudge from silas | 2026-05-15 10:00] approve please"),
            None
        );
    }

    #[test]
    fn no_match_on_quoted_content() {
        assert_eq!(detect_approval_signal("<system-reminder>approve</system-reminder>"), None);
    }

    #[test]
    fn no_match_when_both_appear() {
        // Ambiguous — returns None rather than guessing
        assert_eq!(detect_approval_signal("approve or deny"), None);
        assert_eq!(detect_approval_signal("deny? approve?"), None);
    }

    #[test]
    fn whitespace_only_no_match() {
        assert_eq!(detect_approval_signal("   "), None);
        assert_eq!(detect_approval_signal("\n\n"), None);
    }
}
