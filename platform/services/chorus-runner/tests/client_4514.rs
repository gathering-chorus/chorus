// @test-type: unit
// @domain: services
//! #4514 AC4 — `chorus-runner call`: the pieces that turn a dagu step into a
//! runner call and its answer back into the step's exit code.
use chorus_runner::client::{args_from_pairs, finished, parse_result, task_id};
use serde_json::json;

#[test]
fn pairs_become_text_arguments_and_a_repeated_key_a_list() {
    let a = args_from_pairs(&["card=4514".into(), "dry_run=true".into(), "args=scope".into(), "args=.".into()]).unwrap();
    assert_eq!(a, json!({ "card": "4514", "dry_run": "true", "args": ["scope", "."] }));
    assert!(args_from_pairs(&["card".into()]).is_err(), "a pair without = is refused");
}

#[test]
fn a_long_tools_answer_names_its_task_and_its_status_ends_the_call() {
    let started = r#"{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"started werk-test as task werk-test-abc: call task_status with this id"}]}}"#;
    let (err, text) = parse_result(started);
    assert!(!err);
    assert_eq!(task_id(&text).as_deref(), Some("werk-test-abc"));
    assert_eq!(finished("werk-test-abc: running\n…"), None);
    assert_eq!(finished("werk-test-abc: done (exit 0)\nok"), Some(0));
}

#[test]
fn negative_proof_a_failed_or_lost_task_never_exits_zero() {
    assert_eq!(finished("t: done (exit 3)\nboom"), Some(3));
    assert_eq!(finished("t: lost: the process is gone and wrote no exit code"), Some(1));
    let (err, _) = parse_result(r#"{"jsonrpc":"2.0","id":1,"result":{"isError":true,"content":[{"type":"text","text":"refused"}]}}"#);
    assert!(err, "an error answer read as success");
    let (err2, _) = parse_result("<html>bad gateway</html>");
    assert!(err2, "a non-JSON answer read as success");
}
