// Test-first (DEC-1674): these reference pulse_gather lib functions that don't exist
// yet — RED until src/lib.rs is implemented.
//
// The card's experience: when Jeff runs /gemba (or is in /pair) he sees a role's
// REAL last tool-turn, never "no change since HH:MM" while the observation stream
// has a newer turn. The core that guarantees this is `gather_since`: it emits every
// observation strictly newer than a timestamp cursor, keyed on ts (not line index)
// so it survives the observer's 200-line rotation.
use pulse_gather::{
    effective_cursor, gather_since, parse_observation, render_gather, render_observation,
    role_delta, role_view_from_pulse, spine_args, Observation, RoleView,
};

// The observer writes ISO8601 with a fixed local offset, e.g. 2026-06-05T16:16:18-0400.
// Fixed width + fixed offset => lexicographic order == chronological order.
fn obs_line(ts: &str, tool: &str, digest: &str) -> String {
    format!(
        "{{\"ts\":\"{}\",\"role\":\"wren\",\"tool\":\"{}\",\"action\":\"{}\",\"digest\":\"{}\"}}",
        ts, tool, tool, digest
    )
}

// --- parse_observation: zero-dep JSONL line -> Observation, whitespace tolerant ---

#[test]
fn parse_observation_reads_all_fields() {
    let o = parse_observation(&obs_line("2026-06-05T16:16:18-0400", "Edit", "editing lib.rs"))
        .expect("a well-formed observation parses");
    assert_eq!(o.ts, "2026-06-05T16:16:18-0400");
    assert_eq!(o.role, "wren");
    assert_eq!(o.tool, "Edit");
    assert_eq!(o.digest, "editing lib.rs");
}

#[test]
fn parse_observation_tolerates_spaces_after_colons() {
    let line = "{\"ts\": \"2026-06-05T16:16:18-0400\", \"role\": \"wren\", \"tool\": \"Bash\", \"action\": \"Bash\", \"digest\": \"running tests\"}";
    let o = parse_observation(line).expect("pretty-printed JSON still parses");
    assert_eq!(o.tool, "Bash");
    assert_eq!(o.digest, "running tests");
}

#[test]
fn parse_observation_rejects_garbage() {
    assert!(parse_observation("").is_none());
    assert!(parse_observation("not json").is_none());
    assert!(parse_observation("{\"role\":\"wren\"}").is_none(), "missing ts is not an observation");
}

// --- gather_since: THE no-stale core (AC2, AC6) ---

#[test]
fn gather_since_emits_only_turns_newer_than_cursor() {
    let stream = [
        obs_line("2026-06-05T16:00:00-0400", "Edit", "first"),
        obs_line("2026-06-05T16:05:00-0400", "Bash", "second"),
        obs_line("2026-06-05T16:10:00-0400", "Edit", "third"),
    ]
    .join("\n");
    let r = gather_since(&stream, "2026-06-05T16:00:00-0400");
    assert_eq!(r.fresh.len(), 2, "two turns are newer than the cursor");
    assert_eq!(r.fresh[0].digest, "second");
    assert_eq!(r.fresh[1].digest, "third");
    assert_eq!(r.cursor, "2026-06-05T16:10:00-0400", "cursor advances to the newest ts");
}

#[test]
fn gather_since_empty_cursor_emits_all() {
    let stream = [
        obs_line("2026-06-05T16:00:00-0400", "Edit", "a"),
        obs_line("2026-06-05T16:05:00-0400", "Bash", "b"),
    ]
    .join("\n");
    let r = gather_since(&stream, "");
    assert_eq!(r.fresh.len(), 2, "no cursor => the whole stream is fresh");
    assert_eq!(r.cursor, "2026-06-05T16:05:00-0400");
}

// --- effective_cursor: #3274 cold-start windowing (no backlog dump) ---

#[test]
fn effective_cursor_cold_start_windows_to_last_n() {
    // 15 turns, EMPTY cursor (cold /gemba), window 10 → show the last 10, not all 15.
    let lines: Vec<String> = (0..15)
        .map(|i| obs_line(&format!("2026-06-05T16:{:02}:00-0400", i), "Edit", &format!("turn{}", i)))
        .collect();
    let stream = lines.join("\n");
    let eff = effective_cursor(&stream, "", 10);
    let r = gather_since(&stream, &eff);
    assert_eq!(r.fresh.len(), 10, "cold start shows the last 10, not the full 15 backlog");
    assert_eq!(r.fresh[0].digest, "turn5", "window starts after the 5th-oldest (last 10)");
    assert_eq!(r.fresh[9].digest, "turn14", "...through the newest");
}

#[test]
fn effective_cursor_nonempty_is_unchanged_repolls_stay_exact() {
    // A real re-poll (non-empty cursor) is NEVER windowed — exact since last poll.
    let stream = obs_line("2026-06-05T16:00:00-0400", "Edit", "a");
    assert_eq!(
        effective_cursor(&stream, "2026-06-05T15:00:00-0400", 10),
        "2026-06-05T15:00:00-0400"
    );
}

#[test]
fn effective_cursor_short_stream_shows_all() {
    // <= window turns: nothing to bury, no windowing (returns "" => gather emits all).
    let stream = [
        obs_line("2026-06-05T16:00:00-0400", "Edit", "a"),
        obs_line("2026-06-05T16:01:00-0400", "Bash", "b"),
    ]
    .join("\n");
    assert_eq!(effective_cursor(&stream, "", 10), "");
}

#[test]
fn gather_since_genuinely_no_change_holds_cursor() {
    // The honest "no change": cursor is already at the newest turn.
    let stream = obs_line("2026-06-05T16:10:00-0400", "Edit", "latest");
    let r = gather_since(&stream, "2026-06-05T16:10:00-0400");
    assert!(r.fresh.is_empty(), "nothing newer than the cursor => no deltas");
    assert_eq!(r.cursor, "2026-06-05T16:10:00-0400", "cursor unchanged when nothing is fresh");
}

#[test]
fn gather_since_regression_never_reports_no_change_while_a_newer_turn_exists() {
    // The #3205 staleness bug, pinned: gemba-tick said "no change since 16:56" while
    // the observation stream had a real turn at 17:16. With a ts-cursor, a turn newer
    // than the cursor is ALWAYS emitted — the stream is the source of truth.
    let stream = [
        obs_line("2026-06-05T16:56:00-0400", "Bash", "old turn the snapshot saw"),
        obs_line("2026-06-05T17:16:00-0400", "Edit", "the real last turn"),
    ]
    .join("\n");
    let r = gather_since(&stream, "2026-06-05T16:56:00-0400");
    assert_eq!(r.fresh.len(), 1, "the 17:16 turn is not lost between polls");
    assert_eq!(r.fresh[0].digest, "the real last turn");
}

#[test]
fn gather_since_survives_rotation_by_keying_on_ts_not_line_index() {
    // The observer caps at 200 lines and rotates; a line-index cursor would skip or
    // replay turns after a truncation. A ts-cursor resumes correctly from any window.
    let rotated = [
        obs_line("2026-06-05T18:00:00-0400", "Edit", "post-rotation A"),
        obs_line("2026-06-05T18:01:00-0400", "Bash", "post-rotation B"),
    ]
    .join("\n");
    // cursor predates the whole rotated window => both emitted, none replayed twice.
    let r = gather_since(&rotated, "2026-06-05T17:59:00-0400");
    assert_eq!(r.fresh.len(), 2);
    // re-poll with the advanced cursor => nothing replays.
    let r2 = gather_since(&rotated, &r.cursor);
    assert!(r2.fresh.is_empty(), "advanced cursor means no double-emit on re-poll");
}

#[test]
fn gather_since_ignores_blank_and_garbage_lines() {
    let stream = format!(
        "{}\n\n{}\n{}",
        obs_line("2026-06-05T16:00:00-0400", "Edit", "good1"),
        "garbage-not-json",
        obs_line("2026-06-05T16:05:00-0400", "Bash", "good2"),
    );
    let r = gather_since(&stream, "");
    assert_eq!(r.fresh.len(), 2, "blank + unparseable lines are skipped, not fatal");
}

// --- role_view_from_pulse + role_delta: state deltas off pulse.* (AC1 one source) ---

#[test]
fn role_view_from_pulse_extracts_state_and_card() {
    let pulse = "{\"roles\":{\"wren\":{\"state\":\"building\",\"card\":\"3205\"},\"kade\":{\"state\":\"idle\",\"card\":\"\"}}}";
    let v = role_view_from_pulse(pulse, "wren");
    assert_eq!(v.state, "building");
    assert_eq!(v.card, "3205");
}

#[test]
fn role_delta_reports_state_and_card_changes() {
    let prev = RoleView { state: "idle".into(), card: "".into() };
    let cur = RoleView { state: "building".into(), card: "3205".into() };
    let d = role_delta(&prev, &cur);
    assert_eq!(d.len(), 2, "both state and card changed");
    assert!(d.iter().any(|s| s.contains("idle") && s.contains("building")), "state transition narrated");
    assert!(d.iter().any(|s| s.contains("3205")), "card change narrated");
}

#[test]
fn role_delta_empty_when_unchanged() {
    let v = RoleView { state: "building".into(), card: "3205".into() };
    assert!(role_delta(&v, &v).is_empty(), "no change => no delta lines");
}

// --- render_observation: terse, terminal-friendly narration ---

#[test]
fn render_observation_is_terse_and_carries_signal() {
    let o = Observation {
        ts: "2026-06-05T16:16:18-0400".into(),
        role: "wren".into(),
        tool: "Edit".into(),
        action: "Edit".into(),
        digest: "editing lib.rs".into(),
    };
    let line = render_observation(&o);
    assert!(line.contains("Edit"), "tool is visible");
    assert!(line.contains("editing lib.rs"), "digest is visible");
}

// --- render_gather: reboot-safety contract (Silas #3205 review) ---
// A missing stream is reboot-blindness, not idle. Saying "no activity" when /tmp was
// wiped is the exact staleness lie this verb kills — so an absent stream says
// "rebuilding", a present-but-quiet stream is genuinely silent.

#[test]
fn render_gather_says_rebuilding_when_stream_absent_not_idle() {
    let out = render_gather("kade", false, &[]);
    assert!(!out.is_empty(), "an absent stream must NOT be silent (that's the reboot lie)");
    assert!(out.contains("rebuilding"), "absent stream is announced as rebuilding, not no-activity");
}

#[test]
fn render_gather_silent_on_genuine_quiet() {
    assert_eq!(
        render_gather("kade", true, &[]),
        "",
        "present stream with no turns newer than the cursor is genuine quiet => silent"
    );
}

#[test]
fn render_gather_lists_fresh_turns() {
    let o = Observation {
        ts: "2026-06-05T16:00:00-0400".into(),
        role: "kade".into(),
        tool: "Edit".into(),
        action: "Edit".into(),
        digest: "x".into(),
    };
    let out = render_gather("kade", true, std::slice::from_ref(&o));
    assert!(out.contains("(1 new)"), "fresh turns are counted and listed");
}

// --- spine_args: mirror the werk-verb spine contract (queryable in Loki) ---

#[test]
fn spine_args_carry_event_role_and_extras() {
    let a = spine_args("pulse.gathered", "wren", &[("count", "3"), ("target", "kade")]);
    assert_eq!(a[0], "pulse.gathered", "event is first");
    assert_eq!(a[1], "wren", "role is second");
    assert!(a.contains(&"count=3".to_string()));
    assert!(a.contains(&"target=kade".to_string()));
}
