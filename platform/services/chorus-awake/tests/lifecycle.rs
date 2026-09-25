// #4295 — sign in / sign out decisions. The bats suite (4295-role-login.bats)
// drives the built binary; these hold the pure rules, each with its violation.
use chorus_awake::lifecycle::*;

#[test]
fn a_login_state_round_trips_through_its_file() {
    for st in [
        LoginState::Recorded { session: "silas-abc-1".into(), pid: Some(42) },
        LoginState::Pending { why: "identity not answering".into(), pid: None },
        LoginState::Closed,
    ] {
        assert_eq!(parse_login_state(&login_state_json(&st, "2026-09-25T14:30:00Z")), st);
    }
    assert_eq!(parse_login_state("not json"), LoginState::Unknown);
    assert_eq!(parse_login_state(r#"{"state":"recorded"}"#), LoginState::Unknown, "recorded with no session name is not a login");
}

#[test]
fn a_login_from_an_earlier_session_is_not_this_sessions_login() {
    // 2026-09-25 09:45: a recorded login and a live pid were read as "registered yes"
    let st = LoginState::Recorded { session: "s".into(), pid: Some(4263) };
    assert_eq!(login_word(&st, Some(4263)), "logged in");
    assert_eq!(login_word(&st, Some(46809)), "NOT logged in", "another pid's login must not bless this one");
    // one written just before the pane registered is adopted
    let fresh = LoginState::Recorded { session: "s".into(), pid: None };
    assert_eq!(login_word(&fresh, Some(7)), "logged in");
    assert_eq!(login_word(&LoginState::Unknown, Some(7)), "NOT logged in");
    assert_eq!(login_word(&LoginState::Closed, Some(7)), "NOT logged in");
}

#[test]
fn status_names_running_login_and_answering_on_one_line() {
    let rec = LoginState::Recorded { session: "s".into(), pid: Some(1) };
    assert_eq!(status_line("wren", Some(1), &rec, true), "wren   running  logged in      answering");
    let pend = LoginState::Pending { why: "identity not answering".into(), pid: Some(2) };
    assert_eq!(status_line("kade", Some(2), &pend, true), "kade   running  login pending  answering   (identity not answering, retrying)");
    assert_eq!(status_line("silas", None, &rec, false), "silas  off");
    assert!(status_line("silas", Some(9), &LoginState::Unknown, false).contains("NOT logged in"));
    assert!(status_line("silas", Some(9), &LoginState::Unknown, false).contains("NOT answering"));
}

#[test]
fn negative_proof_no_status_line_ever_says_registered_yes() {
    for st in [LoginState::Unknown, LoginState::Closed, LoginState::Pending { why: "x".into(), pid: Some(3) }] {
        let l = status_line("kade", Some(3), &st, true);
        assert!(!l.contains("registered yes"), "{l}");
        assert!(!l.contains(" logged in ") || l.contains("NOT"), "{l}");
    }
}

#[test]
fn jeffs_shell_may_start_any_role() {
    assert!(caller_may_act(false, None, "silas", "start").is_ok());
    // his Terminal sits in roles/kade and has CHORUS_ROLE=kade; it is still his shell
    assert!(caller_may_act(false, Some("kade"), "silas", "start").is_ok());
}

#[test]
fn a_role_session_may_not_start_or_stop_another_role() {
    // 2026-09-25 09:50:21 — Kade's session ran `chorus-awake silas`
    let e = caller_may_act(true, Some("kade"), "silas", "start").unwrap_err();
    assert!(e.contains("a kade session cannot start silas"), "{e}");
    assert!(caller_may_act(true, None, "silas", "stop").is_err());
    assert!(caller_may_act(true, Some("silas"), "silas", "stop").is_ok(), "a role may sign itself out (/exit)");
}

#[test]
fn a_5xx_or_no_answer_is_not_up() {
    assert!(is_answering("200"));
    assert!(is_answering("401"), "a 401 is a service answering");
    assert!(is_answering("404"));
    assert!(!is_answering("000"));
    assert!(!is_answering("502"), "the boot 502s are a service not ready");
    assert!(!is_answering(""));
    assert!(!is_answering("curl failed"));
}

#[test]
fn the_countdown_names_the_service_and_its_port() {
    assert_eq!(countdown_line("identity", "http://localhost:3001/", 42, 180), "waiting for identity :3001 ... 0:42 of 3:00");
    assert_eq!(countdown_line("chorus-api", "http://localhost:3340/api/chorus/health", 0, 180), "waiting for chorus-api :3340 ... 0:00 of 3:00");
}

#[test]
fn services_spec_parses_and_none_means_none() {
    assert_eq!(parse_services("none"), vec![]);
    let d = parse_services(DEFAULT_SERVICES);
    assert_eq!(d.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), vec!["identity", "chorus-api", "athena-make"]);
    assert_eq!(parse_services("a=http://x:1/,junk,=nourl"), vec![("a".to_string(), "http://x:1/".to_string())]);
}

#[test]
fn the_reboot_line_counts_logins_and_names_what_is_missing() {
    let all = vec![("wren".into(), Came::LoggedIn), ("kade".into(), Came::LoggedIn), ("silas".into(), Came::LoggedIn)];
    assert_eq!(summary_line(&all), "wren kade silas up, 3 of 3 logged in");
    let some = vec![("wren".into(), Came::LoggedIn), ("kade".into(), Came::Pending("identity not answering".into())), ("silas".into(), Came::NotStarted("tmux failed".into()))];
    assert_eq!(summary_line(&some), "wren kade up, 1 of 3 logged in; kade login pending (identity not answering), retrying on its own; silas NOT started (tmux failed)");
}

#[test]
fn negative_proof_a_pending_login_never_counts_as_logged_in() {
    let r = vec![("kade".into(), Came::Pending("x".into()))];
    assert!(summary_line(&r).contains("0 of 1 logged in"), "{}", summary_line(&r));
}

#[test]
fn exit_logs_out_and_clear_does_not() {
    assert!(exit_reason_logs_out("prompt_input_exit"));
    assert!(exit_reason_logs_out("logout"));
    assert!(!exit_reason_logs_out("clear"), "/clear keeps the role running");
    assert!(!exit_reason_logs_out("other"));
}

#[test]
fn closing_a_row_writes_its_end_and_keeps_everything_else() {
    let existing = r#"{"apiVersion":"v1","data":{"name":"silas-x-1","ownedBy":"principal-silas","sessionState":"open","endedAt":"","tokenId":"t"}}"#;
    let row = closed_row(existing, "2026-09-25T18:00:00Z").unwrap();
    assert_eq!(row["sessionState"], "closed");
    assert_eq!(row["endedAt"], "2026-09-25T18:00:00Z");
    assert_eq!(row["ownedBy"], "principal-silas");
    assert_eq!(row["name"], "silas-x-1");
    assert!(closed_row(r#"{"error":"not-found"}"#, "t").is_none(), "an error body is not a row to close");
}

#[test]
fn the_tmux_bar_says_the_login() {
    assert!(tmux_status_right("logged in").starts_with(" logged in |"));
    assert!(tmux_status_right("login pending").contains("login pending"));
}

#[test]
fn vscode_task_runs_on_folder_open_and_attaches_wren() {
    let j: serde_json::Value = serde_json::from_str(&vscode_tasks_json("/h/.chorus/bin/chorus-principal")).unwrap();
    assert_eq!(j["tasks"][0]["command"], "/h/.chorus/bin/chorus-principal on wren");
    assert_eq!(j["tasks"][0]["runOptions"]["runOn"], "folderOpen");
}
