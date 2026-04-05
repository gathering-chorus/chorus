//! Session JSONL cache (#1861)
//! Reads the session JSONL file once per prompt cycle, caches the lines.
//! All hooks that need session history share this cache instead of independent reads.
//!
//! Cache invalidation: re-read if >1 second since last read.
//! This means within a single prompt cycle (multiple hooks firing), the file
//! is read exactly once. On the next prompt cycle, it re-reads.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{debug, info};

/// Cached session JSONL lines
struct CacheEntry {
    lines: Vec<String>,
    read_at: Instant,
}

/// Thread-safe session cache — uses std::sync::Mutex so sync hooks can access it
#[derive(Clone)]
pub struct SessionCache {
    inner: Arc<Mutex<HashMap<String, CacheEntry>>>,
}

impl SessionCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get cached session lines. Re-reads if cache is stale (>1s old).
    /// Returns the full line vector — each hook can apply its own window.
    pub fn get_lines(&self, session_id: &str, cwd: &str) -> Vec<String> {
        let start = Instant::now();

        // Check cache first
        {
            let cache = self.inner.lock().unwrap();
            if let Some(entry) = cache.get(session_id) {
                if entry.read_at.elapsed().as_secs() < 1 {
                    let duration_us = start.elapsed().as_micros();
                    debug!(
                        module = "session_cache",
                        session_id = session_id,
                        event = "cache_hit",
                        lines = entry.lines.len(),
                        duration_us = duration_us,
                    );
                    return entry.lines.clone();
                }
            }
        }

        // Cache miss or stale — read from disk
        let lines = read_session_jsonl(session_id, cwd);
        let duration_us = start.elapsed().as_micros();

        info!(
            module = "session_cache",
            session_id = session_id,
            event = "cache_miss",
            lines = lines.len(),
            duration_us = duration_us,
        );

        // Store in cache
        {
            let mut cache = self.inner.lock().unwrap();
            cache.insert(
                session_id.to_string(),
                CacheEntry {
                    lines: lines.clone(),
                    read_at: Instant::now(),
                },
            );
        }

        lines
    }

    /// Get the last N lines from the cached session.
    /// Convenience method — hooks specify their own window size.
    pub fn get_tail(&self, session_id: &str, cwd: &str, window: usize) -> Vec<String> {
        let lines = self.get_lines(session_id, cwd);
        let start = if lines.len() > window {
            lines.len() - window
        } else {
            0
        };
        lines[start..].to_vec()
    }
}

/// Read session JSONL from disk.
/// First tries the cwd-derived path. If not found, searches all project dirs
/// for the session UUID — handles cwd changes after hooks service restart.
fn read_session_jsonl(session_id: &str, cwd: &str) -> Vec<String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());

    // Try cwd-derived path first (fast path)
    let project_key = cwd.replace('/', "-");
    let project_key = if project_key.starts_with('-') {
        &project_key[1..]
    } else {
        &project_key
    };
    let primary_path = format!(
        "{}/.claude/projects/-{}/{}.jsonl",
        home, project_key, session_id
    );
    if let Ok(f) = std::fs::File::open(&primary_path) {
        let reader = BufReader::new(f);
        let lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();
        if !lines.is_empty() {
            return lines;
        }
    }

    // Fallback: search all project dirs for this session UUID
    // Skip overly broad dirs like -Users-jeffbridwell (home root) — triggers macOS TCC prompts
    let projects_dir = format!("{}/.claude/projects", home);
    if let Ok(entries) = std::fs::read_dir(&projects_dir) {
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            // Only scan chorus project dirs — anything else may resolve to
            // protected folders (Documents, Downloads) and trigger macOS TCC dialogs.
            // Those dialogs interrupt Jeff ~20x/day. Be strict.
            let dir_name = entry.file_name().to_string_lossy().to_string();
            if !dir_name.contains("chorus") {
                continue;
            }
            let candidate = entry.path().join(format!("{}.jsonl", session_id));
            if let Ok(f) = std::fs::File::open(&candidate) {
                let reader = BufReader::new(f);
                let lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();
                if !lines.is_empty() {
                    debug!(
                        module = "session_cache",
                        event = "fallback_found",
                        path = %candidate.display(),
                        lines = lines.len(),
                    );
                    return lines;
                }
            }
        }
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_test_jsonl(dir: &tempfile::TempDir, session_id: &str, lines: &[&str]) -> String {
        let cwd = dir.path().join("test-project");
        std::fs::create_dir_all(&cwd).unwrap();

        let cwd_str = cwd.to_string_lossy().to_string();
        let project_key = cwd_str.replace('/', "-");
        let project_key = project_key.strip_prefix('-').unwrap_or(&project_key);

        let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
        let jsonl_dir = format!("{}/.claude/projects/-{}", home, project_key);
        std::fs::create_dir_all(&jsonl_dir).unwrap();
        let jsonl_path = format!("{}/{}.jsonl", jsonl_dir, session_id);
        let mut f = std::fs::File::create(&jsonl_path).unwrap();
        for line in lines {
            writeln!(f, "{}", line).unwrap();
        }

        cwd_str
    }

    #[test]
    fn cache_returns_lines() {
        let tmp = tempfile::TempDir::new().unwrap();
        let session_id = format!("cache-test-{}", std::process::id());
        let cwd = write_test_jsonl(&tmp, &session_id, &[
            r#"{"type":"human","message":"hello"}"#,
            r#"{"type":"assistant","message":"hi"}"#,
        ]);

        let cache = SessionCache::new();
        let lines = cache.get_lines(&session_id, &cwd);
        assert_eq!(lines.len(), 2);

        // Cleanup
        let project_key = cwd.replace('/', "-");
        let project_key = project_key.strip_prefix('-').unwrap_or(&project_key);
        let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
        let _ = std::fs::remove_file(format!("{}/.claude/projects/-{}/{}.jsonl", home, project_key, session_id));
    }

    #[test]
    fn cache_hit_on_second_call() {
        let tmp = tempfile::TempDir::new().unwrap();
        let session_id = format!("cache-hit-{}", std::process::id());
        let cwd = write_test_jsonl(&tmp, &session_id, &[
            r#"{"type":"human","message":"hello"}"#,
        ]);

        let cache = SessionCache::new();

        // First call — miss
        let lines1 = cache.get_lines(&session_id, &cwd);
        assert_eq!(lines1.len(), 1);

        // Second call within 1s — hit (same data)
        let lines2 = cache.get_lines(&session_id, &cwd);
        assert_eq!(lines2.len(), 1);

        // Cleanup
        let project_key = cwd.replace('/', "-");
        let project_key = project_key.strip_prefix('-').unwrap_or(&project_key);
        let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
        let _ = std::fs::remove_file(format!("{}/.claude/projects/-{}/{}.jsonl", home, project_key, session_id));
    }

    #[test]
    fn get_tail_returns_windowed() {
        let tmp = tempfile::TempDir::new().unwrap();
        let session_id = format!("tail-test-{}", std::process::id());
        let lines: Vec<String> = (0..500).map(|i| format!(r#"{{"line":{}}}"#, i)).collect();
        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let cwd = write_test_jsonl(&tmp, &session_id, &line_refs);

        let cache = SessionCache::new();
        let tail = cache.get_tail(&session_id, &cwd, 200);
        assert_eq!(tail.len(), 200);
        // Should be the last 200 lines (300-499)
        assert!(tail[0].contains("300"));

        // Cleanup
        let project_key = cwd.replace('/', "-");
        let project_key = project_key.strip_prefix('-').unwrap_or(&project_key);
        let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/jeffbridwell".to_string());
        let _ = std::fs::remove_file(format!("{}/.claude/projects/-{}/{}.jsonl", home, project_key, session_id));
    }

    #[test]
    fn missing_file_returns_empty() {
        let cache = SessionCache::new();
        let lines = cache.get_lines("nonexistent-session", "/nonexistent/path");
        assert!(lines.is_empty());
    }
}
