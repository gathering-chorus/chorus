//! #3858 — the room/phone announce is ONE LINE. Jeff decides go/no/more from a
//! glance; the full cockpit stays the session reply. TDD red-first.

use werk_demo::bridge_announce_line;

#[test]
fn bridge_line_is_one_line_with_the_decision_contract() {
    let line = bridge_announce_line(3858, "Demo announce — one line", 4, 4, true, "abc123def456");
    assert!(!line.contains('\n'), "one line — it fills a phone screen otherwise");
    assert!(line.contains("#3858"), "carries the card");
    assert!(line.contains("AC 4/4"), "carries the AC tally");
    assert!(line.contains("go / no / more"), "carries the decision ask");
    assert!(line.contains("borg/trace.html?card=3858"), "detail behind the trace link");
    assert!(line.len() < 220, "glanceable, got {} chars", line.len());
}

/// #3734 negative proof: gates-red must be VISIBLE, never dressed as ready.
#[test]
fn bridge_line_names_red_gates() {
    let line = bridge_announce_line(9, "t", 2, 5, false, "r");
    assert!(line.contains("gates ✗") || line.contains("gates FAIL"), "red gates named: {line}");
    assert!(line.contains("AC 2/5"), "partial AC visible");
}
