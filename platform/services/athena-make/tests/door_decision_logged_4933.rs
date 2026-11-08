// @test-type: unit — pure decision builders and the write-ahead file
// @domain: security
// #4933 AC4 — every door decision on a write, allow and deny, is one log line that
// carries the request's trace id (the login's, when the login sent it), the
// VERIFIED principal (never the x-chorus-caller header), what was asked, and the
// typed reason. One pure builder; the serve seam calls it once per write.
//
// NEGATIVE PROOF (#3734): a 403 must read "deny" with its typed reason, and a 201
// must read "allow"; a builder that logged every write as one verdict, or dropped
// the trace, goes red on the pair below.

use athena_make::{decision_line, json_field};

fn field(line: &str, k: &str) -> String { json_field(line, k).unwrap_or_default() }

#[test]
fn an_allowed_session_open_is_one_allow_line_with_the_login_trace() {
    let l = decision_line(1, "login-login-fx-17-1", "fx", "POST", "Session", "", 201, r#"{ "status": "created" }"#);
    assert_eq!(l.matches('\n').count(), 1, "exactly one line: {l}");
    assert_eq!(field(&l, "event"), "authz.decision");
    assert_eq!(field(&l, "decision"), "allow");
    assert_eq!(field(&l, "trace_id"), "login-login-fx-17-1");
    assert_eq!(field(&l, "principal"), "fx");
    assert_eq!(field(&l, "action"), "POST");
    assert_eq!(field(&l, "class"), "Session");
}

#[test]
fn negative_proof_a_not_served_refusal_is_a_deny_line_with_its_reason() {
    let l = decision_line(1, "login-login-fx-17-1", "fx", "POST", "Session", "", 403, r#"{ "error": "not-served", "message": "x" }"#);
    assert_eq!(field(&l, "decision"), "deny");
    assert_eq!(field(&l, "reason"), "not-served");
    assert_eq!(field(&l, "trace_id"), "login-login-fx-17-1");
}

#[test]
fn an_unauthenticated_write_is_a_deny_with_no_principal() {
    let l = decision_line(1, "t", "", "POST", "Session", "", 401, r#"{ "error": "authn-missing" }"#);
    assert_eq!(field(&l, "decision"), "deny");
    assert_eq!(field(&l, "reason"), "authn-missing");
    assert_eq!(field(&l, "principal"), "");
}

#[test]
fn a_store_that_did_not_answer_is_undecided_not_allow_or_deny() {
    let l = decision_line(1, "t", "fx", "POST", "Session", "", 503, r#"{ "error": "unavailable" }"#);
    assert_eq!(field(&l, "decision"), "undecided");
}

#[test]
fn a_validation_refusal_after_authz_passed_reads_allow_with_its_outcome() {
    // authz allowed the write; the shape refused the body — the decision was allow
    let l = decision_line(1, "t", "fx", "POST", "Session", "", 422, r#"{ "error": "validation" }"#);
    assert_eq!(field(&l, "decision"), "allow");
    assert_eq!(field(&l, "outcome"), "422");
}

// --- Silas 17:01: deny includes authN, and B1 (no log line, no write) ---------------

use athena_make::{decision_for, open_decision_sink, unlogged_refusal};

#[test]
fn a_failed_sign_in_on_a_write_gets_its_own_deny_line() {
    let l = decision_for(1, "t", "", "POST", "Session", "", 401, r#"{ "error": "authn-missing" }"#).expect("a 401 write is a decision");
    assert_eq!(field(&l, "decision"), "deny");
    assert_eq!(field(&l, "reason"), "authn-missing");
}

#[test]
fn negative_proof_a_read_is_not_a_decision_line() {
    // a builder that logged every request would flood the decision log with reads
    assert!(decision_for(1, "t", "fx", "GET", "Session", "", 200, "{}").is_none());
}

#[test]
fn b1_a_decision_log_that_cannot_be_opened_refuses_the_write() {
    let home = std::env::temp_dir().join(format!("b1-4933-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&home);
    // ops/ is a FILE, so ops/logs/<day>.jsonl cannot be opened under it
    let _ = std::fs::write(home.join("ops"), "not a dir");
    let sink = open_decision_sink(&home.to_string_lossy(), 1_700_000_000_000);
    assert!(sink.is_err(), "the sink must fail to open here");
    let (code, body) = unlogged_refusal(&sink).expect("an unloggable write is refused");
    assert_eq!(code, 503, "{body}");
    assert!(body.contains("decision log"), "{body}");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn b1_control_a_writable_decision_log_lets_the_write_through() {
    let home = std::env::temp_dir().join(format!("b1ok-4933-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&home);
    let sink = open_decision_sink(&home.to_string_lossy(), 1_700_000_000_000);
    assert!(sink.is_ok(), "{:?}", sink.err());
    assert!(unlogged_refusal(&sink).is_none());
    let _ = std::fs::remove_dir_all(&home);
}

// --- Silas 17:03: write-ahead. The allow line is appended BEFORE the store is touched;
// an append that fails refuses the write (503); a store write that then fails gets an
// outcome line. One decision line per write, even on a full disk.

use athena_make::{arm_write_ahead, write_ahead, disarm_write_ahead, write_err_resp};

fn tmpfile(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("wal-4933-{}-{}-{}.jsonl", tag, std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()))
}

#[test]
fn write_ahead_appends_the_allow_line_once_before_the_first_store_write() {
    let p = tmpfile("ok");
    let f = std::fs::OpenOptions::new().create(true).append(true).open(&p).unwrap();
    arm_write_ahead(f, "{\"decision\":\"allow\"}\n".into());
    assert!(write_ahead().is_ok());
    assert!(write_ahead().is_ok(), "a second store call in the same request adds no line");
    assert!(disarm_write_ahead(), "the request knows its allow line went ahead");
    assert_eq!(std::fs::read_to_string(&p).unwrap().lines().count(), 1);
    let _ = std::fs::remove_file(&p);
}

#[test]
fn negative_proof_an_unappendable_log_refuses_the_store_write_with_503() {
    let p = tmpfile("ro");
    std::fs::write(&p, "").unwrap();
    let ro = std::fs::OpenOptions::new().read(true).open(&p).unwrap(); // appends fail
    arm_write_ahead(ro, "{\"decision\":\"allow\"}\n".into());
    let e = write_ahead().expect_err("the store must not be touched when the line cannot go ahead");
    assert!(e.starts_with("decision-unlogged"), "{e}");
    assert_eq!(write_err_resp(&e).0, 503);
    assert!(!disarm_write_ahead());
    let _ = std::fs::remove_file(&p);
}

#[test]
fn a_request_that_never_armed_write_ahead_is_not_refused_by_it() {
    // the deploy and tests call the DAL outside a served request
    assert!(write_ahead().is_ok());
}
