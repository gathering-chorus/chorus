// @domain: domains
// @test-type: unit — the spine template compiled directly against fixture logs; no service
// #4991 (AC1): a call logged without a call_id is still a stream row
//! #4991 — Jeff's phone 11-17 07:33: Streams showed only Wren. Abby's runtime logs every
//! call with call_id "" (47 in one 4 MB window), and the #4766 reader dropped them; the old
//! Clearing showed them. Replies are rows too (phase "reply", AC2's data), never work.
#[path = "../templates/spine_log.rs"]
#[allow(dead_code)]
mod spine_log;

fn act(ts: &str, role: &str, phase: &str, id: &str, digest: &str) -> String {
    format!(r#"{{"timestamp":"{ts}","event":"agent.action","role":"{role}","phase":"{phase}","call_id":"{id}","tool":"Bash","digest":"{digest}","session_id":"s-{role}","outcome":"ok"}}"#)
}
fn turn_end(ts: &str, role: &str) -> String {
    format!(r#"{{"timestamp":"{ts}","event":"agent.turn.ended","role":"{role}","session_id":"s-{role}"}}"#)
}

#[test]
fn id_less_calls_are_rows_each_closed_by_its_own_end() {
    let log = [
        act("2026-11-17T07:08:13", "abby-normal", "started", "", "nudge wren"),
        act("2026-11-17T07:08:17", "abby-normal", "ended", "", ""),
        act("2026-11-17T07:08:49", "abby-normal", "started", "", "werk status"),
    ].join("\n");
    let got: Vec<(String, String, String)> = spine_log::calls(&log).into_iter().map(|c| (c.role, c.what, c.phase)).collect();
    assert_eq!(got, [
        ("abby-normal".into(), "werk status".into(), "running".into()),
        ("abby-normal".into(), "nudge wren".into(), "done".into()),
    ]);
}

/// NEGATIVE PROOF: an id-less call left open when its turn ends does not steal the next
/// turn's end; it reads stopped, and the next call reads done.
#[test]
fn an_interrupted_id_less_call_never_takes_the_next_turns_end() {
    let log = [
        act("2026-11-17T07:00:01", "abby-normal", "started", "", "first"),
        turn_end("2026-11-17T07:00:05", "abby-normal"),
        act("2026-11-17T07:01:00", "abby-normal", "started", "", "second"),
        act("2026-11-17T07:01:02", "abby-normal", "ended", "", ""),
    ].join("\n");
    let got: Vec<(String, String)> = spine_log::calls(&log).into_iter().map(|c| (c.what, c.phase)).collect();
    assert_eq!(got, [("second".into(), "done".into()), ("first".into(), "stopped".into())]);
}

/// NEGATIVE PROOF: the #4766 reader (id required) would yield no row here; this one yields both.
#[test]
fn the_old_rule_would_have_dropped_every_one_of_these() {
    let log = [act("2026-11-17T07:08:13", "abby-normal", "started", "", "a"), act("2026-11-17T07:09:13", "abby-normal", "started", "", "b")].join("\n");
    let old_rule: usize = log.lines().filter(|l| !l.contains(r#""call_id":"""#)).count();
    assert_eq!(old_rule, 0);
    assert_eq!(spine_log::calls(&log).len(), 2);
}

#[test]
fn a_reply_is_a_row_marked_reply_with_its_length_never_a_call() {
    let log = r#"{"timestamp":"2026-11-17T06:56:14.371-0500","event":"reply.emitted","role":"wren","session":"s-wren","words":"95"}"#;
    let c = &spine_log::calls(log)[0];
    assert_eq!((c.phase.as_str(), c.tool.as_str(), c.what.as_str()), ("reply", "reply", "replied (95 words)"));
}

/// #4991 demo review (quality) — two id-less calls of one tool open at once: each end closes
/// the OLDEST open one (FIFO). LIFO would close "second" first and leave "first" running.
#[test]
fn two_open_id_less_calls_of_one_tool_close_oldest_first() {
    let log = [
        act("2026-11-17T07:00:01", "abby-normal", "started", "", "first"),
        act("2026-11-17T07:00:02", "abby-normal", "started", "", "second"),
        act("2026-11-17T07:00:03", "abby-normal", "ended", "", ""),
    ].join("\n");
    let got: Vec<(String, String)> = spine_log::calls(&log).into_iter().map(|c| (c.what, c.phase)).collect();
    assert!(got.contains(&("first".into(), "done".into())), "{got:?}");
    assert!(got.contains(&("second".into(), "running".into())), "{got:?}");
}
