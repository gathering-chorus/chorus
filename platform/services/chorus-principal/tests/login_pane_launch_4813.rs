// @domain: identity
//! #4813 — login starts the runtime as the pane's own process, never by typing
//! into a live shell. On 10-30 "@borg-queen u there?" was typed into BQ's pane
//! mid-launch, the shell sat at `dquote>`, and three logins in a row had their
//! send-keys launch line swallowed. These run real tmux on a private socket.
use chorus_principal::{pane_tail_summary, start_in_pane_args};
use std::process::Command;
use std::time::{Duration, Instant};

struct Tmux { socket: String }

impl Tmux {
    fn new(tag: &str) -> Tmux {
        Tmux { socket: format!("t4813-{}-{}", tag, std::process::id()) }
    }
    fn run(&self, args: &[&str]) -> bool {
        Command::new("tmux").arg("-L").arg(&self.socket).args(args).status().map(|s| s.success()).unwrap_or(false)
    }
    fn run_owned(&self, args: &[String]) -> bool {
        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        self.run(&refs)
    }
    fn capture(&self, session: &str) -> String {
        let o = Command::new("tmux").arg("-L").arg(&self.socket).args(["capture-pane", "-p", "-S", "-20", "-t", session]).output().unwrap();
        String::from_utf8_lossy(&o.stdout).to_string()
    }
}

impl Drop for Tmux {
    fn drop(&mut self) { let _ = Command::new("tmux").arg("-L").arg(&self.socket).arg("kill-server").status(); }
}

fn wait_for(path: &std::path::Path, secs: u64) -> bool {
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if path.exists() { return true; }
        std::thread::sleep(Duration::from_millis(100));
    }
    path.exists()
}

/// A pane whose shell is stuck at a `dquote>` continuation, as on 10-30.
fn stuck_pane(t: &Tmux, session: &str, dir: &str) {
    assert!(t.run(&["new-session", "-d", "-s", session, "-c", dir, "sh"]));
    std::thread::sleep(Duration::from_millis(300));
    assert!(t.run(&["send-keys", "-t", session, "echo \"@borg-queen u there?", "Enter"]));
    std::thread::sleep(Duration::from_millis(300));
}

#[test]
fn ac1_a_new_session_runs_the_launch_as_its_own_process() {
    let t = Tmux::new("new");
    let dir = tempfile_dir("new");
    let marker = dir.join("started");
    let launch = format!("touch '{}'; sleep 30", marker.display());
    assert!(t.run_owned(&start_in_pane_args("chorus-x", dir.to_str().unwrap(), &launch, false)));
    assert!(wait_for(&marker, 5), "the runtime never started");
}

#[test]
fn ac2_negative_proof_send_keys_into_a_dquote_pane_is_swallowed() {
    // The old way, against the 10-30 state: the launch line becomes more of the
    // unfinished echo and never runs. This is the red the fix exists to end.
    let t = Tmux::new("old");
    let dir = tempfile_dir("old");
    let marker = dir.join("started");
    stuck_pane(&t, "chorus-x", dir.to_str().unwrap());
    let launch = format!("touch '{}'", marker.display());
    assert!(t.run(&["send-keys", "-t", "chorus-x", &launch, "Enter"]));
    assert!(!wait_for(&marker, 2), "send-keys into a dquote> pane must not run the launch");
}

#[test]
fn ac2_a_dquote_pane_is_reset_and_the_runtime_starts() {
    let t = Tmux::new("fix");
    let dir = tempfile_dir("fix");
    let marker = dir.join("started");
    stuck_pane(&t, "chorus-x", dir.to_str().unwrap());
    let launch = format!("touch '{}'; sleep 30", marker.display());
    assert!(t.run_owned(&start_in_pane_args("chorus-x", dir.to_str().unwrap(), &launch, true)));
    assert!(wait_for(&marker, 5), "respawn must replace the stuck shell and start the runtime");
}

#[test]
fn ac3_a_launch_that_dies_leaves_its_last_lines_readable() {
    let t = Tmux::new("dead");
    let dir = tempfile_dir("dead");
    let launch = "echo 'agy: not signed in, run agy login'; exit 3";
    assert!(t.run_owned(&start_in_pane_args("chorus-x", dir.to_str().unwrap(), launch, false)));
    std::thread::sleep(Duration::from_secs(1));
    let tail = pane_tail_summary(&t.capture("chorus-x"));
    assert!(tail.contains("not signed in"), "the reason must survive the exit: {tail}");
}

#[test]
fn ac3_an_empty_pane_says_so_rather_than_nothing() {
    assert_eq!(pane_tail_summary("\n\n  \n"), "the pane printed nothing");
    assert_eq!(pane_tail_summary("a\nb\n\nc\n"), "a | b | c");
}

fn tempfile_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("t4813-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn ac3_negative_proof_remain_on_exit_set_after_the_launch_loses_the_reason() {
    // The first cut of this fix set remain-on-exit after starting the launch; a
    // launch that dies at once took its pane, and its reason, with it.
    let t = Tmux::new("late");
    let dir = tempfile_dir("late");
    let launch = "echo 'agy: not signed in, run agy login'; exit 3";
    let _ = t.run(&["new-session", "-d", "-s", "chorus-x", "-c", dir.to_str().unwrap(), "bash", "-lc", launch]);
    std::thread::sleep(Duration::from_secs(1));
    let _ = t.run(&["set-option", "-t", "chorus-x", "remain-on-exit", "on"]);
    assert!(!pane_tail_summary(&t.capture("chorus-x")).contains("not signed in"), "a late remain-on-exit must not be able to keep the reason");
}
