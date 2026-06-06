// Test-first (DEC-1674): references pair_heartbeat lib fns that don't exist yet — RED.
//
// The daemon tick's job: for each active pair, decide whether the navigator has gone
// silent and at what level, firing each escalation ONCE (on level increase), never
// re-spamming. Silence is measured on the DAEMON's own clock (it stamps last_active
// when it sees a newer observation ts), so no ISO→epoch parsing is needed.
use pair_heartbeat::{escalation, newest_ts, silence_level, Level};

fn obs(ts: &str) -> String {
    format!("{{\"ts\":\"{}\",\"role\":\"silas\",\"tool\":\"Edit\",\"action\":\"Edit\",\"digest\":\"x\"}}", ts)
}

// --- newest_ts: peek the navigator's last activity (read-only, no cursor) ---

#[test]
fn newest_ts_returns_lexicographically_max_ts() {
    let stream = [obs("2026-06-06T08:00:00-0400"), obs("2026-06-06T08:05:00-0400"), obs("2026-06-06T08:02:00-0400")].join("\n");
    assert_eq!(newest_ts(&stream).as_deref(), Some("2026-06-06T08:05:00-0400"));
}

#[test]
fn newest_ts_none_on_empty_or_garbage() {
    assert_eq!(newest_ts(""), None);
    assert_eq!(newest_ts("not json\n\n"), None);
}

// --- silence_level: seconds-of-silence → escalation level (60/120/180) ---

#[test]
fn silence_level_thresholds() {
    assert_eq!(silence_level(0), Level::Active);
    assert_eq!(silence_level(59), Level::Active);
    assert_eq!(silence_level(60), Level::Warn);
    assert_eq!(silence_level(119), Level::Warn);
    assert_eq!(silence_level(120), Level::ReNudge);
    assert_eq!(silence_level(179), Level::ReNudge);
    assert_eq!(silence_level(180), Level::Stall);
    assert_eq!(silence_level(600), Level::Stall);
}

// --- escalation: fire ONCE per level, only on increase (no re-spam, no downgrade noise) ---

#[test]
fn escalation_fires_on_increase_only() {
    // active → warn fires the warn action
    assert_eq!(escalation(Level::Active, Level::Warn), Some(Level::Warn));
    // warn → renudge fires renudge
    assert_eq!(escalation(Level::Warn, Level::ReNudge), Some(Level::ReNudge));
    // renudge → stall fires stall
    assert_eq!(escalation(Level::ReNudge, Level::Stall), Some(Level::Stall));
}

#[test]
fn escalation_silent_when_level_unchanged() {
    assert_eq!(escalation(Level::Warn, Level::Warn), None, "same level => no re-spam");
    assert_eq!(escalation(Level::Stall, Level::Stall), None);
}

#[test]
fn escalation_silent_on_decrease_navigator_came_back() {
    // navigator resumed → level drops to Active; that's a reset, not an escalation event.
    assert_eq!(escalation(Level::Stall, Level::Active), None, "recovery is not an escalation");
    assert_eq!(escalation(Level::ReNudge, Level::Warn), None);
}

// --- the level ladder is ordered so increase/decrease is well-defined ---

#[test]
fn levels_are_ordered() {
    assert!(Level::Active < Level::Warn);
    assert!(Level::Warn < Level::ReNudge);
    assert!(Level::ReNudge < Level::Stall);
}
