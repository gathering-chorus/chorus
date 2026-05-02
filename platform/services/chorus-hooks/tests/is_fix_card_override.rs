//! #2644 AC2 regression — is_fix_card() must be deterministic for the gate
//! smoke check, independent of live chorus-api board state.
//!
//! The smoke (run_gate_smoke in session_init_gate.rs) verifies log_first_gate
//! and memory_gate block on a fix-card edit with no log/synthesis evidence.
//! Both gates check is_defect_fix() → is_fix_card(). Pre-fix, is_fix_card()
//! consulted only the live board via curl, so smoke passed only when some
//! role had a type:fix WIP card. AC2 requires deterministic behavior.

use chorus_hooks::is_fix_card;

#[test]
fn override_one_forces_true() {
    // SAFETY: tests serial within this binary.
    unsafe { std::env::set_var("CHORUS_TEST_FORCE_FIX_CARD", "1"); }
    assert!(is_fix_card(), "CHORUS_TEST_FORCE_FIX_CARD=1 must force true");
    unsafe { std::env::remove_var("CHORUS_TEST_FORCE_FIX_CARD"); }
}

#[test]
fn override_zero_forces_false() {
    unsafe { std::env::set_var("CHORUS_TEST_FORCE_FIX_CARD", "0"); }
    assert!(!is_fix_card(), "CHORUS_TEST_FORCE_FIX_CARD=0 must force false");
    unsafe { std::env::remove_var("CHORUS_TEST_FORCE_FIX_CARD"); }
}

#[test]
fn override_true_string_forces_true() {
    unsafe { std::env::set_var("CHORUS_TEST_FORCE_FIX_CARD", "true"); }
    assert!(is_fix_card(), "CHORUS_TEST_FORCE_FIX_CARD=true must force true");
    unsafe { std::env::remove_var("CHORUS_TEST_FORCE_FIX_CARD"); }
}
