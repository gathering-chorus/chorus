// @test-type: unit
// @domain: domains
// @card: #4716
// @owner: kade
//! #4716 (Jeff 09:04: option A) — src/lib.rs `deploy_principal`. athena-deploy
//! signed every event with DEPLOY_ROLE / CHORUS_ROLE, a string any caller can
//! type. It now writes as its own principal, `deploy`, whatever the env says.
use athena_deploy::deploy_principal;

#[test]
fn negative_proof_a_typed_role_does_not_sign_a_model_deploy() {
    std::env::set_var("DEPLOY_ROLE", "jeff");
    std::env::set_var("CHORUS_ROLE", "silas");
    assert_eq!(deploy_principal(), "deploy");
}
