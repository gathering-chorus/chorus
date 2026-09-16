// #4184 — the decision core, unit-tested with fixtures. The bats suite drives the
// built binary with stub claude/tmux/ps for the integration proofs.
use chorus_awake::{decide, parse_registry, proof_line, projects_dir_for, Live};

const NOW: u128 = 1_789_000_000_000;
fn agent(id: &str, sid: &str, pid: &str, started_ago_h: f64, state: &str) -> String {
    let started = NOW - (started_ago_h * 3600.0 * 1000.0) as u128;
    format!(r#"{{"pid":"{}","id":"{}","cwd":"/r/kade","kind":"background","startedAt":"{}","sessionId":"{}","name":"n","state":"{}"}}"#, pid, id, started, sid, state)
}

#[test]
fn registry_entry_parses_and_proof_line_reads_like_the_spec() {
    let l = parse_registry(r#"{"role":"kade","pid":56344,"tty":"/dev/ttys004","host":"tmux","tmux":"%0","registered_at":"1"}"#).unwrap();
    assert_eq!(l, Live { pid: 56344, tty: "/dev/ttys004".into(), host: "tmux".into(), pane: "%0".into() });
    assert_eq!(proof_line("kade", &l, "already awake"), "awake: kade  pid 56344  tty /dev/ttys004  pane %0  registered yes  via already awake");
    assert!(parse_registry(r#"{"role":"kade","tty":"/dev/ttys004"}"#).is_none(), "no pid is not a session");
}

#[test]
fn non_tmux_host_is_named_in_the_line_because_nudges_need_a_pane() {
    let l = parse_registry(r#"{"pid":1,"tty":"/dev/ttys001","host":"vscode"}"#).unwrap();
    assert!(proof_line("wren", &l, "x").contains("yes-but-host=vscode"));
    assert_eq!(l.pane, "-");
}

#[test]
fn attaches_only_the_background_session_that_is_the_latest_conversation() {
    let json = format!("[{}]", agent("79906dc2", "79906dc2-1681-44e5", "87866", 0.5, "working"));
    let d = decide(&json, Some("79906dc2-1681-44e5"), 24.0, NOW);
    assert_eq!(d.attach.as_deref(), Some("79906dc2"));
    assert!(d.stale.is_empty());
}

#[test]
fn negative_proof_an_older_helper_is_never_attached_when_a_newer_conversation_exists() {
    // first live run 2026-09-16: the 08:25 helper (8faa3fa4) was attached instead of the 13:00 conversation
    let json = format!("[{}]", agent("8faa3fa4", "8faa3fa4-80ae", "1", 5.0, "working"));
    let d = decide(&json, Some("38b6cebe-6041"), 24.0, NOW);
    assert_eq!(d.attach, None, "must fall to claude -c");
}

#[test]
fn exited_or_pidless_background_sessions_are_not_attachable() {
    let json = format!("[{},{}]", agent("aaaa", "aaaa-1", "", 0.1, "working"), agent("bbbb", "bbbb-1", "9", 0.1, "exited"));
    assert_eq!(decide(&json, Some("aaaa-1"), 24.0, NOW).attach, None);
    assert_eq!(decide(&json, Some("bbbb-1"), 24.0, NOW).attach, None);
}

#[test]
fn stale_is_older_than_the_window_and_never_the_attach_target() {
    let json = format!("[{},{},{}]", agent("old1", "o1", "1", 30.0, "blocked"), agent("old2", "o2", "2", 26.0, "blocked"), agent("cur", "38b6-1", "3", 30.0, "working"));
    let d = decide(&json, Some("38b6-1"), 24.0, NOW);
    assert_eq!(d.attach.as_deref(), Some("cur"));
    assert_eq!(d.stale, vec!["old1".to_string(), "old2".to_string()]);
    let d2 = decide(&json, Some("38b6-1"), 48.0, NOW);
    assert!(d2.stale.is_empty(), "a wider window names nothing");
}

#[test]
fn garbage_or_empty_session_list_means_continue_and_nothing_stale() {
    assert_eq!(decide("[]", Some("x"), 24.0, NOW), chorus_awake::Decision { attach: None, stale: vec![] });
    assert_eq!(decide("not json", Some("x"), 24.0, NOW).attach, None);
    assert_eq!(decide("[]", None, 24.0, NOW).attach, None);
}

#[test]
fn projects_dir_is_the_role_dir_with_slashes_as_dashes() {
    assert_eq!(projects_dir_for("/Users/j", "/Users/j/CascadeProjects/chorus/roles/kade").to_string_lossy(), "/Users/j/.claude/projects/-Users-j-CascadeProjects-chorus-roles-kade");
}
