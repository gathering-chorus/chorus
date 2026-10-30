// @domain: spine — placed by chorus-crawl's domain rules (#4538)
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
    pub chunk: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Single canonical pending directory for a role (#4786 AC4):
/// `$CHORUS_STATE_DIR/pending-approvals/<role>/` (or `$CHORUS_PENDING_DIR/<role>/`).
/// The bouncer (running as the role) writes here, and the responder (running as
/// launcher/jeffbridwell) reads only this location. Avoids crossing 0700 role home dirs.
pub fn role_pending_dir(role: &str) -> PathBuf {
    let role_clean = role.trim_start_matches("chorus-");
    if let Ok(dir) = std::env::var("CHORUS_PENDING_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir).join(role_clean);
        }
    }
    let state_dir = std::env::var("CHORUS_STATE_DIR")
        .unwrap_or_else(|_| "/Users/jeffbridwell/.chorus".to_string());
    PathBuf::from(state_dir).join("pending-approvals").join(role_clean)
}

#[derive(Debug, PartialEq, Eq)]
pub enum PendingStoreError {
    RetryTooSoon { age_ms: u64, path: String },
    Io(String),
}

impl std::fmt::Display for PendingStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PendingStoreError::RetryTooSoon { age_ms, path } => {
                write!(
                    f,
                    "pending-retry-too-soon: same-role+title written {}ms ago at {}",
                    age_ms, path
                )
            }
            PendingStoreError::Io(e) => write!(f, "io error: {}", e),
        }
    }
}

/// Store a pending approval payload (#4786 AC4).
/// Called by the daemon when bouncer submits a pending request over the socket.
/// Handles deduplication (overwrites in place if within 10min) and retry refusal (if within 30s).
pub fn store_pending_approval(
    role: &str,
    stamp: &str,
    nudge: &str,
    title: &str,
    opts: &serde_json::Value,
    now: SystemTime,
) -> Result<(PathBuf, PathBuf), PendingStoreError> {
    let role_clean = role.trim_start_matches("chorus-");
    let dir = role_pending_dir(role_clean);
    std::fs::create_dir_all(&dir).map_err(|e| PendingStoreError::Io(e.to_string()))?;

    // Check existing pending files for dedupe & retry refusal
    let prefix = format!("{}-", role_clean);
    let entries = std::fs::read_dir(&dir).map_err(|e| PendingStoreError::Io(e.to_string()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.starts_with(&prefix) || !name.ends_with(".argv.json") {
            continue;
        }
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) {
                if parsed.get("title").and_then(|v| v.as_str()) == Some(title) {
                    if let Ok(meta) = entry.metadata().and_then(|m| m.modified()) {
                        let age = now.duration_since(meta).unwrap_or(Duration::ZERO);
                        let age_ms = age.as_millis() as u64;
                        if age < Duration::from_secs(30) {
                            return Err(PendingStoreError::RetryTooSoon {
                                age_ms,
                                path: path.to_string_lossy().to_string(),
                            });
                        }
                        if age < Duration::from_secs(600) {
                            // Dedupe: overwrite in place using the same stem
                            let stem = path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("");
                            let stem = stem.strip_suffix(".argv").unwrap_or(stem);
                            let txt_path = dir.join(format!("{}.txt", stem));
                            let argv_path = path.clone();
                            let argv_payload = serde_json::json!({
                                "title": title,
                                "opts": opts,
                            });
                            std::fs::write(
                                &argv_path,
                                serde_json::to_string_pretty(&argv_payload)
                                    .unwrap_or_default()
                                    + "\n",
                            )
                            .map_err(|e| PendingStoreError::Io(e.to_string()))?;
                            std::fs::write(&txt_path, format!("{}\n", nudge))
                                .map_err(|e| PendingStoreError::Io(e.to_string()))?;
                            return Ok((txt_path, argv_path));
                        }
                    }
                }
            }
        }
    }

    // Write new pair
    let txt_path = dir.join(format!("{}-{}.txt", role_clean, stamp));
    let argv_path = dir.join(format!("{}-{}.argv.json", role_clean, stamp));
    let argv_payload = serde_json::json!({
        "title": title,
        "opts": opts,
    });
    std::fs::write(
        &argv_path,
        serde_json::to_string_pretty(&argv_payload).unwrap_or_default() + "\n",
    )
    .map_err(|e| PendingStoreError::Io(e.to_string()))?;
    std::fs::write(&txt_path, format!("{}\n", nudge))
        .map_err(|e| PendingStoreError::Io(e.to_string()))?;
    Ok((txt_path, argv_path))
}

/// Find ALL `<role>-*.argv.json` files under `pending_dir`, ordered oldest-first
/// by mtime so callers can process them in queue order. #2964 / #4786: when Jeff approves,
/// the responder drains the queue for the role in its canonical pending directory.
pub fn find_all_pending(role: &str, pending_dir: &Path) -> Vec<PathBuf> {
    let role_clean = role.trim_start_matches("chorus-");
    let prefix = format!("{}-", role_clean);
    let entries = match std::fs::read_dir(pending_dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut hits: Vec<(PathBuf, SystemTime)> = Vec::new();
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
        hits.push((path, mtime));
    }
    hits.sort_by_key(|(_, t)| *t);
    hits.into_iter().map(|(p, _)| p).collect()
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
    let pairs: [(&str, &Option<String>); 8] = [
        ("--owner", &opts.owner),
        ("--priority", &opts.priority),
        ("--domain", &opts.domain),
        ("--type", &opts.card_type),
        ("--origin", &opts.origin),
        ("--sequence", &opts.sequence),
        ("--subproduct", &opts.subproduct),
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

/// #2964 / #4786: drain the entire pending queue for a role on one approval keyword
/// from its canonical role pending directory.
/// Iterates over every `<role>-*.argv.json` in `pending_dir` oldest-first.
/// Each payload: stale/parse/approve/deny handling inline
/// (stale → TimedOut + cleanup; parse-fail → ParseFailed + preserve; approve
/// → spawn + cleanup on success; deny → cleanup). Returns one `ApprovalOutcome`
/// per payload, in processing order — caller iterates to emit per-card spine
/// events ("Filed #2959, #2960, #2961" instead of one).
///
/// `spawn_cards` is called once per pending payload — closures must be
/// callable repeatedly (so `Fn`, not `FnOnce`).
pub fn handle_approval_request_all<F>(
    role: &str,
    signal: ApprovalSignal,
    pending_dir: &Path,
    now: SystemTime,
    spawn_cards: F,
) -> Vec<ApprovalOutcome>
where
    F: Fn(&Path, Vec<String>) -> std::io::Result<bool>,
{
    let pending_paths = find_all_pending(role, pending_dir);
    if pending_paths.is_empty() {
        return vec![ApprovalOutcome::NoPending];
    }
    let mut outcomes = Vec::with_capacity(pending_paths.len());
    for path in pending_paths {
        if is_stale(&path, now) {
            remove_pending_pair(&path);
            outcomes.push(ApprovalOutcome::TimedOut);
            continue;
        }
        let payload_str = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => {
                outcomes.push(ApprovalOutcome::ReadFailed);
                continue;
            }
        };
        let payload: PendingPayload = match serde_json::from_str(&payload_str) {
            Ok(p) => p,
            Err(_) => {
                outcomes.push(ApprovalOutcome::ParseFailed);
                continue;
            }
        };
        match signal {
            ApprovalSignal::Deny => {
                let title = payload.title.clone();
                remove_pending_pair(&path);
                outcomes.push(ApprovalOutcome::Denied { title });
            }
            ApprovalSignal::Approve => {
                let desc = payload.opts.description.clone().unwrap_or_default();
                let desc_path = std::env::temp_dir().join(format!(
                    "card-approval-{}-{}-{}.md",
                    std::process::id(),
                    now.duration_since(SystemTime::UNIX_EPOCH)
                        .map(|d| d.as_millis())
                        .unwrap_or(0),
                    outcomes.len(),
                ));
                if std::fs::write(&desc_path, &desc).is_err() {
                    outcomes.push(ApprovalOutcome::ReadFailed);
                    continue;
                }
                let argv = build_cards_add_argv(&payload);
                let result = spawn_cards(&desc_path, argv);
                let _ = std::fs::remove_file(&desc_path);
                match result {
                    Ok(true) => {
                        let title = payload.title.clone();
                        remove_pending_pair(&path);
                        outcomes.push(ApprovalOutcome::Approved { title });
                    }
                    _ => outcomes.push(ApprovalOutcome::SpawnFailed),
                }
            }
        }
    }
    outcomes
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
/// - Pulse's wake line is ignored — a nudge arrived, not Jeff typing (#4339).
///   A "[nudge from" label is no longer a peer: peers never type words into
///   the pane, so any typed words are Jeff's.
/// - Quoted/template content (`<…>`) is ignored.
/// - When BOTH `approve` and `deny` appear, returns `None` — ambiguous.
pub fn detect_approval_signal(prompt: &str) -> Option<ApprovalSignal> {
    if prompt.is_empty() || prompt.len() > 200 {
        return None;
    }
    let trimmed = prompt.trim();
    if crate::shared::wake::is_relay(trimmed) || trimmed.starts_with('<') {
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

    /// #4339 — a nudge reaches the pane only as pulse's wake line, which can
    /// never approve. A "[nudge from" label typed into the pane is Jeff's
    /// words, so it approves like any other thing he types.
    #[test]
    fn no_match_on_the_wake_line_and_a_typed_label_is_jeff() {
        assert_eq!(detect_approval_signal(crate::shared::wake::WAKE_LINE), None);
        assert_eq!(
            detect_approval_signal("[nudge from silas | 2026-05-15 10:00] approve please"),
            Some(ApprovalSignal::Approve)
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
                chunk: Some("ops".into()),
                description: Some("body".into()),
            },
        };
        let argv = build_cards_add_argv(&payload);
        let s = argv.join(" ");
        assert!(s.contains("--sequence werk"));
        assert!(s.contains("--subproduct werk"));
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

    // ---- #2964 drain-all-pending tests ----

    #[test]
    fn find_all_pending_returns_empty_when_dir_missing() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("does-not-exist");
        assert!(find_all_pending("wren", &missing).is_empty());
    }

    #[test]
    fn find_all_pending_filters_by_role_and_orders_oldest_first() {
        let tmp = TempDir::new().unwrap();
        // Three wren files + one silas file. The silas file must not appear.
        let a = write_pending(tmp.path(), "wren", "a", "{}");
        let b = write_pending(tmp.path(), "wren", "b", "{}");
        let c = write_pending(tmp.path(), "wren", "c", "{}");
        let _silas = write_pending(tmp.path(), "silas", "x", "{}");
        // Force ordering: a oldest, b middle, c newest
        set_mtime(&a, 120);
        set_mtime(&b, 60);
        // c stays fresh
        let all = find_all_pending("wren", tmp.path());
        assert_eq!(all.len(), 3);
        assert_eq!(all[0], a, "oldest first");
        assert_eq!(all[1], b);
        assert_eq!(all[2], c, "newest last");
    }

    #[test]
    fn handle_all_approve_files_every_queued_payload() {
        let tmp = TempDir::new().unwrap();
        write_pending(tmp.path(), "wren", "first", &fresh_payload_json());
        write_pending(tmp.path(), "wren", "second", &fresh_payload_json_titled("second card"));
        write_pending(tmp.path(), "wren", "third", &fresh_payload_json_titled("third card"));
        let calls = std::sync::Mutex::new(0u32);
        let outcomes = handle_approval_request_all(
            "wren",
            ApprovalSignal::Approve,
            tmp.path(),
            SystemTime::now(),
            |_, _| {
                *calls.lock().unwrap() += 1;
                Ok(true)
            },
        );
        assert_eq!(outcomes.len(), 3);
        assert_eq!(*calls.lock().unwrap(), 3, "spawn called once per payload");
        for o in &outcomes {
            assert!(matches!(o, ApprovalOutcome::Approved { .. }));
        }
        // All pending pairs cleaned up
        assert!(find_all_pending("wren", tmp.path()).is_empty());
    }

    #[test]
    fn handle_all_returns_no_pending_when_queue_empty() {
        let tmp = TempDir::new().unwrap();
        let outcomes = handle_approval_request_all(
            "wren",
            ApprovalSignal::Approve,
            tmp.path(),
            SystemTime::now(),
            |_, _| panic!("spawn must not run when queue empty"),
        );
        assert_eq!(outcomes, vec![ApprovalOutcome::NoPending]);
    }

    #[test]
    fn handle_all_mixed_fresh_and_stale_processes_each_appropriately() {
        let tmp = TempDir::new().unwrap();
        let stale = write_pending(tmp.path(), "wren", "old", &fresh_payload_json());
        set_mtime(&stale, PENDING_TIMEOUT_SECS + 30);
        write_pending(tmp.path(), "wren", "new", &fresh_payload_json_titled("new card"));
        let outcomes = handle_approval_request_all(
            "wren",
            ApprovalSignal::Approve,
            tmp.path(),
            SystemTime::now(),
            |_, _| Ok(true),
        );
        assert_eq!(outcomes.len(), 2);
        // Stale (older mtime → first) is TimedOut; fresh is Approved
        assert!(matches!(outcomes[0], ApprovalOutcome::TimedOut));
        assert!(matches!(outcomes[1], ApprovalOutcome::Approved { .. }));
        // Both cleaned up regardless
        assert!(!stale.exists());
    }

    #[test]
    fn handle_all_deny_clears_queue_without_spawning() {
        let tmp = TempDir::new().unwrap();
        write_pending(tmp.path(), "wren", "a", &fresh_payload_json());
        write_pending(tmp.path(), "wren", "b", &fresh_payload_json_titled("b"));
        let outcomes = handle_approval_request_all(
            "wren",
            ApprovalSignal::Deny,
            tmp.path(),
            SystemTime::now(),
            |_, _| panic!("spawn must not run on deny"),
        );
        assert_eq!(outcomes.len(), 2);
        for o in &outcomes {
            assert!(matches!(o, ApprovalOutcome::Denied { .. }));
        }
        assert!(find_all_pending("wren", tmp.path()).is_empty());
    }

    #[test]
    fn handle_all_spawn_failure_preserves_offending_payload_continues_others() {
        let tmp = TempDir::new().unwrap();
        let a = write_pending(tmp.path(), "wren", "a", &fresh_payload_json_titled("a"));
        let b = write_pending(tmp.path(), "wren", "b", &fresh_payload_json_titled("b"));
        // Spawn fails for "b", succeeds for "a"
        let outcomes = handle_approval_request_all(
            "wren",
            ApprovalSignal::Approve,
            tmp.path(),
            SystemTime::now(),
            |_, argv| {
                let title = argv.first().cloned().unwrap_or_default();
                Ok(title == "a")
            },
        );
        assert_eq!(outcomes.len(), 2);
        // a succeeds → Approved, cleaned up
        assert!(matches!(outcomes[0], ApprovalOutcome::Approved { .. }));
        assert!(!a.exists());
        // b fails → SpawnFailed, preserved
        assert!(matches!(outcomes[1], ApprovalOutcome::SpawnFailed));
        assert!(b.exists());
    }

    fn fresh_payload_json_titled(title: &str) -> String {
        serde_json::json!({
            "title": title,
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

    #[test]
    fn role_pending_dir_resolves_to_state_dir_pending_approvals_role() {
        let dir = role_pending_dir("silas");
        assert!(
            dir.ends_with("pending-approvals/silas"),
            "must end with pending-approvals/silas: {:?}",
            dir
        );

        let dir_prefixed = role_pending_dir("chorus-silas");
        assert!(
            dir_prefixed.ends_with("pending-approvals/silas"),
            "must strip chorus- prefix: {:?}",
            dir_prefixed
        );
    }

    #[test]
    fn negative_proof_file_under_role_home_is_not_picked_up() {
        // #4786 AC4 / #3734 negative proof:
        // Silas directs: bouncer writes to $CHORUS_STATE_DIR/pending-approvals/<role>/
        // responder reads ONLY that.
        // Negative proof: a file under /Users/chorus-<role>/ is NOT picked up.
        let tmp = TempDir::new().unwrap();
        let role_isolated_dir = tmp.path().join("Users/chorus-silas/.chorus/pending-approvals");
        let state_pending_dir = tmp.path().join("chorus_state/pending-approvals/silas");
        fs::create_dir_all(&role_isolated_dir).unwrap();
        fs::create_dir_all(&state_pending_dir).unwrap();

        // Step 1: write a file under the role's home dir
        let home_file = write_pending(
            &role_isolated_dir,
            "silas",
            "16-25-00",
            &fresh_payload_json_titled("silas-home-card"),
        );
        assert!(home_file.exists());

        // Negative proof: responder reads ONLY the state dir (role_pending_dir).
        // It does NOT read the role's home directory.
        let spawn_count = std::sync::Mutex::new(0u32);
        let outcomes_home = handle_approval_request_all(
            "silas",
            ApprovalSignal::Approve,
            &state_pending_dir,
            SystemTime::now(),
            |_, _| {
                *spawn_count.lock().unwrap() += 1;
                Ok(true)
            },
        );
        assert_eq!(outcomes_home, vec![ApprovalOutcome::NoPending]);
        assert_eq!(*spawn_count.lock().unwrap(), 0, "must not file card from role home dir");
        assert!(home_file.exists(), "file under role home is NOT picked up");

        // Positive proof: bouncer writes to $CHORUS_STATE_DIR/pending-approvals/<role>/
        let state_file = write_pending(
            &state_pending_dir,
            "silas",
            "20-10-00",
            &fresh_payload_json_titled("silas-state-card"),
        );
        assert!(state_file.exists());

        let outcomes_state = handle_approval_request_all(
            "silas",
            ApprovalSignal::Approve,
            &state_pending_dir,
            SystemTime::now(),
            |_, argv| {
                *spawn_count.lock().unwrap() += 1;
                assert_eq!(argv.first().map(|s| s.as_str()), Some("silas-state-card"));
                Ok(true)
            },
        );
        assert_eq!(outcomes_state.len(), 1);
        assert_eq!(*spawn_count.lock().unwrap(), 1, "card filed exactly once");
        assert!(matches!(outcomes_state[0], ApprovalOutcome::Approved { ref title } if title == "silas-state-card"));
        assert!(!state_file.exists(), "state file cleaned up after approval");

        // Second run does not double-file
        let second_run = handle_approval_request_all(
            "silas",
            ApprovalSignal::Approve,
            &state_pending_dir,
            SystemTime::now(),
            |_, _| panic!("spawn must not run when queue empty"),
        );
        assert_eq!(second_run, vec![ApprovalOutcome::NoPending]);
    }

    #[test]
    fn store_pending_approval_writes_and_refuses_retry() {
        let tmp = TempDir::new().unwrap();
        std::env::set_var("CHORUS_PENDING_DIR", tmp.path().join("pending-approvals"));

        let opts = serde_json::json!({"owner": "silas", "priority": "P1"});
        let now = SystemTime::now();

        // 1. Initial write
        let (txt, argv) = store_pending_approval("silas", "10-00-00", "nudge body", "my title", &opts, now).unwrap();
        assert!(txt.exists());
        assert!(argv.exists());
        assert!(argv.to_string_lossy().contains("silas-10-00-00.argv.json"));

        // 2. Retry within 30s -> Refused
        let err = store_pending_approval("silas", "10-00-10", "nudge body", "my title", &opts, now).unwrap_err();
        match err {
            PendingStoreError::RetryTooSoon { age_ms: _, path } => {
                assert!(path.contains("silas-10-00-00.argv.json"));
            }
            _ => panic!("expected RetryTooSoon, got {:?}", err),
        }

        // 3. Retry within 10min -> Deduplicated / overwritten in place
        let later = now + Duration::from_secs(60);
        let (txt2, argv2) = store_pending_approval("silas", "10-01-00", "nudge updated", "my title", &opts, later).unwrap();
        assert_eq!(argv, argv2, "dedupes by overwriting in place with original stem");
        assert_eq!(txt, txt2);
        let content = fs::read_to_string(&txt2).unwrap();
        assert!(content.contains("nudge updated"));

        std::env::remove_var("CHORUS_PENDING_DIR");
    }
}
