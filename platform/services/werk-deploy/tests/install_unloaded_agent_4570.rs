// @test-type: unit — temp dirs and closures stand in for launchctl; no launchd touched.
// @domain: services
//! #4570 — prod deploy installs a LaunchAgent nobody loaded (the 4th land that failed
//! because deploy only kickstarted), and a failed first deploy removes it again.
use std::cell::RefCell;
use std::fs;
use std::path::Path;
use werk_deploy::{ensure_agent_installed, uninstall_agent, AgentInstall};

struct Tmp(std::path::PathBuf);
impl Drop for Tmp { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

fn tree() -> (Tmp, String, std::path::PathBuf) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let t = Tmp(std::env::temp_dir().join(format!("wd4570-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst))));
    let root = t.0.join("chorus");
    fs::create_dir_all(root.join("platform/scripts/launchagents-canonical")).unwrap();
    let agents = t.0.join("LaunchAgents");
    fs::create_dir_all(&agents).unwrap();
    (t, root.to_string_lossy().into_owned(), agents)
}

#[test]
fn an_unloaded_agent_with_a_canonical_plist_is_copied_and_bootstrapped() {
    let (_t, root, agents) = tree();
    fs::write(format!("{}/platform/scripts/launchagents-canonical/com.chorus.x.plist", root), "<plist/>").unwrap();
    let booted = RefCell::new(Vec::new());
    let r = ensure_agent_installed("com.chorus.x", &root, &agents, &|_| false,
        &|p: &Path| { booted.borrow_mut().push(p.to_path_buf()); Ok(String::new()) }).unwrap();
    assert_eq!(r, AgentInstall::Installed);
    assert_eq!(fs::read_to_string(agents.join("com.chorus.x.plist")).unwrap(), "<plist/>");
    assert_eq!(booted.borrow().as_slice(), &[agents.join("com.chorus.x.plist")]);
}

#[test]
fn a_loaded_agent_is_left_alone() {
    let (_t, root, agents) = tree();
    fs::write(format!("{}/platform/scripts/launchagents-canonical/com.chorus.x.plist", root), "<plist/>").unwrap();
    let r = ensure_agent_installed("com.chorus.x", &root, &agents, &|_| true,
        &|_: &Path| panic!("bootstrapped a loaded agent")).unwrap();
    assert_eq!(r, AgentInstall::Loaded);
    assert!(!agents.join("com.chorus.x.plist").exists());
}

#[test]
fn negative_proof_no_canonical_plist_installs_nothing() {
    let (_t, root, agents) = tree();
    let r = ensure_agent_installed("com.chorus.y", &root, &agents, &|_| false,
        &|_: &Path| panic!("bootstrapped with no plist")).unwrap();
    assert!(matches!(r, AgentInstall::Missing(_)));
    assert!(!agents.join("com.chorus.y.plist").exists());
}

#[test]
fn uninstall_boots_out_and_removes_the_copied_plist() {
    let (_t, _root, agents) = tree();
    fs::write(agents.join("com.chorus.x.plist"), "<plist/>").unwrap();
    let out = RefCell::new(Vec::new());
    uninstall_agent("com.chorus.x", &agents, &|l: &str| { out.borrow_mut().push(l.to_string()); Ok(String::new()) }).unwrap();
    assert_eq!(out.borrow().as_slice(), &["com.chorus.x".to_string()]);
    assert!(!agents.join("com.chorus.x.plist").exists());
}
