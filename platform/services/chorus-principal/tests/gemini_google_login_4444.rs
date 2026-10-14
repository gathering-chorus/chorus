// @test-type: unit
// @domain: identity
//! #4444 reopen 1 — Jeff, 2026-10-14 06:30: "i did all the steps and still get
//! denied". Abby's account said Google login, but the repo's workspace settings
//! said gemini-api-key and won, so Gemini asked for a key on every start.

use chorus_principal::rows::gemini_auth_is_google;

#[test]
fn abbys_workspace_settings_sign_in_with_google() {
    let root = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../..");
    let settings = std::fs::read_to_string(root.join("roles/abby-normal/.gemini/settings.json")).unwrap();
    assert_eq!(gemini_auth_is_google(&settings), Ok(()));
}

#[test]
fn negative_proof_an_api_key_sign_in_is_refused_by_name() {
    let why = gemini_auth_is_google(r#"{"security":{"auth":{"selectedType":"gemini-api-key"}}}"#).unwrap_err();
    assert!(why.contains("gemini-api-key"), "{why}");
    assert!(gemini_auth_is_google("{}").is_err(), "no sign-in named is not Google login");
}
