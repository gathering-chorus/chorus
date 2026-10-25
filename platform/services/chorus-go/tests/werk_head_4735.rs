// @test-type: unit — a scratch git repo in a temp dir; git's own test switch stands in for a werk another Mac account owns.
// @domain: security
// @card: #4735
// @owner: silas
//! src/lib.rs werk_head: the go page reads a werk another account owns.
//! Jeff, 10-25 12:53, after a third safe.directory by hand: "so this is automation????"
use chorus_go::werk_head;
use std::process::Command;

struct Repo(std::path::PathBuf);
impl Drop for Repo {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}
impl Repo {
    fn path(&self) -> &std::path::Path { &self.0 }
}

fn repo(tag: &str) -> Repo {
    let d = Repo(std::env::temp_dir().join(format!("werk-head-4735-{tag}-{}", std::process::id())));
    let p = d.path().to_str().unwrap();
    for args in [vec!["init", "-q", p], vec!["-C", p, "-c", "user.email=t@t", "-c", "user.name=t", "commit", "-q", "--allow-empty", "-m", "x"]] {
        assert!(Command::new("git").args(&args).status().unwrap().success());
    }
    d
}

#[test]
fn a_werk_owned_by_another_account_is_read() {
    let d = repo("read");
    std::env::set_var("GIT_TEST_ASSUME_DIFFERENT_OWNER", "1");
    let head = werk_head(d.path().to_str().unwrap());
    std::env::remove_var("GIT_TEST_ASSUME_DIFFERENT_OWNER");
    assert_eq!(head.unwrap().len(), 40);
}

// NEGATIVE PROOF: the same repo read the old way (no per-call trust) is refused
// as dubious ownership, so the test above separates the two states; and a path
// that is not a werk still fails.
#[test]
fn negative_proof_without_the_trust_git_refuses_and_a_missing_werk_fails() {
    let d = repo("neg");
    let p = d.path().to_str().unwrap();
    let o = Command::new("git").env("GIT_TEST_ASSUME_DIFFERENT_OWNER", "1").args(["-C", p, "rev-parse", "HEAD"]).output().unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("dubious ownership"));
    assert!(werk_head("/nonexistent/silas-0").is_err());
}
