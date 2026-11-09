// @test-type: integration — real tmux on a private socket
// @domain: identity
//! #4934 AC3 — Jeff 2026-11-09 08:53: "i still dont like the tmux pane dead when i
//! close sessions for all of u". Login sets remain-on-exit BEFORE the launch (#4813
//! AC3: a launch that dies at once keeps its last lines readable) and never cleared
//! it, so a runtime that started fine and later quit left a dead pane behind.
//! Once the runtime has settled, the pane is set to close when it exits.
//!
//! NEGATIVE PROOF (#3734): without the settled step, the session outlives the
//! runtime as a dead pane, and `a_runtime_that_quits_after_settling_leaves_no_pane`
//! goes red. The #4813 behaviour is kept: a launch that dies before settling still
//! leaves its pane readable.
use chorus_principal::{settled_pane_args, start_in_pane_args};
use std::process::Command;
use std::time::{Duration, Instant};

struct Tmux { socket: String }

impl Tmux {
    fn new(tag: &str) -> Tmux { Tmux { socket: format!("t4934-{}-{}", tag, std::process::id()) } }
    fn run(&self, args: &[String]) -> bool {
        Command::new("tmux").arg("-L").arg(&self.socket).args(args).status().map(|s| s.success()).unwrap_or(false)
    }
    fn has(&self, session: &str) -> bool {
        Command::new("tmux").arg("-L").arg(&self.socket).args(["has-session", "-t", session])
            .stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
    }
}

impl Drop for Tmux {
    fn drop(&mut self) { let _ = Command::new("tmux").arg("-L").arg(&self.socket).arg("kill-server").status(); }
}

fn dir(tag: &str) -> String {
    let d = std::env::temp_dir().join(format!("t4934-{}-{}", tag, std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d.to_string_lossy().to_string()
}

fn gone_within(t: &Tmux, session: &str, secs: u64) -> bool {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if !t.has(session) { return true; }
        std::thread::sleep(Duration::from_millis(100));
    }
    !t.has(session)
}

#[test]
fn a_runtime_that_quits_after_settling_leaves_no_pane() {
    let t = Tmux::new("quit");
    assert!(t.run(&start_in_pane_args("chorus-x", &dir("quit"), "sleep 2", false)));
    assert!(t.run(&settled_pane_args("chorus-x")), "the settled step runs");
    assert!(gone_within(&t, "chorus-x", 8), "the runtime quit and its dead pane was left behind");
}

#[test]
fn control_a_launch_that_dies_before_settling_keeps_its_pane_readable() {
    let t = Tmux::new("dies");
    assert!(t.run(&start_in_pane_args("chorus-x", &dir("dies"), "echo not signed in; exit 1", false)));
    std::thread::sleep(Duration::from_millis(1500));
    assert!(t.has("chorus-x"), "#4813 AC3: a launch that dies at once keeps its last lines");
}

#[test]
fn the_settled_step_names_only_the_roles_session() {
    let a = settled_pane_args("chorus-kade");
    assert_eq!(a, vec!["set-option", "-t", "chorus-kade", "remain-on-exit", "off"]);
}
