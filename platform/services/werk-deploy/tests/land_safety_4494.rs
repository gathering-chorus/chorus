// @domain: builds — placed by chorus-crawl's domain rules (#4538)
//! #4494 reopen — two ways a v2 land hurt itself on 2026-10-10.
//! 22:22: #4494's land changed platform/config/dagu.yaml, the deploy restarted
//! com.chorus.dagu, and the land (with every other running run) was killed.
//! 23:39 (Wren, #4493): deploy-canonical sat 10+ minutes stat-ing 658,446
//! retirement backups while it looked for LaunchAgent plists.
use werk_deploy::{daemons_on_changed_code, find_plists, running_runs, split_orchestrator, ORCHESTRATOR};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn tmp(tag: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("wd-4494-{}-{}", tag, n));
    fs::create_dir_all(&d).unwrap();
    d
}

const C: &str = "/Users/u/CascadeProjects/chorus";

#[test]
fn a_land_that_changes_dagu_config_never_restarts_dagu_inline() {
    // NEGATIVE PROOF first: the #4446 selection DOES pick dagu for this diff —
    // the state that killed the land at 22:22.
    let dagu = format!(
        "<plist><dict><key>ProgramArguments</key><array><string>/opt/homebrew/bin/dagu</string><string>start-all</string><string>--config</string><string>{C}/platform/config/dagu.yaml</string></array><key>KeepAlive</key><true/></dict></plist>"
    );
    let picked = daemons_on_changed_code(&[(ORCHESTRATOR.to_string(), dagu)], C, "platform/config/dagu.yaml\n", &[], &[], &|_| None);
    assert_eq!(picked, vec![ORCHESTRATOR.to_string()], "the selection must see dagu's config change, or this test proves nothing");
    // and the split moves it out of the inline restarts
    let (now, later) = split_orchestrator(picked);
    assert!(now.is_empty(), "dagu would be restarted inside the land: {:?}", now);
    assert_eq!(later, vec![ORCHESTRATOR.to_string()]);
}

#[test]
fn other_services_still_restart_inline() {
    let (now, later) = split_orchestrator(vec!["com.chorus.api".into(), ORCHESTRATOR.into(), "com.chorus.pulse".into()]);
    assert_eq!(now, vec!["com.chorus.api".to_string(), "com.chorus.pulse".to_string()]);
    assert_eq!(later, vec![ORCHESTRATOR.to_string()]);
}

#[test]
fn idle_means_dagu_said_zero_running() {
    let busy = r#"{"queues":[{"name":"werk","runningCount":2},{"name":"werk-land","runningCount":1}]}"#;
    let idle = r#"{"queues":[{"name":"werk","runningCount":0},{"name":"werk-land","runningCount":0}]}"#;
    assert_eq!(running_runs(busy), Some(3));
    assert_eq!(running_runs(idle), Some(0));
    // NEGATIVE PROOF: an answer that is not dagu's queue list is never idle
    assert_eq!(running_runs(""), None);
    assert_eq!(running_runs("<html>502 Bad Gateway</html>"), None);
    assert_eq!(running_runs(r#"{"queues":[]}"#), None);
}

#[test]
fn plists_come_from_git_not_a_walk_of_the_backups() {
    let root = tmp("git");
    let git = |args: &[&str]| assert!(Command::new("git").args(args).current_dir(&root).output().unwrap().status.success());
    git(&["init", "-q"]);
    fs::create_dir_all(root.join("platform/launchd")).unwrap();
    fs::write(root.join("platform/launchd/com.chorus.api.plist"), "<plist/>").unwrap();
    git(&["add", "."]);
    // an untracked plist under the backups: a walk finds it, git does not
    fs::create_dir_all(root.join("platform/backups/graph-retirements/x")).unwrap();
    fs::write(root.join("platform/backups/graph-retirements/x/old.plist"), "<plist/>").unwrap();
    let found = find_plists(&root);
    assert_eq!(found, vec![root.join("platform/launchd/com.chorus.api.plist")]);
}

#[test]
fn a_dir_git_cannot_read_is_walked_without_the_backups() {
    let root = tmp("walk");
    fs::create_dir_all(root.join("a")).unwrap();
    fs::write(root.join("a/com.chorus.x.plist"), "<plist/>").unwrap();
    fs::create_dir_all(root.join("platform/backups")).unwrap();
    fs::write(root.join("platform/backups/stale.plist"), "<plist/>").unwrap();
    let found = find_plists(&root);
    assert_eq!(found, vec![root.join("a/com.chorus.x.plist")]);
}
