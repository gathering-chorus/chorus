//! #4446 reopen — prod audit 2026-10-07 15:40: seven always-on services never
//! restarted onto the land's code (bridge-subscriber ×3, share-guard ×2,
//! eventloop-probe, athena-make.staging), and the seven crates that include
//! shared/service_lifecycle.rs were never rebuilt. These tests prove the deploy
//! now finds both sets. Pure: fixtures in a temp dir, no launchctl, no HOME.
use werk_deploy::{daemon_refs, daemons_on_changed_code, imports_of, plist_is_daemon, shared_includers};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn tmp(tag: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("wd-4446-{}-{}", tag, n));
    fs::create_dir_all(&d).unwrap();
    d
}

fn krate(root: &PathBuf, name: &str, main: &str) {
    let d = root.join("platform/services").join(name);
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("Cargo.toml"), format!("[package]\nname = \"{}\"\n", name)).unwrap();
    fs::write(d.join("src/main.rs"), main).unwrap();
}

fn plist(args: &[&str], keepalive: &str) -> String {
    let a: String = args.iter().map(|x| format!("<string>{}</string>", x)).collect();
    format!("<plist><dict><key>ProgramArguments</key><array>{}</array>{}</dict></plist>", a, keepalive)
}
const ALWAYS: &str = "<key>KeepAlive</key><true/>";
const ON_FAIL: &str = "<key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>";
const NEVER: &str = "<key>KeepAlive</key><false/>";
const C: &str = "/Users/u/CascadeProjects/chorus";

#[test]
fn a_changed_shared_file_deploys_every_crate_that_includes_it() {
    let root = tmp("inc");
    krate(&root, "pair-heartbeat", "include!(\"../../shared/service_lifecycle.rs\");\nfn main(){}");
    krate(&root, "werk-test", "mod x { include!(\"../../shared/service_lifecycle.rs\"); }\nfn main(){}");
    krate(&root, "chorus-messaging", "fn main(){}");
    fs::create_dir_all(root.join("platform/services/shared")).unwrap();
    let diff = "platform/services/shared/service_lifecycle.rs\nplatform/scripts/x.sh\n";
    assert_eq!(shared_includers(&root, diff), vec!["pair-heartbeat".to_string(), "werk-test".to_string()]);
}

#[test]
fn negative_proof_a_land_that_touches_no_shared_file_adds_no_includer() {
    let root = tmp("inc-neg");
    krate(&root, "pair-heartbeat", "include!(\"../../shared/service_lifecycle.rs\");\nfn main(){}");
    assert!(shared_includers(&root, "platform/services/pair-heartbeat/src/main.rs\n").is_empty());
    // another shared file changed: only its own includers count
    assert!(shared_includers(&root, "platform/services/shared/scope_units.rs\n").is_empty());
}

#[test]
fn keepalive_true_or_a_condition_is_always_on_and_false_or_absent_is_not() {
    assert!(plist_is_daemon(&plist(&["/bin/x"], ALWAYS)));
    assert!(plist_is_daemon(&plist(&["/bin/x"], ON_FAIL)));
    assert!(!plist_is_daemon(&plist(&["/bin/x"], NEVER)));
    assert!(!plist_is_daemon(&plist(&["/bin/x"], "<key>StartInterval</key><integer>300</integer>")));
}

#[test]
fn refs_follow_a_wrapper_to_the_program_it_starts() {
    let wrapper = format!("{}/platform/scripts/share-guard-wrapper.sh", C);
    let worker = format!("{}/platform/scripts/chorus-eventloop-probe-worker.sh", C);
    let launch = "/Users/u/.chorus/bin/athena-make-launch.sh";
    let read = |p: &str| -> Option<String> {
        if p.ends_with("share-guard-wrapper.sh") { Some("# comment platform/scripts/ignored.py\nexec python3 \"$CHORUS_ROOT/platform/scripts/chorus-share-guard.py\"\n".into()) }
        else if p.ends_with("probe-worker.sh") { Some("WORKER_JS=\"${X:-$HOME/CascadeProjects/chorus/platform/api/dist/eventloop-probe.js}\"\n".into()) }
        else if p.ends_with("athena-make-launch.sh") { Some("exec \"$HOME/.chorus/bin/athena-make\" \"$@\"\n".into()) }
        else { None }
    };
    let node = format!("{}/platform/scripts/bridge-subscriber.js", C);
    assert!(daemon_refs_c(&plist(&["/usr/bin/node", &node, "kade"], ALWAYS), &read).contains(&"platform/scripts/bridge-subscriber.js".to_string()));
    let sg = daemon_refs_c(&plist(&[&wrapper], ON_FAIL), &read);
    assert!(sg.contains(&"platform/scripts/chorus-share-guard.py".to_string()), "{:?}", sg);
    assert!(!sg.contains(&"platform/scripts/ignored.py".to_string()), "a comment is not a program: {:?}", sg);
    assert!(daemon_refs_c(&plist(&["/bin/bash", &worker], ALWAYS), &read).contains(&"platform/api/src/eventloop-probe.ts".to_string()));
    assert!(daemon_refs_c(&plist(&[launch, "serve"], ALWAYS), &read).contains(&"bin:athena-make".to_string()));
}

fn agents() -> Vec<(String, String)> {
    let node = format!("{}/platform/scripts/bridge-subscriber.js", C);
    vec![
        ("com.chorus.bridge-subscriber-kade".into(), plist(&["/usr/bin/node", &node, "kade"], ALWAYS)),
        ("com.chorus.athena-make.staging".into(), plist(&["/Users/u/.chorus/bin/athena-make-launch.sh", "serve"], ALWAYS)),
        ("com.chorus.athena-make".into(), plist(&["/Users/u/.chorus/bin/athena-make-launch.sh", "serve"], ALWAYS)),
        // a scheduled job running the same script: its next run picks the code up
        ("com.chorus.bridge-subscriber-watchdog".into(), format!("{}<key>StartInterval</key><integer>60</integer>", plist(&["/usr/bin/node", &node], ""))),
    ]
}
fn read(p: &str) -> Option<String> {
    if p.ends_with("athena-make-launch.sh") { Some("exec \"$HOME/.chorus/bin/athena-make\" \"$@\"\n".into()) } else { None }
}

#[test]
fn a_land_restarts_the_always_on_services_running_its_changed_code() {
    let diff = "platform/scripts/bridge-subscriber.js\nplatform/services/athena-make/src/lib.rs\n";
    let bins = vec!["athena-make".to_string()];
    // athena-make's own label is restarted by its crate deploy; the staging copy is not
    let skip = vec!["com.chorus.athena-make".to_string()];
    assert_eq!(daemons_on_changed_code(&agents(), C, diff, &bins, &skip, &read),
        vec!["com.chorus.athena-make.staging".to_string(), "com.chorus.bridge-subscriber-kade".to_string()]);
}

#[test]
fn negative_proof_a_land_that_changes_none_of_their_code_restarts_nothing() {
    assert!(daemons_on_changed_code(&agents(), C, "docs/readme.md\nplatform/scripts/other.sh\n", &[], &[], &read).is_empty());
}

fn daemon_refs_c(plist: &str, read: &dyn Fn(&str) -> Option<String>) -> Vec<String> { daemon_refs(plist, C, read) }

#[test]
fn refs_follow_one_level_of_imports_to_the_helper_a_land_changed() {
    let worker = format!("{}/platform/scripts/chorus-eventloop-probe-worker.sh", C);
    let wrapper = format!("{}/platform/scripts/share-guard-wrapper.sh", C);
    let read = |p: &str| -> Option<String> {
        match p.trim_start_matches(C).trim_start_matches('/') {
            x if x.ends_with("probe-worker.sh") => Some("W=\"$HOME/CascadeProjects/chorus/platform/api/dist/eventloop-probe.js\"\n".into()),
            "platform/api/dist/eventloop-probe.js" => Some("const a = require(\"./eventloop-alert\");\nconst l = require(\"../../chorus-sdk/lifecycle/service-lifecycle\");\nconst c = require('node:child_process');\n".into()),
            "platform/api/dist/eventloop-alert.js" | "platform/chorus-sdk/lifecycle/service-lifecycle.js" => Some(String::new()),
            x if x.ends_with("share-guard-wrapper.sh") => Some("exec python3 \"$CHORUS_ROOT/platform/scripts/chorus-share-guard.py\"\n".into()),
            "platform/scripts/chorus-share-guard.py" => Some("import json\nfrom service_lifecycle import service_lifecycle\n".into()),
            "platform/scripts/lib/service_lifecycle.py" => Some(String::new()),
            _ => None,
        }
    };
    let ep = daemon_refs(&plist(&["/bin/bash", &worker], ALWAYS), C, &read);
    assert!(ep.contains(&"platform/chorus-sdk/lifecycle/service-lifecycle.js".to_string()), "{:?}", ep);
    assert!(ep.contains(&"platform/api/dist/eventloop-alert.js".to_string()), "{:?}", ep);
    let sg = daemon_refs(&plist(&[&wrapper], ON_FAIL), C, &read);
    assert!(sg.contains(&"platform/scripts/lib/service_lifecycle.py".to_string()), "{:?}", sg);
    // only files that exist: json is the standard library, not a repo file
    assert!(!sg.iter().any(|r| r.ends_with("json.py")), "{:?}", sg);
    let agents = vec![("com.chorus.eventloop-probe".to_string(), plist(&["/bin/bash", &worker], ALWAYS)),
                      ("com.chorus.share-guard".to_string(), plist(&[&wrapper], ON_FAIL))];
    assert_eq!(daemons_on_changed_code(&agents, C, "platform/chorus-sdk/lifecycle/service-lifecycle.js\n", &[], &[], &read),
        vec!["com.chorus.eventloop-probe".to_string()]);
}

#[test]
fn negative_proof_imports_stop_at_one_level_and_skip_packages() {
    let got = imports_of("platform/api/dist/x.js", "const a = require('express');\nimport b from \"node:fs\";\nconst c = require('./y');\n");
    assert!(got.iter().all(|g| g.starts_with("platform/api/dist/y")), "{:?}", got);
    assert!(!got.is_empty());
}
