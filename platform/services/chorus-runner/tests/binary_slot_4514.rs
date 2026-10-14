// @test-type: unit
// @domain: services
//! #4514 — a verb run through the runner uses the caller's werk bin slot first,
//! as the cicd pipeline's PATH does, so a card that changes a verb runs its own
//! build. NEGATIVE PROOF: another role's slot is never used.
use chorus_runner::server::binary_path;

#[test]
fn the_callers_slot_wins_and_another_roles_slot_is_never_used() {
    let base = std::env::temp_dir().join(format!("werk-base-4514-{}", std::process::id()));
    std::fs::create_dir_all(base.join("wren-bin")).unwrap();
    std::fs::write(base.join("wren-bin/werk-build"), "#!/bin/sh\n").unwrap();
    std::env::set_var("CHORUS_WERK_BASE", &base);
    assert_eq!(binary_path("werk-build", "wren"), base.join("wren-bin/werk-build").display().to_string());
    assert!(!binary_path("werk-build", "kade").contains("wren-bin"), "kade ran wren's slot");
    assert!(!binary_path("werk-build", "").contains("-bin/"), "no caller picked a slot");
}
