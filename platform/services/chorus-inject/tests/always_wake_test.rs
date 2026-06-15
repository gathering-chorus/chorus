//! by-tty delivery (nudge, Wren-owned): the inject script must NOT gate on
//! frontmost-app focus. #3128 originally `activate`d Terminal on a tty match;
//! #3352 (Jeff 2026-06-11) superseded that with `do script` into the matched TAB
//! — focus-independent, no focus theft. These assert the shipped #3352 behavior.

use chorus_inject::build_inject_by_tty_script;

#[test]
fn no_focus_gate_in_by_tty_script() {
    let s = build_inject_by_tty_script("ttys003", "hello");
    assert!(
        !s.contains("focus-gate-miss"),
        "always-wake: focus-gate-miss sentinel must be gone"
    );
    assert!(
        !s.contains("frontApp"),
        "always-wake: frontmost-app check must be gone"
    );
}

#[test]
fn by_tty_script_writes_into_matched_tab_without_focus_theft() {
    // #3352 (Jeff 2026-06-11) SUPERSEDED #3128's activate-and-keystroke: the by-tty
    // path now writes directly into the matched Terminal TAB via `do script`, which
    // is focus-independent and must NOT `activate`/steal focus (the old path sprayed
    // into Jeff's active window every demo). This test was stale-red on main — it
    // asserted the removed `activate`. Corrected to the shipped #3352 behavior.
    let s = build_inject_by_tty_script("ttys003", "hello");
    assert!(!s.contains("activate"), "#3352: must NOT activate / steal focus");
    assert!(s.contains("do script"), "#3352: writes into the matched tab via do script");
}

#[test]
fn by_tty_script_still_routes_by_exact_tty() {
    let s = build_inject_by_tty_script("ttys042", "msg");
    assert!(s.contains(r#"(tty of t) is "ttys042""#));
}
