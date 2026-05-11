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
/// from CHORUS_ROLE env, not a hardcoded "kade". Verifies end-to-end by
/// reading the fresh tail of ~/.chorus/chorus.log after firing the override
/// and asserting the new event name + role both appear, with a unique
/// CHORUS_ROLE marker to avoid races with other concurrent emits.
#[test]
fn override_emits_renamed_event_with_real_role() {
    let log_path: PathBuf = dirs_home()
        .join(".chorus")
        .join("chorus.log");

    // If chorus.log doesn't exist locally (fresh checkout, never run), skip
    // — the emit is best-effort and the runtime path doesn't require the file
    // to pre-exist; we only assert the contents when we can observe them.
    if !log_path.exists() {
        eprintln!("skip: {:?} not present — emit-attribution check requires live spine log", log_path);
        return;
    }

    let len_before = fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0);
    let unique_role = format!("test_attr_role_2899_{}", std::process::id());

    unsafe {
        std::env::set_var("CHORUS_ROLE", &unique_role);
        std::env::set_var("CHORUS_TEST_FORCE_FIX_CARD", "1");
    }
    let _ = is_fix_card();
    unsafe {
        std::env::remove_var("CHORUS_TEST_FORCE_FIX_CARD");
        std::env::remove_var("CHORUS_ROLE");
    }

    // Read only the tail beyond len_before to scope the search.
    let bytes = fs::read(&log_path).expect("read chorus.log");
    let tail: String = String::from_utf8_lossy(&bytes[len_before as usize..]).to_string();

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

fn dirs_home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("HOME env required for chorus.log path"))
}
