// @test-type: unit
// @domain: services
//! #4747 swarm (Silas) — tools/call through the real router, hermetic: the door
//! and the store are closed ports, so nothing here reaches prod. Covers
//! platform/services/chorus-runner/src/server.rs (call_tool, task_status_text,
//! run_athena, refresh_skills): what a verified caller gets back when a call
//! cannot run, and that a task is only read by its own caller.
mod common;
use common::*;

const META: &str = r#""_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientInfo":{"name":"t","version":"0"},"io.modelcontextprotocol/clientCapabilities":{}}"#;

fn tasks_dir() -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("runner-call-4747-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn hermetic() {
    std::env::set_var("CHORUS_FUSEKI", "http://127.0.0.1:9");
    std::env::set_var("CHORUS_RUNNER_TASKS", tasks_dir());
}

async fn call(tool: &str, args: &str) -> (http::StatusCode, String) {
    hermetic();
    let params = format!(r#"{{"name":"{tool}","arguments":{args},{META}}}"#);
    let mut req = rpc("tools/call", &params, "2026-07-28", &good_token());
    // 2026-07-28: a tools/call names its tool in a header as well as the body
    req.headers_mut().insert("mcp-name", http::HeaderValue::from_str(tool).unwrap());
    send(req).await
}

fn is_error(body: &str) -> bool {
    body.contains(r#""isError":true"#)
}

#[tokio::test]
async fn task_status_refuses_an_id_no_long_tool_returned() {
    for id in ["", "../etc", "a.b"] {
        let (status, body) = call("task_status", &format!(r#"{{"id":"{id}"}}"#)).await;
        assert_eq!(status, 200, "{body}");
        assert!(is_error(&body) && body.contains("id must be a run id"), "{id}: {body}");
    }
}

#[tokio::test]
async fn task_status_names_a_task_that_does_not_exist() {
    let (_, body) = call("task_status", r#"{"id":"zz-none-4747"}"#).await;
    assert!(is_error(&body) && body.contains("no task zz-none-4747"), "{body}");
}

/// A task is read only by its caller (the token is wren's) or by jeff.
/// NEGATIVE PROOF: kade's task is refused to wren, while her own is answered.
#[tokio::test]
async fn a_task_is_read_by_its_own_caller_and_refused_to_another() {
    hermetic();
    let d = tasks_dir();
    std::fs::write(d.join("t-kade-4747.json"), r#"{"caller":"kade"}"#).unwrap();
    std::fs::write(d.join("t-wren-4747.json"), r#"{"caller":"wren"}"#).unwrap();
    let (_, theirs) = call("task_status", r#"{"id":"t-kade-4747"}"#).await;
    assert!(is_error(&theirs) && theirs.contains("is kade's; wren may not read it"), "{theirs}");
    let (_, mine) = call("task_status", r#"{"id":"t-wren-4747"}"#).await;
    assert!(!is_error(&mine) && mine.contains("t-wren-4747:"), "{mine}");
}

/// A door that does not answer is a failed call with its trace, never a success.
#[tokio::test]
async fn a_door_tool_with_the_door_down_fails_and_names_its_trace() {
    let (status, body) = call("athena-list", r#"{"collection":"/cards/cards"}"#).await;
    assert_eq!(status, 200, "{body}");
    assert!(is_error(&body) && body.contains("(trace "), "{body}");
}

/// A name that is neither a Skill row (the store is closed, so none load) nor a
/// door tool is refused, not run.
#[tokio::test]
async fn an_unknown_tool_is_refused() {
    let (_, body) = call("zz-no-such-tool-4747", "{}").await;
    assert!(is_error(&body), "{body}");
}
