// @test-type: unit
// @domain: roles
//! #4520 — Abby gets the same per-prompt Chorus context the Claude roles get.
//! Her Gemini settings route BeforeAgent (Gemini's prompt-submit) through the
//! runtime adapter, which runs the same user-prompt-submit the Claude hook runs.

use chorus_agent::server::gemini_hooks_missing;

/// Her real settings file, found at run time (#4030: never env!() at compile).
fn abby_settings() -> String {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(&crate_dir).join("../../../roles/abby-normal/.gemini/settings.json");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn abbys_settings_route_every_prompt_through_the_adapter() {
    assert_eq!(gemini_hooks_missing(&abby_settings()), Vec::<String>::new());
}

#[test]
fn negative_proof_without_before_agent_the_check_names_it() {
    // The state Abby was in until today: tool guard and stop wired, prompt not.
    let before_4520 = r#"{"hooks":{
      "BeforeTool":[{"matcher":".*","hooks":[{"command":"chorus-hook-shim runtime-hook gemini BeforeTool"}]}],
      "AfterAgent":[{"matcher":".*","hooks":[{"command":"chorus-hook-shim runtime-hook gemini AfterAgent"}]}],
      "BeforeAgent":[{"matcher":".*","hooks":[{"command":"chorus-principal seen abby-normal"}]}]}}"#;
    assert_eq!(gemini_hooks_missing(before_4520), vec!["BeforeAgent".to_string()]);
}

#[test]
fn negative_proof_unreadable_settings_miss_every_event() {
    assert_eq!(gemini_hooks_missing("not json").len(), 3);
}
