// @domain: identity — the login status line (#4997)
// #4997 AC1 — "logged in" only when the role's own login succeeded AND its Session row is
// open in the store (read, never copied); a refused login reads "refused" with the reason.
// 11-09: the self-test showed every role green while their logins were being refused.
use chorus_principal::lifecycle::*;

fn rec() -> LoginState { LoginState::Recorded { session: "s".into(), pid: Some(1) } }

#[test]
fn logged_in_needs_the_login_and_an_open_session_row() {
    assert_eq!(status_line("wren", Some(1), &rec(), true, Some(true)), "wren   running  logged in      answering");
}

#[test]
fn negative_proof_a_closed_session_row_is_never_logged_in() {
    let line = status_line("wren", Some(1), &rec(), true, Some(false));
    assert!(!line.contains("running  logged in"), "{line}");
    assert!(line.contains("session row closed"), "{line}");
}

#[test]
fn negative_proof_an_unread_store_is_never_logged_in() {
    let line = status_line("wren", Some(1), &rec(), true, None);
    assert!(!line.contains("running  logged in"), "{line}");
    assert!(line.contains("unverified"), "{line}");
}

#[test]
fn a_refused_login_reads_refused_with_its_reason() {
    let st = LoginState::Refused { why: "the identity server refused the credential (HTTP 401)".into(), pid: Some(2) };
    let line = status_line("kade", Some(2), &st, true, Some(true));
    assert!(line.contains("refused"), "{line}");
    assert!(line.contains("HTTP 401"), "{line}");
    assert!(!line.contains("logged in") && !line.contains("pending"), "{line}");
}

#[test]
fn negative_proof_a_refused_login_never_reads_logged_in_even_with_an_open_row() {
    // the row can be left open by an earlier login; the refusal still wins
    let st = LoginState::Refused { why: "refused".into(), pid: Some(3) };
    assert_eq!(login_word(&st, Some(3)), "refused");
    assert!(!status_line("kade", Some(3), &st, true, Some(true)).contains("running  logged in"));
}

#[test]
fn a_refused_login_round_trips_through_its_file() {
    let st = LoginState::Refused { why: "HTTP 403".into(), pid: Some(9) };
    assert_eq!(parse_login_state(&login_state_json(&st, "2026-11-17T19:20:00Z")), st);
}

#[test]
fn a_refusal_is_told_apart_from_a_retryable_failure() {
    assert!(is_refusal("no token: invalid_client (HTTP 401)"));
    assert!(is_refusal("the identity API answered HTTP 403"));
    assert!(!is_refusal("the identity API answered HTTP 503"));
    assert!(!is_refusal("identity not answering"));
}

#[test]
fn another_accounts_login_reads_unverified_never_not_logged_in() {
    // live 11-17 19:27: from silas's account kade read "NOT logged in" with its row open
    let st = login_as_seen(LoginState::Unknown, false);
    let line = status_line("kade", Some(4), &st, false, Some(true));
    assert!(line.contains("unverified") && line.contains("own account"), "{line}");
    assert!(!line.contains("NOT logged in") && !line.contains("running  logged in"), "{line}");
}

#[test]
fn negative_proof_the_roles_own_missing_login_still_reads_not_logged_in() {
    let st = login_as_seen(LoginState::Unknown, true);
    assert_eq!(st, LoginState::Unknown);
    assert!(status_line("silas", Some(5), &st, true, Some(true)).contains("NOT logged in"));
    // and a login it can read is never masked
    assert_eq!(login_as_seen(rec(), false), rec());
}
