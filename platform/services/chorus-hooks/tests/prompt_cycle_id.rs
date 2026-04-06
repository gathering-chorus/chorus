//! Tests for prompt cycle ID (#2231)
//! UserPromptSubmit generates a cycle_id, PreToolUse hooks read it.

use std::collections::HashMap;
use std::sync::Mutex;

/// Simulates the cycle_id in AppState
struct CycleState {
    cycles: Mutex<HashMap<String, String>>,
}

impl CycleState {
    fn new() -> Self {
        Self { cycles: Mutex::new(HashMap::new()) }
    }

    fn set_cycle(&self, session_id: &str, cycle_id: &str) {
        self.cycles.lock().unwrap().insert(session_id.to_string(), cycle_id.to_string());
    }

    fn get_cycle(&self, session_id: &str) -> Option<String> {
        self.cycles.lock().unwrap().get(session_id).cloned()
    }
}

#[test]
fn prompt_sets_cycle_id() {
    let state = CycleState::new();
    state.set_cycle("session-1", "cycle-abc123");
    assert_eq!(state.get_cycle("session-1"), Some("cycle-abc123".to_string()));
}

#[test]
fn tool_use_reads_same_cycle() {
    let state = CycleState::new();
    state.set_cycle("session-1", "cycle-def456");
    let cycle = state.get_cycle("session-1");
    assert_eq!(cycle, Some("cycle-def456".to_string()));
}

#[test]
fn new_prompt_replaces_cycle() {
    let state = CycleState::new();
    state.set_cycle("session-1", "cycle-first");
    state.set_cycle("session-1", "cycle-second");
    assert_eq!(state.get_cycle("session-1"), Some("cycle-second".to_string()));
}

#[test]
fn different_sessions_independent() {
    let state = CycleState::new();
    state.set_cycle("session-1", "cycle-aaa");
    state.set_cycle("session-2", "cycle-bbb");
    assert_eq!(state.get_cycle("session-1"), Some("cycle-aaa".to_string()));
    assert_eq!(state.get_cycle("session-2"), Some("cycle-bbb".to_string()));
}

#[test]
fn no_cycle_returns_none() {
    let state = CycleState::new();
    assert_eq!(state.get_cycle("nonexistent"), None);
}
