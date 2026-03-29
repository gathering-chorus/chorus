use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    inner: Arc<Mutex<StateInner>>,
    pub config: Arc<Config>,
}

struct StateInner {
    /// Last human message per session (for autonomy-guard Stop hook)
    last_human_msg: HashMap<String, String>,
    /// Recent error timestamps per role (for struggle detection)
    recent_errors: HashMap<String, Vec<i64>>,
    /// Search block timestamps (pattern_hash -> timestamp) for retry bypass
    search_blocks: HashMap<String, i64>,
    /// Last Chorus search timestamp per role
    chorus_last_search: HashMap<String, i64>,
    /// Session init done markers per role
    session_init_done: HashMap<String, bool>,
}

/// Static configuration
pub struct Config {
    pub repo_root: PathBuf,
    pub log_dir: PathBuf,
    pub chorus_db: PathBuf,
    pub prefs_file: PathBuf,
    pub home_dir: PathBuf,
}

impl AppState {
    pub fn new() -> Self {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
        let repo_root = PathBuf::from("/Users/jeffbridwell/CascadeProjects");

        Self {
            inner: Arc::new(Mutex::new(StateInner {
                last_human_msg: HashMap::new(),
                recent_errors: HashMap::new(),
                search_blocks: HashMap::new(),
                chorus_last_search: HashMap::new(),
                session_init_done: HashMap::new(),
            })),
            config: Arc::new(Config {
                log_dir: repo_root.join("chorus/platform/logs"),
                prefs_file: repo_root.join("chorus/jeff-preferences.json"),
                chorus_db: PathBuf::from(&home).join(".chorus/index.db"),
                repo_root,
                home_dir: PathBuf::from(home),
            }),
        }
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

    pub async fn set_chorus_searched(&self, role: &str) {
        let now = Utc::now().timestamp();
        self.inner
            .lock()
            .await
            .chorus_last_search
            .insert(role.to_string(), now);
    }

    pub async fn chorus_searched_recently(&self, role: &str) -> bool {
        let inner = self.inner.lock().await;
        if let Some(&ts) = inner.chorus_last_search.get(role) {
            Utc::now().timestamp() - ts < 90
        } else {
            false
        }
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
}

/// Append a JSONL line to a log file (async, best-effort)
pub async fn append_log(path: &std::path::Path, line: &str) {
    use tokio::fs::OpenOptions;
    use tokio::io::AsyncWriteExt;

    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
    {
        let _ = f.write_all(line.as_bytes()).await;
        let _ = f.write_all(b"\n").await;
    }
}

/// Emit a chorus-log event (replaces bash chorus-log.sh)
pub async fn chorus_log(event: &str, role: &str, kvs: &[(&str, &str)]) {
    let log_path = PathBuf::from("/Users/jeffbridwell/CascadeProjects/chorus/platform/logs/chorus.log");
    let ts = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();

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
