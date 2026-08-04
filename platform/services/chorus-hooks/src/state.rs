use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::session_cache::SessionCache;
use crate::shared::state_paths::chorus_root;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Mutex<StateInner>>,
    pub config: Arc<Config>,
    /// Session JSONL cache — shared across all hooks (#1861)
    /// Uses std::sync::Mutex internally so sync hooks can access it
    pub session_cache: SessionCache,
    /// #3278 — sessions that have edited a test file THIS run, recorded live the
    /// instant the daemon sees the Edit/Write (PreToolUse). The sync tdd_gate reads
    /// this instead of the transcript JSONL, which flushes ~a turn late and blinded
    /// the gate to write-test-then-write-code TDD in a werk. std::sync::Mutex so the
    /// sync gate can read it without an async context.
    test_edits: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
}

/// Chorus search results cached from context_inject (#2225)
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ContextSearchResults {
    pub chorus_hits: Vec<(String, String, String)>, // (role, content, timestamp)
    pub memory_hits: Vec<String>,
    pub query: String,
    pub stored_at: i64,
}

struct StateInner {
    /// Last human message per session (for autonomy-guard Stop hook)
    last_human_msg: HashMap<String, String>,
    /// Recent error timestamps per role (for struggle detection)
    recent_errors: HashMap<String, Vec<i64>>,
    /// Search block timestamps (pattern_hash -> timestamp) for retry bypass
    search_blocks: HashMap<String, i64>,
    /// Session init done markers per role
    session_init_done: HashMap<String, bool>,
    /// Interaction pattern per role (#1911): fix→swat, new→ideation, enhance→demo, chore→direction, swat→bypass
    interaction_pattern: HashMap<String, String>,
    /// Shared Chorus search results from context_inject (#2225)
    /// Key: session_id, expires after 30s
    context_results: HashMap<String, ContextSearchResults>,
    /// Prompt cycle ID (#2231) — correlates UserPromptSubmit with PreToolUse
    /// Key: session_id, Value: cycle_id (UUID)
    cycle_id: HashMap<String, String>,
}

/// Static configuration
pub struct Config {
    pub repo_root: PathBuf,
    pub log_dir: PathBuf,
    pub chorus_db: PathBuf,
    pub prefs_file: PathBuf,
    pub home_dir: PathBuf,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
        let repo_root = PathBuf::from(chorus_root());

        Self {
            inner: Arc::new(Mutex::new(StateInner {
                last_human_msg: HashMap::new(),
                recent_errors: HashMap::new(),
                search_blocks: HashMap::new(),
                session_init_done: HashMap::new(),
                interaction_pattern: HashMap::new(),
                context_results: HashMap::new(),
                cycle_id: HashMap::new(),
            })),
            session_cache: SessionCache::new(),
            test_edits: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            config: Arc::new(Config {
                log_dir: repo_root.join("platform/logs"),
                prefs_file: repo_root.join("jeff-preferences.json"),
                chorus_db: PathBuf::from(&home).join(".chorus/index.db"),
                repo_root,
                home_dir: PathBuf::from(home),
            }),
        }
    }

    /// #3278 — record that this session has edited a test file (called live from
    /// PreToolUse the instant the daemon sees an Edit/Write to a test path). Sync.
    pub fn mark_test_edit(&self, session_id: &str) {
        if let Ok(mut set) = self.test_edits.lock() {
            set.insert(session_id.to_string());
        }
    }

    /// #3278 — has this session edited a test file this run? Sync, transcript-free —
    /// the tdd_gate reads this so a same-turn test edit is visible immediately.
    pub fn has_test_edit(&self, session_id: &str) -> bool {
        self.test_edits
            .lock()
            .map(|set| set.contains(session_id))
            .unwrap_or(false)
    }

    pub async fn set_last_human_msg(&self, session_id: &str, msg: String) {
        self.inner
            .lock()
            .await
            .last_human_msg
            .insert(session_id.to_string(), msg);
    }

    pub async fn get_last_human_msg(&self, session_id: &str) -> Option<String> {
        self.inner
            .lock()
            .await
            .last_human_msg
            .get(session_id)
            .cloned()
    }

    pub async fn record_error(&self, role: &str) -> usize {
        let mut inner = self.inner.lock().await;
        let now = Utc::now().timestamp();
        let cutoff = now - 60;
        let errors = inner
            .recent_errors
            .entry(role.to_string())
            .or_default();
        errors.push(now);
        errors.retain(|&t| t >= cutoff);
        errors.len()
    }

    pub async fn set_search_block(&self, key: &str) {
        let now = Utc::now().timestamp();
        self.inner
            .lock()
            .await
            .search_blocks
            .insert(key.to_string(), now);
    }

    pub async fn check_search_block(&self, key: &str) -> bool {
        let inner = self.inner.lock().await;
        if let Some(&ts) = inner.search_blocks.get(key) {
            let now = Utc::now().timestamp();
            now - ts < 60
        } else {
            false
        }
    }

    pub async fn clear_search_block(&self, key: &str) {
        self.inner.lock().await.search_blocks.remove(key);
    }

    pub async fn mark_session_init_done(&self, role: &str) {
        self.inner
            .lock()
            .await
            .session_init_done
            .insert(role.to_string(), true);
    }

    pub async fn is_session_init_done(&self, role: &str) -> bool {
        self.inner
            .lock()
            .await
            .session_init_done
            .get(role)
            .copied()
            .unwrap_or(false)
    }

    /// Set interaction pattern for a role based on card type (#1911)
    /// Maps: fix→swat, new→ideation, enhance→demo, chore→direction, swat→bypass
    pub async fn set_interaction_pattern(&self, role: &str, card_type: &str) {
        let pattern = match card_type {
            "fix" => "swat",
            "new" => "ideation",
            "enhance" => "demo",
            "chore" => "direction",
            "swat" => "bypass",
            _ => "unknown",
        };
        self.inner
            .lock()
            .await
            .interaction_pattern
            .insert(role.to_string(), pattern.to_string());
    }

    /// Set interaction pattern directly from detected prompt (#2282)
    pub async fn set_interaction_pattern_direct(&self, role: &str, pattern: &str) {
        self.inner
            .lock()
            .await
            .interaction_pattern
            .insert(role.to_string(), pattern.to_string());
    }

    /// Get current interaction pattern for a role
    pub async fn get_interaction_pattern(&self, role: &str) -> String {
        self.inner
            .lock()
            .await
            .interaction_pattern
            .get(role)
            .cloned()
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Store Chorus search results from context_inject (#2225)
    pub async fn store_context_results(&self, session_id: &str, results: ContextSearchResults) {
        self.inner
            .lock()
            .await
            .context_results
            .insert(session_id.to_string(), results);
    }

    /// Get cached Chorus search results — returns None if expired (>30s)
    pub async fn get_context_results(&self, session_id: &str) -> Option<ContextSearchResults> {
        let inner = self.inner.lock().await;
        let result = inner.context_results.get(session_id)?;
        let now = Utc::now().timestamp();
        if now - result.stored_at > 30 {
            return None;
        }
        Some(result.clone())
    }

    /// Set prompt cycle ID for a session (#2231)
    pub async fn set_cycle_id(&self, session_id: &str, cycle_id: String) {
        self.inner.lock().await.cycle_id.insert(session_id.to_string(), cycle_id);
    }

    /// Get current prompt cycle ID for a session (#2231)
    pub async fn get_cycle_id(&self, session_id: &str) -> Option<String> {
        self.inner.lock().await.cycle_id.get(session_id).cloned()
    }

    /// Sync check: does context_inject have cached results for this session? (#2225)
    /// Used by sync hooks (memory_gate, memory_first, log_first_gate) that can't await.
    /// Returns false if lock is contended rather than blocking.
    pub fn has_context_results_sync(&self, session_id: &str) -> bool {
        match self.inner.try_lock() {
            Ok(inner) => {
                if let Some(result) = inner.context_results.get(session_id) {
                    let now = Utc::now().timestamp();
                    now - result.stored_at <= 30
                } else {
                    false
                }
            }
            Err(_) => false, // Lock contended — don't block, fall through
        }
    }
}

/// Append a JSONL line to a log file, returning the io error instead of eating it.
///
/// #3721 — this used to be the whole of `append_log`, with `if let Ok(f) = open`
/// and `let _ = write_all`. Both arms discarded their error, so a failed open and
/// a failed write each DROPPED the line with no signal anywhere. That is the
/// wrong default for this file specifically: chorus.log is the spine, the team's
/// memory layer, and an event that silently never lands is indistinguishable from
/// one that never happened. It surfaced as #3278's atomicity test getting 199 of
/// 200 lines on CI with zero corrupt lines — nothing interleaved, one append just
/// vanished, and neither the function nor the test could say why.
///
/// The lost event turned out to have TWO causes, and the swallowed error hid
/// both:
///
///   1. THE MISSING FLUSH (the actual one — see the comment at the flush call).
///      `tokio::fs::File` buffers and does not flush on drop, so `write_all`
///      could report success while the bytes never reached the OS.
///   2. Unreported io errors. Even with the flush, an open or write that genuinely
///      fails must not vanish silently on this file.
///
/// Transient failures are retried rather than lost — 200 concurrent appends can
/// hit the process fd ceiling (EMFILE/ENFILE) or a signal-interrupted syscall on
/// a small runner, and a short backoff clears those. Worth being precise about
/// what that retry is and isn't: I could not reproduce the CI loss through fd
/// pressure (0/25 locally, clean at `ulimit -n 96`), and the flush is what
/// actually explains it. The retry is defence for a condition that has not been
/// observed here; the flush is the fix. A persistent error (bad path,
/// permissions, full disk) still returns Err after the retries so the caller —
/// and the test — can name it.
pub async fn append_log_result(path: &std::path::Path, line: &str) -> std::io::Result<()> {
    use tokio::fs::OpenOptions;
    use tokio::io::AsyncWriteExt;

    // #3278 — ONE write of line+newline. Writing the line and the '\n' as two
    // separate write_all calls let another process's append land between them,
    // fusing two JSON events onto one corrupt line (~6% of chorus.log, measured
    // 2026-06-07; the test reproduced 25/63 corrupt under load). O_APPEND makes a
    // single write() atomic at EOF, so one buffer is one uninterruptible append.
    let mut buf = Vec::with_capacity(line.len() + 1);
    buf.extend_from_slice(line.as_bytes());
    buf.push(b'\n');

    let mut last: Option<std::io::Error> = None;
    for attempt in 0..RETRYABLE_APPEND_ATTEMPTS {
        if attempt > 0 {
            // Linear backoff: 2ms, 4ms, 6ms... Enough for peers to close fds,
            // short enough that a hook never visibly stalls on it.
            tokio::time::sleep(std::time::Duration::from_millis(2 * attempt as u64)).await;
        }
        let attempted = async {
            let mut f = OpenOptions::new().create(true).append(true).open(path).await?;
            f.write_all(&buf).await?;
            // #3721 — THE FLUSH IS LOAD-BEARING, and its absence was the actual
            // bug. `tokio::fs::File` is not a thin wrapper around the syscall: it
            // buffers, and dropping it does NOT flush. The old code did
            // `write_all(...).await` and let the file drop at end of scope, so a
            // write could report success and still never reach the OS. That is
            // the missing spine event — #3278's atomicity test seeing 199 of 200
            // lines with zero corrupt ones, and my own two-append unit test
            // reading back only the first line on CI. Nothing was interleaved and
            // no fd was exhausted; the bytes were simply still in a buffer that
            // got dropped.
            f.flush().await
        }
        .await;
        match attempted {
            Ok(()) => return Ok(()),
            Err(e) if is_transient_append_error(&e) => last = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("append retries exhausted")))
}

const RETRYABLE_APPEND_ATTEMPTS: usize = 5;

/// Momentary conditions worth retrying: the fd table is full right now, or the
/// syscall was interrupted. Neither says anything is wrong with the write.
fn is_transient_append_error(e: &std::io::Error) -> bool {
    use std::io::ErrorKind;
    if matches!(e.kind(), ErrorKind::Interrupted | ErrorKind::WouldBlock) {
        return true;
    }
    // EMFILE (24) = per-process fd limit, ENFILE (23) = system-wide. No stable
    // ErrorKind for these on the Rust versions we pin, so match the raw errno.
    matches!(e.raw_os_error(), Some(23) | Some(24))
}

/// Append a JSONL line to a log file (async, best-effort at the call site).
///
/// Call sites are hooks that must never fail a user's action over a log write, so
/// this still returns `()`. What changed in #3721 is that a dropped line is no
/// longer SILENT — it goes to stderr naming the path and the error. Best-effort
/// is a promise about what we do next, not a licence to lose the event quietly.
pub async fn append_log(path: &std::path::Path, line: &str) {
    if let Err(e) = append_log_result(path, line).await {
        eprintln!("chorus-hooks: DROPPED log append to {} — {e}", path.display());
    }
}

/// Emit a chorus-log event (replaces bash chorus-log.sh)
pub async fn chorus_log(event: &str, role: &str, kvs: &[(&str, &str)]) {
    let log_path = PathBuf::from(crate::shared::state_paths::chorus_log_file_for_write());
    let ts = Utc::now().with_timezone(&crate::hooks::clock_sync::boston_offset_pub()).format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string();

    let mut obj = serde_json::json!({
        "timestamp": ts,
        "level": "info",
        "appName": "chorus-events",
        "event": event,
        "role": role,
    });

    if let Some(map) = obj.as_object_mut() {
        for (k, v) in kvs {
            map.insert(k.to_string(), serde_json::Value::String(v.to_string()));
        }
    }

    append_log(&log_path, &obj.to_string()).await;
}


#[cfg(test)]
mod append_log_tests {
    use super::*;

    /// #3721 — a persistent error must SURFACE, not be swallowed. This is the
    /// regression guard for the original defect: `if let Ok(f) = open(...)` meant
    /// an unopenable path dropped the line and returned as if it had written.
    #[tokio::test]
    async fn persistent_open_failure_returns_err_instead_of_dropping() {
        // A path whose parent is a FILE, not a directory — open can never succeed.
        let parent = std::env::temp_dir().join(format!("chorus-3721-notadir-{}", std::process::id()));
        std::fs::write(&parent, b"x").expect("seed file");
        let path = parent.join("child.log");

        let err = append_log_result(&path, "{}").await.expect_err("must not report success");
        assert!(
            !is_transient_append_error(&err),
            "a non-directory parent is permanent, not transient: {err:?}"
        );

        let _ = std::fs::remove_file(&parent);
    }

    /// A good path still writes exactly the line plus one newline.
    #[tokio::test]
    async fn successful_append_writes_line_and_single_newline() {
        let path = std::env::temp_dir().join(format!("chorus-3721-ok-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);

        append_log_result(&path, "{\"a\":1}").await.expect("append should succeed");
        append_log_result(&path, "{\"a\":2}").await.expect("append should succeed");

        let content = std::fs::read_to_string(&path).expect("log readable");
        assert_eq!(content, "{\"a\":1}\n{\"a\":2}\n");
        let _ = std::fs::remove_file(&path);
    }

    /// The retry classifier must catch the fd-exhaustion errnos it exists for —
    /// EMFILE (24) and ENFILE (23) — and must NOT retry a permanent one.
    #[test]
    fn transient_classifier_covers_fd_exhaustion_only() {
        assert!(is_transient_append_error(&std::io::Error::from_raw_os_error(24)), "EMFILE");
        assert!(is_transient_append_error(&std::io::Error::from_raw_os_error(23)), "ENFILE");
        assert!(is_transient_append_error(&std::io::Error::from(std::io::ErrorKind::Interrupted)));
        assert!(!is_transient_append_error(&std::io::Error::from(std::io::ErrorKind::PermissionDenied)));
        assert!(!is_transient_append_error(&std::io::Error::from(std::io::ErrorKind::NotFound)));
    }
}
