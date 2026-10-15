//! #4535 — a role on its own Mac account writes the team's spine, not a
//! private one. Abby's events went to /Users/chorus-abby-normal/.chorus/chorus.log
//! (1.7 MB on 2026-10-15, never in Loki).
use chorus_hooks::shared::state_paths::{chorus_log_file, chorus_state_dir};

#[test]
fn the_spine_follows_the_shared_state_dir_not_home() {
    std::env::remove_var("CHORUS_LOG_FILE");
    std::env::set_var("HOME", "/Users/chorus-abby-normal");
    // NEGATIVE PROOF: with no shared state dir named, the private log is what you get
    std::env::remove_var("CHORUS_STATE_DIR");
    assert_eq!(chorus_log_file(), "/Users/chorus-abby-normal/.chorus/chorus.log");

    std::env::set_var("CHORUS_STATE_DIR", "/Users/jeffbridwell/.chorus");
    assert_eq!(chorus_state_dir(), "/Users/jeffbridwell/.chorus");
    assert_eq!(chorus_log_file(), "/Users/jeffbridwell/.chorus/chorus.log");
}
