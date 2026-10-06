// @domain: identity
//! #4432 — the agent roles come from the roles door, never a list of three.
use chorus_principal::rows::{agent_roles, agent_socket_for};

const DOOR: &str = r#"{"data":[
 {"name":"abby-normal","roleKind":"agent"},{"name":"jeff","roleKind":"human"},
 {"name":"kade","roleKind":"agent"},{"name":"nightly","roleKind":""},
 {"name":"silas","roleKind":"agent"},{"name":"wren","roleKind":"agent"}]}"#;

#[test]
fn agent_roles_are_every_agent_row_including_abby() {
    assert_eq!(agent_roles(DOOR).unwrap(), vec!["abby-normal", "kade", "silas", "wren"]);
}

#[test]
fn a_human_or_unkinded_row_is_not_an_agent_role() {
    let got = agent_roles(DOOR).unwrap();
    assert!(!got.contains(&"jeff".to_string()) && !got.contains(&"nightly".to_string()));
}

#[test]
fn negative_proof_an_unreadable_door_is_an_error_not_the_usual_three() {
    assert!(agent_roles("").is_err());
    assert!(agent_roles("<html>502</html>").is_err());
    assert!(agent_roles(r#"{"error":"down"}"#).is_err());
}

#[test]
fn negative_proof_a_door_with_no_agent_role_is_an_error() {
    assert!(agent_roles(r#"{"data":[{"name":"jeff","roleKind":"human"}]}"#).is_err());
}

#[test]
fn an_agent_on_its_own_account_is_asked_on_that_accounts_socket() {
    assert_eq!(agent_socket_for("/Users", Some("chorus-abby-normal")).as_deref(),
               Some("/Users/chorus-abby-normal/.chorus/run/chorus-agent.sock"));
}

#[test]
fn negative_proof_no_own_account_means_the_launchers_default_socket() {
    assert_eq!(agent_socket_for("/Users", None), None);
    assert_eq!(agent_socket_for("/Users", Some("")), None);
}
