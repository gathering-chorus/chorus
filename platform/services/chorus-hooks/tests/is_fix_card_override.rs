//! #2644 AC2 regression — is_fix_card() must be deterministic for the gate
//! smoke check, independent of live chorus-api board state.
//!
//! The smoke (run_gate_smoke in session_init_gate.rs) verifies log_first_gate
//! and memory_gate block on a fix-card edit with no log/synthesis evidence.
//! Both gates check is_defect_fix() → is_fix_card(). Pre-fix, is_fix_card()
//! consulted only the live board via curl, so smoke passed only when some
//! role had a type:fix WIP card. AC2 requires deterministic behavior.
//!
//! #2899 — when the override fires, the spine emit uses the canonical event
//! name `gate.test_override.checked` (not the misleading `gate.bypass.*`)
//! and attributes the actual caller's role from CHORUS_ROLE / DEPLOY_ROLE
//! env, never a hardcoded "kade".

use chorus_hooks::is_fix_card;
use std::fs;
use std::path::PathBuf;

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

/// #2899 — spine emit uses the renamed event and attributes the caller's role
/// from CHORUS_ROLE env, not a hardcoded "kade". #3615: pre-membrane this test
/// asserted on the LIVE ~/.chorus/chorus.log — writing a test event onto the
/// production spine every run, the exact class the membrane refuses. It now
/// brings its own world (#3528): CHORUS_LOG_FILE → a per-test tempdir spine.
#[test]
fn override_emits_renamed_event_with_real_role() {
    let dir = std::env::temp_dir().join(format!("is_fix_card_2899_{}", std::process::id()));
    fs::create_dir_all(&dir).expect("create test spine dir");
    let log_path: PathBuf = dir.join("spine.log");
    let unique_role = format!("test_attr_role_2899_{}", std::process::id());

    // SAFETY: this binary runs single-threaded (RUST_TEST_THREADS=1).
    unsafe {
        std::env::set_var("CHORUS_LOG_FILE", &log_path);
        std::env::set_var("CHORUS_ROLE", &unique_role);
        std::env::set_var("CHORUS_TEST_FORCE_FIX_CARD", "1");
    }
    let _ = is_fix_card();
    unsafe {
        std::env::remove_var("CHORUS_TEST_FORCE_FIX_CARD");
        std::env::remove_var("CHORUS_ROLE");
        std::env::remove_var("CHORUS_LOG_FILE");
    }

    let tail = fs::read_to_string(&log_path).expect("read test spine");
    let _ = fs::remove_dir_all(&dir);

    assert!(
        tail.contains("gate.test_override.checked"),
        "expected new event name in spine tail; got: {}", tail
    );
    assert!(
        tail.contains(&unique_role),
        "expected role={} in spine tail (no hardcoded 'kade'); got: {}", unique_role, tail
    );
    assert!(
        !tail.contains("gate.bypass.fix_card_override"),
        "old event name must not be re-emitted after #2899; got: {}", tail
    );
}
