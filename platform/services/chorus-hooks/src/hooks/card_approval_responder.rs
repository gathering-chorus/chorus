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
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::{Duration, SystemTime};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ApprovalSignal {
    Approve,
    Deny,
}

/// Pending files older than this are treated as timed-out — ignored on
/// match, swept on every invocation. AC4 default; configurable later via
/// CHORUS_APPROVAL_TIMEOUT_SECS env if needed.
pub const PENDING_TIMEOUT_SECS: u64 = 600; // 10 minutes

/// Structured payload the bouncer wrote — read back for replay.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PendingPayload {
    pub title: String,
    pub opts: PendingOpts,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PendingOpts {
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default, rename = "type")]
    pub card_type: Option<String>,
    #[serde(default)]
    pub origin: Option<String>,
    #[serde(default)]
    pub sequence: Option<String>,
    #[serde(default)]
    pub subproduct: Option<String>,
    #[serde(default)]
    pub subdomain: Option<String>,
    #[serde(default)]
    pub chunk: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Find the most-recently-modified `<role>-*.argv.json` file under `pending_dir`.
/// Returns `None` if the directory doesn't exist or has no matching files.
pub fn find_most_recent_pending(role: &str, pending_dir: &Path) -> Option<PathBuf> {
    let prefix = format!("{}-", role);
    let entries = std::fs::read_dir(pending_dir).ok()?;
    let mut best: Option<(PathBuf, SystemTime)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.starts_with(&prefix) || !name.ends_with(".argv.json") {
            continue;
        }
        let mtime = match entry.metadata().and_then(|m| m.modified()) {
            Ok(t) => t,
            Err(_) => continue,
        };
        match best {
            None => best = Some((path, mtime)),
            Some((_, prev_mtime)) if mtime > prev_mtime => best = Some((path, mtime)),
            _ => {}
        }
    }
    best.map(|(p, _)| p)
}

/// True if the file's mtime is older than `PENDING_TIMEOUT_SECS` relative to `now`.
pub fn is_stale(path: &Path, now: SystemTime) -> bool {
    let mtime = match std::fs::metadata(path).and_then(|m| m.modified()) {
        Ok(t) => t,
        Err(_) => return true, // missing/unreadable counts as stale
    };
    match now.duration_since(mtime) {
        Ok(age) => age > Duration::from_secs(PENDING_TIMEOUT_SECS),
        Err(_) => false, // mtime in future — not stale
    }
}

/// Build the cards-CLI argv list for a pending payload (without the binary path).
/// Excludes the description — that ships via `--desc-file` written separately.
pub fn build_cards_add_argv(payload: &PendingPayload) -> Vec<String> {
    let mut argv: Vec<String> = vec![payload.title.clone()];
    let opts = &payload.opts;
    let pairs: [(&str, &Option<String>); 9] = [
        ("--owner", &opts.owner),
        ("--priority", &opts.priority),
        ("--domain", &opts.domain),
        ("--type", &opts.card_type),
        ("--origin", &opts.origin),
        ("--sequence", &opts.sequence),
        ("--subproduct", &opts.subproduct),
        ("--subdomain", &opts.subdomain),
        ("--chunk", &opts.chunk),
    ];
    for (flag, value) in pairs.iter() {
        if let Some(v) = value {
            argv.push((*flag).to_string());
            argv.push(v.clone());
        }
    }
    argv
}

/// Sweep stale pending files (mtime > PENDING_TIMEOUT_SECS old). For each
/// stale `*.argv.json` removes both the `.argv.json` and its sibling `.txt`.
/// Returns the basenames of swept files (for spine events / logging).
pub fn sweep_stale_pending(pending_dir: &Path, now: SystemTime) -> Vec<String> {
    let mut swept = Vec::new();
    let entries = match std::fs::read_dir(pending_dir) {
        Ok(e) => e,
        Err(_) => return swept,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        if !name.ends_with(".argv.json") {
            continue;
        }
        if !is_stale(&path, now) {
            continue;
        }
        let stem = name.trim_end_matches(".argv.json");
        let txt_path = pending_dir.join(format!("{}.txt", stem));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&txt_path);
        swept.push(stem.to_string());
    }
    swept
}

/// Resolve the cards CLI absolute path. Reads `CHORUS_CARDS_BIN` env var
/// (testable via `resolve_cards_cli_path_with_env` for an alternate name);
/// falls back to the canonical absolute path because launchctl-managed
/// processes (the chorus-hooks daemon) inherit a minimal PATH that does
/// NOT include the chorus platform/scripts directory. Production tonight
/// hit this as a `spawn-failed` outcome on the first live replay — the
/// fallback closes that path-boundary gap.
pub fn resolve_cards_cli_path() -> String {
    resolve_cards_cli_path_with_env("CHORUS_CARDS_BIN")
}

/// Internal helper that accepts the env var name so tests can use isolated
/// names without trampling production config.
pub fn resolve_cards_cli_path_with_env(env_name: &str) -> String {
    std::env::var(env_name).unwrap_or_else(|_| {
        "/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/cards".to_string()
    })
}

/// Remove a pending pair (.argv.json + .txt) given the .argv.json path.
/// Best-effort — missing siblings are not errors.
pub fn remove_pending_pair(argv_path: &Path) {
    let _ = std::fs::remove_file(argv_path);
    if let Some(stem) = argv_path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.trim_end_matches(".argv.json").to_string())
    {
        if let Some(parent) = argv_path.parent() {
            let _ = std::fs::remove_file(parent.join(format!("{}.txt", stem)));
        }
    }
}

/// Outcome of an approval-signal handling pass — discriminator for the
/// caller (main.rs) to decide which spine event to emit and how to log.
#[derive(Debug, PartialEq, Eq)]
pub enum ApprovalOutcome {
    Approved { title: String },
    Denied { title: String },
    TimedOut,
    NoPending,
    ReadFailed,
    ParseFailed,
    SpawnFailed,
}

/// Orchestrate the approve/deny pass on the most-recent pending request for
/// a role. Pure relative to the injected `spawn_cards` closure — tests pass
/// a mock; production passes a closure that invokes the cards CLI with
/// DEPLOY_ROLE=jeff (bypasses the bouncer cleanly per #2905).
///
/// `spawn_cards(desc_path, argv) -> Ok(true)` means the card was filed
/// successfully. The closure owns the actual subprocess invocation so this
/// function stays test-able without a live cards CLI.
pub fn handle_approval_request<F>(
    role: &str,
    signal: ApprovalSignal,
    pending_dir: &Path,
    now: SystemTime,
    spawn_cards: F,
) -> ApprovalOutcome
where
    F: FnOnce(&Path, Vec<String>) -> std::io::Result<bool>,
{
    let pending_path = match find_most_recent_pending(role, pending_dir) {
        Some(p) => p,
        None => return ApprovalOutcome::NoPending,
    };
    if is_stale(&pending_path, now) {
        remove_pending_pair(&pending_path);
        return ApprovalOutcome::TimedOut;
    }
    let payload_str = match std::fs::read_to_string(&pending_path) {
        Ok(s) => s,
        Err(_) => return ApprovalOutcome::ReadFailed,
    };
    let payload: PendingPayload = match serde_json::from_str(&payload_str) {
        Ok(p) => p,
        Err(_) => return ApprovalOutcome::ParseFailed,
    };
    match signal {
        ApprovalSignal::Deny => {
            let title = payload.title.clone();
            remove_pending_pair(&pending_path);
            ApprovalOutcome::Denied { title }
        }
        ApprovalSignal::Approve => {
            let desc = payload.opts.description.clone().unwrap_or_default();
            let desc_path = std::env::temp_dir().join(format!(
                "card-approval-{}-{}.md",
                std::process::id(),
                now.duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0),
            ));
            if std::fs::write(&desc_path, &desc).is_err() {
                return ApprovalOutcome::ReadFailed;
            }
            let argv = build_cards_add_argv(&payload);
            let result = spawn_cards(&desc_path, argv);
            let _ = std::fs::remove_file(&desc_path);
            match result {
                Ok(true) => {
                    let title = payload.title.clone();
                    remove_pending_pair(&pending_path);
                    ApprovalOutcome::Approved { title }
                }
                _ => ApprovalOutcome::SpawnFailed,
            }
        }
    }
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

    // ---- file-handling tests (AC3/AC4) ----

    use std::fs;
    use std::time::{Duration, SystemTime};
    use tempfile::TempDir;

    fn write_pending(dir: &Path, role: &str, stamp: &str, argv_body: &str) -> PathBuf {
        let argv = dir.join(format!("{}-{}.argv.json", role, stamp));
        let txt = dir.join(format!("{}-{}.txt", role, stamp));
        fs::write(&argv, argv_body).unwrap();
        fs::write(&txt, "stub").unwrap();
        argv
    }

    fn set_mtime(path: &Path, age_secs: u64) {
        // Use filetime to set the mtime to N seconds ago — but filetime isn't
        // in deps. Workaround: write the file, then read it back has current
        // mtime. For "stale" tests, sleep is too slow. Instead, write the file
        // then use std::fs::set_modified (stable in 1.80+).
        let target = SystemTime::now() - Duration::from_secs(age_secs);
        let file = fs::OpenOptions::new().write(true).open(path).unwrap();
        file.set_modified(target).unwrap();
    }

    #[test]
    fn find_most_recent_pending_returns_none_when_dir_missing() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("does-not-exist");
        assert!(find_most_recent_pending("wren", &missing).is_none());
    }

    #[test]
    fn find_most_recent_pending_returns_none_when_no_match() {
        let tmp = TempDir::new().unwrap();
        write_pending(tmp.path(), "silas", "x", "{}");
        write_pending(tmp.path(), "kade", "y", "{}");
        assert!(find_most_recent_pending("wren", tmp.path()).is_none());
    }

    #[test]
    fn find_most_recent_pending_picks_newest_mtime_for_role() {
        let tmp = TempDir::new().unwrap();
        let older = write_pending(tmp.path(), "wren", "a", "{}");
        let newer = write_pending(tmp.path(), "wren", "b", "{}");
        // Make "older" actually older.
        set_mtime(&older, 60);
        let found = find_most_recent_pending("wren", tmp.path()).unwrap();
        assert_eq!(found, newer);
    }

    #[test]
    fn is_stale_true_when_older_than_timeout() {
        let tmp = TempDir::new().unwrap();
        let p = write_pending(tmp.path(), "wren", "old", "{}");
        set_mtime(&p, PENDING_TIMEOUT_SECS + 5);
        assert!(is_stale(&p, SystemTime::now()));
    }

    #[test]
    fn is_stale_false_when_fresh() {
        let tmp = TempDir::new().unwrap();
        let p = write_pending(tmp.path(), "wren", "fresh", "{}");
        assert!(!is_stale(&p, SystemTime::now()));
    }

    #[test]
    fn is_stale_true_when_file_missing() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("nope.argv.json");
        assert!(is_stale(&missing, SystemTime::now()));
    }

    #[test]
    fn build_argv_minimum_fields() {
        let payload = PendingPayload {
            title: "test card".into(),
            opts: PendingOpts {
                owner: Some("wren".into()),
                priority: Some("P1".into()),
                domain: Some("chorus".into()),
                card_type: Some("fix".into()),
                origin: Some("reactive".into()),
                sequence: None,
                subproduct: None,
                subdomain: None,
                chunk: None,
                description: None,
            },
        };
        let argv = build_cards_add_argv(&payload);
        assert_eq!(argv[0], "test card");
        assert!(argv.iter().any(|a| a == "--owner"));
        assert!(argv.iter().any(|a| a == "wren"));
        assert!(argv.iter().any(|a| a == "--priority"));
        assert!(argv.iter().any(|a| a == "P1"));
        assert!(argv.iter().any(|a| a == "--type"));
        assert!(argv.iter().any(|a| a == "fix"));
        // Optional fields not set → no flag
        assert!(!argv.iter().any(|a| a == "--sequence"));
        assert!(!argv.iter().any(|a| a == "--subproduct"));
    }

    #[test]
    fn build_argv_full_fields() {
        let payload = PendingPayload {
            title: "full card".into(),
            opts: PendingOpts {
                owner: Some("silas".into()),
                priority: Some("P1".into()),
                domain: Some("chorus".into()),
                card_type: Some("new".into()),
                origin: Some("reflective".into()),
                sequence: Some("werk".into()),
                subproduct: Some("werk".into()),
                subdomain: Some("gates-service".into()),
                chunk: Some("ops".into()),
                description: Some("body".into()),
            },
        };
        let argv = build_cards_add_argv(&payload);
        let s = argv.join(" ");
        assert!(s.contains("--sequence werk"));
        assert!(s.contains("--subproduct werk"));
        assert!(s.contains("--subdomain gates-service"));
        assert!(s.contains("--chunk ops"));
    }

    #[test]
    fn sweep_stale_removes_old_pair_and_keeps_fresh() {
        let tmp = TempDir::new().unwrap();
        let stale_argv = write_pending(tmp.path(), "wren", "old", "{}");
        let fresh_argv = write_pending(tmp.path(), "wren", "new", "{}");
        set_mtime(&stale_argv, PENDING_TIMEOUT_SECS + 30);

        let swept = sweep_stale_pending(tmp.path(), SystemTime::now());
        assert_eq!(swept, vec!["wren-old"]);
        assert!(!stale_argv.exists());
        assert!(!tmp.path().join("wren-old.txt").exists());
        assert!(fresh_argv.exists());
        assert!(tmp.path().join("wren-new.txt").exists());
    }

    #[test]
    fn remove_pending_pair_removes_both_files() {
        let tmp = TempDir::new().unwrap();
        let argv = write_pending(tmp.path(), "wren", "z", "{}");
        let txt = tmp.path().join("wren-z.txt");
        assert!(argv.exists() && txt.exists());
        remove_pending_pair(&argv);
        assert!(!argv.exists());
        assert!(!txt.exists());
    }

    fn fresh_payload_json() -> String {
        serde_json::json!({
            "title": "test card",
            "opts": {
                "owner": "wren",
                "priority": "P1",
                "domain": "chorus",
                "type": "fix",
                "origin": "reactive",
                "description": "## Experience\n\nbody"
            }
        })
        .to_string()
    }

    #[test]
    fn handle_approve_happy_path() {
        let tmp = TempDir::new().unwrap();
        write_pending(tmp.path(), "wren", "live", &fresh_payload_json());
        let mut captured_argv: Vec<String> = Vec::new();
        let mut captured_desc_present = false;
        let argv_ref = &mut captured_argv;
        let desc_ref = &mut captured_desc_present;
        let outcome = handle_approval_request(
            "wren",
            ApprovalSignal::Approve,
            tmp.path(),
            SystemTime::now(),
            |desc_path, argv| {
                *desc_ref = desc_path.exists();
                *argv_ref = argv;
                Ok(true)
            },
        );
        assert_eq!(outcome, ApprovalOutcome::Approved { title: "test card".into() });
        assert!(captured_desc_present, "desc file should exist at spawn time");
        assert_eq!(captured_argv[0], "test card");
        // pending pair cleaned up
        assert!(!tmp.path().join("wren-live.argv.json").exists());
        assert!(!tmp.path().join("wren-live.txt").exists());
    }

    #[test]
    fn handle_deny_happy_path() {
        let tmp = TempDir::new().unwrap();
        write_pending(tmp.path(), "wren", "live", &fresh_payload_json());
        let outcome = handle_approval_request(
            "wren",
            ApprovalSignal::Deny,
            tmp.path(),
            SystemTime::now(),
            |_, _| panic!("spawn_cards must not be called on deny"),
        );
        assert_eq!(outcome, ApprovalOutcome::Denied { title: "test card".into() });
        assert!(!tmp.path().join("wren-live.argv.json").exists());
        assert!(!tmp.path().join("wren-live.txt").exists());
    }

    #[test]
    fn handle_no_pending_returns_no_pending() {
        let tmp = TempDir::new().unwrap();
        let outcome = handle_approval_request(
            "wren",
            ApprovalSignal::Approve,
            tmp.path(),
            SystemTime::now(),
            |_, _| panic!("spawn must not run when there's no pending"),
        );
        assert_eq!(outcome, ApprovalOutcome::NoPending);
    }

    #[test]
    fn handle_stale_pending_times_out_and_cleans_up() {
        let tmp = TempDir::new().unwrap();
        let argv = write_pending(tmp.path(), "wren", "old", &fresh_payload_json());
        set_mtime(&argv, PENDING_TIMEOUT_SECS + 30);
        let outcome = handle_approval_request(
            "wren",
            ApprovalSignal::Approve,
            tmp.path(),
            SystemTime::now(),
            |_, _| panic!("spawn must not run when stale"),
        );
        assert_eq!(outcome, ApprovalOutcome::TimedOut);
        assert!(!argv.exists());
        assert!(!tmp.path().join("wren-old.txt").exists());
    }

    #[test]
    fn handle_parse_failed_when_json_garbage() {
        let tmp = TempDir::new().unwrap();
        write_pending(tmp.path(), "wren", "bad", "{not valid json");
        let outcome = handle_approval_request(
            "wren",
            ApprovalSignal::Approve,
            tmp.path(),
            SystemTime::now(),
            |_, _| panic!("spawn must not run on parse failure"),
        );
        assert_eq!(outcome, ApprovalOutcome::ParseFailed);
        // pending preserved for inspection
        assert!(tmp.path().join("wren-bad.argv.json").exists());
    }

    #[test]
    fn handle_spawn_failed_preserves_pending() {
        let tmp = TempDir::new().unwrap();
        let argv = write_pending(tmp.path(), "wren", "live", &fresh_payload_json());
        let outcome = handle_approval_request(
            "wren",
            ApprovalSignal::Approve,
            tmp.path(),
            SystemTime::now(),
            |_, _| Ok(false), // simulate exit-status non-zero
        );
        assert_eq!(outcome, ApprovalOutcome::SpawnFailed);
        // pending preserved so Jeff can retry / inspect
        assert!(argv.exists());
        assert!(tmp.path().join("wren-live.txt").exists());
    }

    #[test]
    fn cards_cli_path_env_override() {
        std::env::set_var("CHORUS_CARDS_BIN_TEST_ONLY", "/tmp/test-cards-bin");
        let p = resolve_cards_cli_path_with_env("CHORUS_CARDS_BIN_TEST_ONLY");
        assert_eq!(p, "/tmp/test-cards-bin");
        std::env::remove_var("CHORUS_CARDS_BIN_TEST_ONLY");
    }

    #[test]
    fn cards_cli_path_fallback_is_absolute_canonical() {
        std::env::remove_var("CHORUS_CARDS_BIN_TEST_ONLY_MISSING");
        let p = resolve_cards_cli_path_with_env("CHORUS_CARDS_BIN_TEST_ONLY_MISSING");
        assert!(p.starts_with('/'), "fallback must be absolute: {}", p);
        assert!(p.ends_with("/platform/scripts/cards"), "fallback must point at cards CLI: {}", p);
    }

    #[test]
    fn payload_deserializes_from_sdk_format() {
        let json = r###"{
            "title": "fix the thing",
            "opts": {
                "owner": "wren",
                "priority": "P1",
                "domain": "chorus",
                "type": "fix",
                "origin": "reactive",
                "sequence": "werk",
                "description": "## Experience\n\nbody"
            }
        }"###;
        let payload: PendingPayload = serde_json::from_str(json).unwrap();
        assert_eq!(payload.title, "fix the thing");
        assert_eq!(payload.opts.owner.as_deref(), Some("wren"));
        assert_eq!(payload.opts.card_type.as_deref(), Some("fix"));
    }
}
