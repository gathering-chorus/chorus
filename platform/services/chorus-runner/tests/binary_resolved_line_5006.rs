// @test-type: unit
// @domain: services
//! #5006 AC3 — every binary the runner resolves is one runner.binary.resolved
//! line naming the path, why it was chosen and whether the caller opted into a
//! slot, so a land that ran a slot binary is visible on the spine.
//! NEGATIVE PROOF: the slot and the installed choice carry different reasons.
use chorus_runner::server::{resolution_line, resolve_binary};
use std::os::unix::fs::PermissionsExt;

fn world() -> (String, String) {
    let root = std::env::temp_dir().join(format!("runner-5006-line-{}", std::process::id()));
    let base = root.join("werk");
    let state = root.join("state");
    std::fs::create_dir_all(base.join("wren-bin")).unwrap();
    std::fs::create_dir_all(state.join("bin")).unwrap();
    for f in [base.join("wren-bin/athena-deploy"), state.join("bin/athena-deploy")] {
        std::fs::write(&f, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::set_permissions(base.join("wren-bin"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("CHORUS_WERK_BASE", &base);
    std::env::set_var("CHORUS_STATE_DIR", &state);
    (base.join("wren-bin/athena-deploy").display().to_string(), state.join("bin/athena-deploy").display().to_string())
}

#[test]
fn every_resolution_says_which_path_and_why() {
    let (w_slot, w_installed) = world();
    let (slot, why_slot) = resolve_binary("athena-deploy", "wren", true).unwrap();
    let (inst, why_inst) = resolve_binary("athena-deploy", "wren", false).unwrap();
    assert_eq!((slot.as_str(), inst.as_str()), (w_slot.as_str(), w_installed.as_str()));
    for (path, why, opt) in [(&slot, why_slot, true), (&inst, why_inst, false)] {
        let line: serde_json::Value = serde_json::from_str(&resolution_line("athena-deploy", "wren", path, why, opt, "t5006")).unwrap();
        assert_eq!(line["event"], "runner.binary.resolved");
        assert_eq!(line["path"], path.as_str());
        assert_eq!(line["why"], why);
        assert_eq!(line["slot_opt_in"], opt);
        assert_eq!(line["trace"], "t5006");
    }
    assert_ne!(why_slot, why_inst, "the two choices must be told apart in the line");
}


/// The call path: what prepare runs and logs. A binary tool gets its line; an
/// http tool (its binary is a route) gets none and resolves nothing.
/// NEGATIVE PROOF: flipping which kind logs fails both halves.
#[test]
fn a_binary_call_logs_its_line_and_an_http_call_logs_none() {
    use chorus_runner::server::resolve_for_call;
    let (_slot, installed) = world();
    std::env::remove_var("CHORUS_USE_SLOT");
    let (path, line) = resolve_for_call("athena-deploy", "athena-deploy", "wren", false, "t5006c").unwrap();
    assert_eq!(path, installed);
    let line: serde_json::Value = serde_json::from_str(&line.expect("a binary call logs its resolution")).unwrap();
    assert_eq!((line["event"].as_str(), line["path"].as_str(), line["slot_opt_in"].as_bool()), (Some("runner.binary.resolved"), Some(installed.as_str()), Some(false)));
    let (route, none) = resolve_for_call("cards-get", "GET http://127.0.0.1:9/x", "wren", true, "t5006c").unwrap();
    assert_eq!(route, "GET http://127.0.0.1:9/x");
    assert!(none.is_none(), "an http call logged a binary resolution");
}
