// @domain: identity
//! #4943 — every role session chorus-principal starts has mouse off, so Jeff can
//! select and copy text in a role pane with a plain drag. Real tmux on a private
//! socket whose global mouse is ON, so a session that skips the option reads `on`.
use chorus_principal::start_in_pane_args;
use std::process::Command;

struct Tmux { socket: String }

impl Tmux {
    fn new(tag: &str) -> Tmux {
        let t = Tmux { socket: format!("t4943-{}-{}", tag, std::process::id()) };
        // a holder session keeps the server up; global mouse on is the hostile default
        assert!(t.run(&["new-session", "-d", "-s", "holder", "sleep", "60"]), "start private tmux");
        assert!(t.run(&["set-option", "-g", "mouse", "on"]), "set global mouse on");
        t
    }
    fn run(&self, args: &[&str]) -> bool {
        Command::new("tmux").arg("-L").arg(&self.socket).args(args).status().map(|s| s.success()).unwrap_or(false)
    }
    fn run_owned(&self, args: &[String]) -> bool {
        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.run(&refs)
    }
    fn mouse(&self, session: &str) -> String {
        let o = Command::new("tmux").arg("-L").arg(&self.socket)
            .args(["show-options", "-t", session, "-v", "mouse"]).output().unwrap();
        let local = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if !local.is_empty() { return local; }
        // no session value: the session inherits the global one
        let o = Command::new("tmux").arg("-L").arg(&self.socket)
            .args(["show-options", "-g", "-v", "mouse"]).output().unwrap();
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    }
}

impl Drop for Tmux {
    fn drop(&mut self) { let _ = Command::new("tmux").arg("-L").arg(&self.socket).arg("kill-server").status(); }
}

#[test]
fn a_new_role_session_starts_with_mouse_off() {
    let t = Tmux::new("new");
    let dir = std::env::temp_dir();
    let args = start_in_pane_args("role-new", dir.to_str().unwrap(), "sleep 30", false);
    assert!(t.run_owned(&args), "start_in_pane_args runs: {:?}", args);
    assert_eq!(t.mouse("role-new"), "off");
}

#[test]
fn a_relaunch_into_an_existing_session_turns_mouse_off() {
    let t = Tmux::new("existing");
    let dir = std::env::temp_dir();
    assert!(t.run(&["new-session", "-d", "-s", "role-old", "sleep", "60"]));
    assert_eq!(t.mouse("role-old"), "on", "precondition: the existing session inherits mouse on");
    let args = start_in_pane_args("role-old", dir.to_str().unwrap(), "sleep 30", true);
    assert!(t.run_owned(&args), "start_in_pane_args runs: {:?}", args);
    assert_eq!(t.mouse("role-old"), "off");
}

/// Negative proof: the same check reads `on` for a session started without the
/// option, so a launch path that drops it fails the tests above.
#[test]
fn negative_proof_a_session_without_the_option_reads_on() {
    let t = Tmux::new("neg");
    assert!(t.run(&["new-session", "-d", "-s", "bare", "sleep", "30"]));
    assert_eq!(t.mouse("bare"), "on");
}
