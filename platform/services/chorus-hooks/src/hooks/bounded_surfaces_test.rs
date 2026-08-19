//! #3424 red-first tests for the bounded-verb/skill guard (DEC-1674).
//!
//! The bound this enforces: a NEW werk verb crate or a NEW role skill may not
//! appear beside the others by accident. Widening the set stays cheap — one
//! deliberate edit to platform/config/canonical-surfaces.json, visible in the
//! diff — but a name that never entered that list is refused at edit time.
//!
//! Written before the module exists; `cargo test` is RED until it lands.

use crate::hooks::bounded_surfaces::*;

const LIST: &str = r#"{
  "verbs": ["werk-pull", "werk-commit", "werk-test"],
  "skills": ["pull", "demo", "wip"]
}"#;

fn set() -> CanonicalSurfaces {
    parse_surfaces(LIST).expect("fixture list parses")
}

#[test]
fn new_verb_crate_outside_the_list_is_refused() {
    let v = check_path(&set(), "platform/services/werk-teleport/src/main.rs");
    assert!(v.is_some(), "an off-list verb crate must be refused");
    let msg = v.unwrap();
    assert!(msg.contains("werk-teleport"), "names the offending surface: {msg}");
    assert!(msg.contains("canonical-surfaces.json"), "names the list to edit: {msg}");
}

#[test]
fn listed_verb_crate_passes() {
    assert!(check_path(&set(), "platform/services/werk-test/src/lib.rs").is_none());
}

#[test]
fn new_skill_outside_the_list_is_refused() {
    let v = check_path(&set(), "skills/teleport/SKILL.md");
    assert!(v.is_some(), "an off-list skill must be refused");
    assert!(v.unwrap().contains("teleport"));
}

#[test]
fn listed_skill_passes() {
    assert!(check_path(&set(), "skills/demo/SKILL.md").is_none());
}

#[test]
fn unrelated_paths_are_untouched() {
    // The guard must be silent everywhere it has no business — a noisy guard
    // gets disabled, and a disabled guard bounds nothing.
    assert!(check_path(&set(), "platform/api/src/server.ts").is_none());
    assert!(check_path(&set(), "designing/docs/whatever.html").is_none());
    assert!(check_path(&set(), "platform/services/chorus-hooks/src/main.rs").is_none());
}

#[test]
fn editing_the_list_itself_is_always_allowed() {
    // Widening the set is the sanctioned path — the guard must never block the
    // very edit it demands (that would make the bound unwidenable).
    assert!(check_path(&set(), "platform/config/canonical-surfaces.json").is_none());
}

#[test]
fn a_missing_or_broken_list_fails_loud_not_open() {
    // #3734 — the guarded condition is "list unreadable". A guard that silently
    // allows everything when its list is gone is a hollow gate; it must say so.
    assert!(parse_surfaces("not json at all").is_err());
    assert!(parse_surfaces(r#"{"verbs": []}"#).is_err(), "empty/absent skills is not a valid bound");
}
