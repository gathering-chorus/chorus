// @domain: code — placed by chorus-crawl's domain rules (#4538)
//! #4535 — a role on its own Mac account runs the team's installed tools.
//! Abby's HOME (/Users/chorus-abby-normal) has no .chorus/bin, so the runner's
//! $HOME/.chorus/bin lookup found nothing for her. The shared state dir comes
//! from CHORUS_STATE_DIR (chorus-env-setup exports the checkout owner's).
use chorus_runner::server::binary_path;

#[test]
fn a_role_with_an_empty_home_finds_the_shared_install() {
    let root = std::env::temp_dir().join(format!("state-4535-{}", std::process::id()));
    let shared = root.join("owner/.chorus");
    let empty_home = root.join("abby");
    std::fs::create_dir_all(shared.join("bin")).unwrap();
    std::fs::create_dir_all(&empty_home).unwrap();
    std::fs::write(shared.join("bin/probe-tool-4535"), "").unwrap();
    std::env::remove_var("CHORUS_WERK_BASE");
    std::env::set_var("HOME", &empty_home);

    // NEGATIVE PROOF: with no shared state dir named, the empty HOME finds nothing
    std::env::remove_var("CHORUS_STATE_DIR");
    assert_eq!(binary_path("probe-tool-4535", "abby-normal"), "probe-tool-4535");

    std::env::set_var("CHORUS_STATE_DIR", &shared);
    assert_eq!(binary_path("probe-tool-4535", "abby-normal"), shared.join("bin/probe-tool-4535").to_str().unwrap());

    let _ = std::fs::remove_dir_all(&root);
}
