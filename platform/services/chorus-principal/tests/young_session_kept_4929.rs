// @domain: identity
// #4929 AC2 — the boot watcher never replaces a session younger than its silence
// window. 2026-11-08 boot: Kade's own-account session was replaced as "not spoken
// in 900s — MUTE" about 27s after it started.
use chorus_principal::{parse_etime, too_young_to_judge};

#[test]
fn a_27s_old_silent_session_is_kept() {
    assert!(too_young_to_judge(parse_etime("00:27"), 900));
}

#[test]
fn negative_proof_a_silent_session_past_the_window_is_judged() {
    assert!(!too_young_to_judge(parse_etime("15:01"), 900));
    assert!(!too_young_to_judge(parse_etime("1-02:03:04"), 900));
    // unreadable age is judged by the old rule, never silently exempt
    assert!(!too_young_to_judge(None, 900));
}

#[test]
fn etime_forms_parse() {
    assert_eq!(parse_etime("00:27"), Some(27));
    assert_eq!(parse_etime("01:02:03"), Some(3723));
    assert_eq!(parse_etime(" 2-00:00:01\n"), Some(172801));
    assert_eq!(parse_etime("bogus"), None);
}
