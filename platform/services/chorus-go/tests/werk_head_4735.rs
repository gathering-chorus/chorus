// @test-type: unit — scratch git repos in a temp dir: a plain repo, a linked worktree, packed refs, and a werk whose config tries to run a program
// @domain: security
// @card: #4735 #4710
// @owner: wren
//! src/lib.rs werk_head: the go page reads the round a role's werk holds. #4735 (Jeff 10-25
//! 12:53, "so this is automation????") needed git to trust werks other accounts own. #4710: the
//! lander must run no program a role can write, and a role's .git/config can make git run one,
//! so werk_head reads git's files and never runs git.
use chorus_go::werk_head;
use std::process::Command;

struct Repo(std::path::PathBuf);
impl Drop for Repo {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

fn git(args: &[&str]) {
    let mut c = Command::new("git");
    for (k, _) in std::env::vars().filter(|(k, _)| k == "GIT_CONFIG_COUNT" || k.starts_with("GIT_CONFIG_KEY_") || k.starts_with("GIT_CONFIG_VALUE_")) { c.env_remove(k); }
    assert!(c.args(["-c", "user.email=t@t", "-c", "user.name=t"]).args(args).status().unwrap().success(), "git {args:?}");
}
fn out(args: &[&str]) -> String {
    String::from_utf8_lossy(&Command::new("git").args(args).output().unwrap().stdout).trim().to_string()
}
fn repo(tag: &str) -> Repo {
    let d = Repo(std::env::temp_dir().join(format!("werk-head-4710-{tag}-{}", std::process::id())));
    let _ = std::fs::remove_dir_all(&d.0);
    let p = d.0.to_str().unwrap();
    git(&["init", "-q", "-b", "main", p]);
    git(&["-C", p, "commit", "-q", "--allow-empty", "-m", "x"]);
    d
}

#[test]
fn a_plain_repo_and_a_linked_worktree_read_their_head() {
    let d = repo("plain");
    let p = d.0.to_str().unwrap();
    assert_eq!(werk_head(p).unwrap(), out(&["-C", p, "rev-parse", "HEAD"]));
    let wt = d.0.with_extension("wt");
    let _ = std::fs::remove_dir_all(&wt);
    git(&["-C", p, "worktree", "add", "-q", "-b", "wren/9", wt.to_str().unwrap()]);
    git(&["-C", wt.to_str().unwrap(), "commit", "-q", "--allow-empty", "-m", "card work"]);
    assert_eq!(werk_head(wt.to_str().unwrap()).unwrap(), out(&["-C", wt.to_str().unwrap(), "rev-parse", "HEAD"]));
    // packed refs: the branch ref lives only in packed-refs after a pack
    git(&["-C", p, "pack-refs", "--all"]);
    assert_eq!(werk_head(wt.to_str().unwrap()).unwrap(), out(&["-C", wt.to_str().unwrap(), "rev-parse", "HEAD"]));
    let _ = std::fs::remove_dir_all(&wt);
}

// NEGATIVE PROOF: a werk whose config would make git run a program (core.fsmonitor) is read
// and the program never runs; a path that is not a werk fails.
#[test]
fn a_werk_config_that_would_run_a_program_runs_nothing() {
    let d = repo("fsmon");
    let p = d.0.to_str().unwrap();
    let ran = d.0.join("ran");
    let hook = d.0.join("mon.sh");
    std::fs::write(&hook, format!("#!/bin/sh\ntouch {}\n", ran.display())).unwrap();
    { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap(); }
    git(&["-C", p, "config", "core.fsmonitor", hook.to_str().unwrap()]);
    // the planted program does run under git itself (so this proof can tell the two apart)
    let _ = Command::new("git").args(["-C", p, "status", "--porcelain"]).output();
    assert!(ran.exists(), "git status did not run the fsmonitor: the negative cannot separate the states");
    std::fs::remove_file(&ran).unwrap();
    assert_eq!(werk_head(p).unwrap().len(), 40);
    assert!(!ran.exists(), "werk_head ran the werk's configured program");
    assert!(werk_head("/nonexistent/silas-0").is_err());
}
