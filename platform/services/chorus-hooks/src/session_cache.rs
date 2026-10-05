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

const EVIDENCE_UNAVAILABLE: &str = r#"{"type":"evidence-unavailable","reason":"canonical runtime history unavailable or incomplete"}"#;

fn runtime_state_root() -> std::path::PathBuf {
    std::env::var("CHORUS_AGENT_STATE_DIR").map(std::path::PathBuf::from)
        .unwrap_or_else(|_|std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".chorus"))
}

/// Only supervisor-owned registry files select the enrolled compatibility path;
/// a hook payload claiming a runtime is not enrollment.
pub fn is_enrolled(session_id: &str) -> bool {
    is_enrolled_in(&runtime_state_root(), session_id)
}

fn is_enrolled_in(root: &std::path::Path, session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        && root.join("sessions/v2").join(format!("{session_id}.json")).exists()
}

/// Runtime-local memory is not shared role memory. An enrolled non-Claude
/// session may query the canonical Chorus index, but never silently imports a
/// different host's automatic memory directory without source provenance.
pub fn legacy_memory_allowed(session_id: &str) -> bool {
    legacy_memory_allowed_in(&runtime_state_root(), session_id)
}

fn legacy_memory_allowed_in(root: &std::path::Path, session_id: &str) -> bool {
    if session_id.is_empty() || !session_id.bytes().all(|b|b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {return false;}
    let file=root.join("sessions/v2").join(format!("{session_id}.json"));
    match std::fs::read(file) {
        Err(error) if error.kind()==std::io::ErrorKind::NotFound => true,
        Err(_) => false,
        Ok(bytes) => serde_json::from_slice::<serde_json::Value>(&bytes).ok()
            .is_some_and(|session|session["runtime"]=="claude" || session["runtime"]=="claude-code"),
    }
}

/// Compatibility view for the existing guard readers. Only actual assistant
/// text and completed, normalized tool operations become evidence. A requested
/// (possibly denied) tool is not proof that it executed.
pub fn project_runtime_history(lines: &[String], session_id: &str) -> Vec<String> {
    use serde_json::{json, Value};
    let mut projected = Vec::new();
    let mut text = String::new();
    for line in lines {
        let event: Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => return vec![EVIDENCE_UNAVAILABLE.into()] };
        if event["version"] != 1 || event["session_id"] != session_id {
            return vec![EVIDENCE_UNAVAILABLE.into()];
        }
        let kind = event["type"].as_str().unwrap_or("");
        let data = &event["data"];
        if matches!(kind, "message.delta" | "message.replaced") {
            if kind == "message.replaced" { text.clear(); }
            if let Some(part) = data["text"].as_str() { text.push_str(part); }
            continue;
        }
        if !text.is_empty() {
            projected.push(json!({"type":"assistant","message":{"content":[{"type":"text","text":text}]}}).to_string());
            text.clear();
        }
        if data["evidence_unavailable"] == true { projected.push(EVIDENCE_UNAVAILABLE.into()); continue; }
        match kind {
            "message.completed" | "assistant.message" => {
                if let Some(text) = data["text"].as_str() {
                    projected.push(json!({"type":"assistant","message":{"content":[{"type":"text","text":text}]}}).to_string());
                }
            }
            "turn.started" => {
                if let Some(text) = data["prompt"].as_str() {
                    projected.push(json!({"type":"human","message":{"content":[{"type":"text","text":text}]}}).to_string());
                }
            }
            "tool.completed" => {
                let operations = match data["operations"].as_array() { Some(v) => v, None => continue };
                let failed = data["tool_output_is_error"] == true;
                for (index, op) in operations.iter().enumerate() {
                    let tool = match op["tool_name"].as_str() { Some(v) => v, None => continue };
                    let id = format!("{}-{index}", event["tool_call_id"].as_str().unwrap_or("unknown"));
                    if !failed {
                        projected.push(json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":id,"name":tool,"input":op["tool_input"]}]}}).to_string());
                    }
                    projected.push(json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":id,"content":data["tool_response"],"is_error":failed}]}}).to_string());
                }
            }
            _ => {}
        }
    }
    if !text.is_empty() { projected.push(json!({"type":"assistant","message":{"content":[{"type":"text","text":text}]}}).to_string()); }
    // Several historical guards fail open on an empty transcript. A canonical
    // session with no usable evidence must take their nonempty/no-evidence path.
    if projected.is_empty() { projected.push(EVIDENCE_UNAVAILABLE.into()); }
    projected
}

/// Extract only rendered assistant text. Tool arguments/results that quote
/// "assistant" or "Prior work:" must not impersonate a context synthesis.
pub fn assistant_text(line: &str) -> String {
    use serde_json::Value;
    let value: Value = match serde_json::from_str(line) { Ok(v) => v, Err(_) => return String::new() };
    if value["type"] != "assistant" { return String::new(); }
    let content = value.get("message").and_then(|m| m.get("content")).or_else(||value.get("content"));
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks.iter().filter(|b| b["type"] == "text")
            .filter_map(|b|b["text"].as_str()).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

fn canonical_history(root: &std::path::Path, session_id: &str) -> Option<Vec<String>> {
    use std::io::{Read, Seek, SeekFrom};
    if session_id.is_empty() || !session_id.bytes().all(|b|b.is_ascii_alphanumeric() || b == b'-' || b == b'_') { return None; }
    if !is_enrolled_in(root, session_id) { return None; }
    let unavailable = || Some(vec![EVIDENCE_UNAVAILABLE.to_string()]);
    let mut file = match std::fs::File::open(root.join("agent-events").join(format!("{session_id}.jsonl"))) { Ok(f) => f, Err(_) => return unavailable() };
    // Bounded tail: readers need <=500 recent rows, not an unbounded session.
    let length = match file.metadata() { Ok(m) => m.len(), Err(_) => return unavailable() };
    let offset = length.saturating_sub(8 * 1024 * 1024);
    if file.seek(SeekFrom::Start(offset)).is_err() { return unavailable(); }
    let mut data = String::new();
    if file.take(8 * 1024 * 1024).read_to_string(&mut data).is_err() || !data.ends_with('\n') { return unavailable(); }
    if offset > 0 { data = data.split_once('\n').map(|(_, tail)|tail.to_string()).unwrap_or_default(); }
    let lines: Vec<String> = data.lines().map(str::to_string).collect();
    Some(project_runtime_history(&lines, session_id))
}

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

    // Enrollment chooses history source. Never substitute another runtime's
    // missing evidence with a coincidentally matching Claude transcript.
    let root = runtime_state_root();
    if let Some(lines) = canonical_history(&root, session_id) { return lines; }

    // Try cwd-derived path first (fast path)
    let project_key = cwd.replace('/', "-");
    #[allow(clippy::manual_strip)]
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
        let lines: Vec<String> = reader.lines().map_while(Result::ok).collect();
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
                let lines: Vec<String> = reader.lines().map_while(Result::ok).collect();
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

    #[test]
    fn canonical_evidence_projects_completed_operations_and_actual_text() {
        use serde_json::json;
        let event = |kind, data|json!({"version":1,"session_id":"sid","type":kind,"tool_call_id":"call","data":data}).to_string();
        let lines = vec![
            event("tool.requested",json!({"decision":"deny","operations":[{"tool_name":"Write","tool_input":{"file_path":"tests/denied.rs"}}]})),
            event("tool.completed",json!({"operations":[{"tool_name":"Bash","tool_input":{"command":"git log -- src/lib.rs"}}],"tool_response":"commit 123","tool_output_is_error":false})),
            event("tool.completed",json!({"operations":[{"tool_name":"Write","tool_input":{"file_path":"tests/failed.rs"}}],"tool_response":"permission denied","tool_output_is_error":true})),
            event("message.delta",json!({"text":"Prior work: "})),
            event("message.delta",json!({"text":"the parser already validates updates."})),
        ];
        let projection = project_runtime_history(&lines, "sid");
        let joined = projection.join("\n");
        assert!(joined.contains("git log -- src/lib.rs"));
        assert!(joined.contains("commit 123"));
        assert!(!joined.contains("tests/denied.rs"));
        assert!(!joined.contains("tests/failed.rs"));
        assert!(assistant_text(projection.last().unwrap()).starts_with("Prior work: the parser"));
        assert!(assistant_text(&projection[0]).is_empty());
    }

    #[test]
    fn enrollment_with_missing_or_torn_history_is_explicitly_unavailable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("sessions/v2")).unwrap();
        std::fs::create_dir_all(dir.path().join("agent-events")).unwrap();
        std::fs::write(dir.path().join("sessions/v2/sid.json"), "{}").unwrap();
        assert_eq!(canonical_history(dir.path(), "sid").unwrap(), vec![EVIDENCE_UNAVAILABLE]);
        std::fs::write(dir.path().join("agent-events/sid.jsonl"), "{incomplete").unwrap();
        assert_eq!(canonical_history(dir.path(), "sid").unwrap(), vec![EVIDENCE_UNAVAILABLE]);
        assert!(canonical_history(dir.path(), "../../outside").is_none());
        assert!(canonical_history(dir.path(), "legacy").is_none());
    }

    #[test]
    fn tool_content_cannot_impersonate_assistant_synthesis() {
        assert!(assistant_text(r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"assistant Prior work: quoted"}]}}"#).is_empty());
        assert!(assistant_text(r#"{"type":"assistant","message":{"content":[{"type":"tool_use","input":{"content":"Prior work: quoted"}}]}}"#).is_empty());
        assert_eq!(assistant_text(r#"{"type":"assistant","content":"Prior work: real"}"#), "Prior work: real");
    }

    #[test]
    fn native_runtime_does_not_import_claude_automatic_memory() {
        let dir=tempfile::tempdir().unwrap();
        let sessions=dir.path().join("sessions/v2"); std::fs::create_dir_all(&sessions).unwrap();
        assert!(legacy_memory_allowed_in(dir.path(),"legacy"));
        assert!(!is_enrolled_in(dir.path(), "legacy"));
        assert!(!is_enrolled_in(dir.path(), "../sid"));
        for (runtime,expected) in [("codex",false),("opencode",false),("gemini",false),("claude",true)] {
            std::fs::write(sessions.join("sid.json"),serde_json::json!({"runtime":runtime}).to_string()).unwrap();
            assert_eq!(legacy_memory_allowed_in(dir.path(),"sid"),expected);
            assert!(is_enrolled_in(dir.path(), "sid"));
        }
        std::fs::write(sessions.join("sid.json"),"corrupt").unwrap();
        assert!(!legacy_memory_allowed_in(dir.path(),"sid"));
    }

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
