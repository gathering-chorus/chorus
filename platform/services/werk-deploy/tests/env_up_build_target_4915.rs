// @test-type: unit — env_up's real build path with a recording command runner; no cargo, no launchd
// @domain: deploys
// @card: #4915 · owner: kade
// #4915 (Silas + Wren 14:00): a fresh werk's demo hooks never started. env_up built chorus-hooks
// with no CARGO_TARGET_DIR, so the workspace build landed in target/release, while the plist ran
// target/<account>/release (built_binary). Code review of e6353088c: test the build itself, not
// constants or a source grep, so these call build_service_dist_with on the real chorus-hooks service.
use werk_deploy::demo_env::{build_service_dist_with, env_services};

/// Run the real build path for chorus-hooks in a scratch werk; return what cargo was handed.
fn hooks_build_call(werk_root: &str, account: &str) -> (Option<String>, Vec<(String, String)>, String, Vec<String>) {
    let svc = env_services().into_iter().find(|s| s.name == "chorus-hooks").expect("env_up starts chorus-hooks");
    std::fs::create_dir_all(format!("{}/{}", werk_root.trim_end_matches('/'), svc.source_dir_rel)).unwrap();
    let mut seen = None;
    build_service_dist_with(&svc, werk_root, "kade", account, &mut |d, e, c, a| {
        seen = Some((d.map(String::from), e.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), c.to_string(), a.iter().map(|x| x.to_string()).collect()));
        Ok(String::new())
    }).expect("the build call succeeds with a runner that succeeds");
    seen.expect("the build ran a command")
}

fn scratch(name: &str) -> String {
    let d = std::env::temp_dir().join(format!("env-up-4915-{}-{}", name, std::process::id()));
    d.to_string_lossy().to_string()
}

#[test]
fn env_ups_hooks_build_targets_the_folder_the_plist_runs() {
    let werk = scratch("a");
    let (_, envs, cmd, args) = hooks_build_call(&werk, "jeffbridwell");
    assert_eq!(cmd, "cargo");
    assert!(args.iter().any(|a| a == "--release"), "{args:?}");
    let tgt = envs.iter().find(|(k, _)| k == "CARGO_TARGET_DIR").map(|(_, v)| v.clone())
        .expect("the build names its target folder (the old line handed cargo an empty environment)");
    assert_eq!(format!("{}/release/chorus-hooks", tgt), werk_deploy::built_binary(&werk, "jeffbridwell", "chorus-hooks"));
}

#[test]
fn a_trailing_slash_on_the_werk_does_not_move_the_folder() {
    let werk = scratch("b");
    let (_, envs, _, _) = hooks_build_call(&format!("{}/", werk), "chorus-kade");
    let tgt = envs.iter().find(|(k, _)| k == "CARGO_TARGET_DIR").map(|(_, v)| v.clone()).unwrap();
    assert_eq!(format!("{}/release/chorus-hooks", tgt), werk_deploy::built_binary(&werk, "chorus-kade", "chorus-hooks"));
}
