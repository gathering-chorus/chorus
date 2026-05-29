//! #3128 — always-wake delivery: the by-tty inject script must NOT gate on
//! frontmost-app focus, and must `activate` Terminal on a tty match so the
//! keystroke lands in the matched tab regardless of which app is frontmost.
//! These are the AC checks for #3128; they fail against the pre-#3128 script.

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
fn by_tty_script_activates_terminal() {
    let s = build_inject_by_tty_script("ttys003", "hello");
    assert!(
        s.contains("activate"),
        "always-wake: must activate Terminal so keystroke lands in the matched tab"
    );
}

#[test]
fn by_tty_script_still_routes_by_exact_tty() {
    let s = build_inject_by_tty_script("ttys042", "msg");
    assert!(s.contains(r#"(tty of t) is "ttys042""#));
}
