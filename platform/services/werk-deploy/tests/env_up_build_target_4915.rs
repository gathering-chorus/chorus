// @test-type: unit — the cargo call env_up makes for a Rust service; no build, no launchd
// @domain: deploys
// @card: #4915 · owner: kade
// #4915 (Silas + Wren 14:00): a fresh werk's demo hooks never started. env_up built chorus-hooks
// with no CARGO_TARGET_DIR, so the workspace build landed in target/release, while the plist ran
// target/<account>/release (built_binary). The build's folder and the plist's must be one.
use werk_deploy::demo_env::rust_build_command;

fn target_dir_of(envs: &[(String, String)]) -> Option<String> {
    envs.iter().find(|(k, _)| k == "CARGO_TARGET_DIR").map(|(_, v)| v.clone())
}

#[test]
fn env_up_builds_where_the_plist_reads() {
    let (envs, args) = rust_build_command("/w/kade-4915/", "jeffbridwell");
    let tgt = target_dir_of(&envs).expect("the build names its target folder");
    assert_eq!(format!("{}/release/chorus-hooks", tgt), werk_deploy::built_binary("/w/kade-4915", "jeffbridwell", "chorus-hooks"));
    assert!(args.iter().any(|a| a == "--release"), "{args:?}");
}

// NEGATIVE PROOF: the old call (no CARGO_TARGET_DIR) builds to cargo's default, target/release,
// which is not where the plist reads; a build without the variable is caught here
#[test]
fn a_build_without_the_target_folder_is_not_the_plists() {
    let old: Vec<(String, String)> = vec![];
    assert_eq!(target_dir_of(&old), None);
    let default = "/w/kade-4915/target/release/chorus-hooks";
    assert_ne!(default, werk_deploy::built_binary("/w/kade-4915", "jeffbridwell", "chorus-hooks"));
}

// the Rust build in env_up goes through rust_build_command, so the folder above is the one used
// (the old line called cargo with an empty environment)
#[test]
fn env_ups_rust_build_uses_the_command() {
    let src = include_str!("../src/demo_env.rs");
    let rust = &src[src.find("ProgramArgsTemplate::Rust { binary: _ } =>").expect("the Rust build branch")..];
    let branch = &rust[..rust.find("ProgramArgsTemplate::WerkBinBare").expect("the next branch")];
    assert!(branch.contains("rust_build_command("), "the Rust build must take its env from rust_build_command:\n{branch}");
    assert!(!branch.contains("&[], \"cargo\""), "cargo with an empty environment is the old line:\n{branch}");
}
