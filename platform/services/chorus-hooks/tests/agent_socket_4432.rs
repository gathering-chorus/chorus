//! #4432 — the agent socket: the OS says who is calling, the payload never
//! does (Wren's conditions, 2026-10-06 09:04).
use axum::{routing::post, Json, Router};
use chorus_hooks::agent_socket::{account_name, identify, role_for_account, AgentPeer};
use serde_json::{json, Value};
use std::io::{Read, Write};

fn door(rows: Value) -> Value { json!({ "data": rows }) }

#[test]
fn one_principal_on_the_account_names_the_role() {
    let d = door(json!([{"name":"abby-normal","hostAccount":"chorus-abby-normal","holdsRole":"role-abby-normal"}]));
    assert_eq!(role_for_account("chorus-abby-normal", &d).unwrap(), "abby-normal");
}

#[test]
fn negative_proof_unknown_or_shared_accounts_are_refused_loudly() {
    let d = door(json!([
        {"name":"wren","hostAccount":"jeffbridwell","holdsRole":"role-wren"},
        {"name":"silas","hostAccount":"jeffbridwell","holdsRole":"role-silas"},
        {"name":"kade","hostAccount":"jeffbridwell","holdsRole":"role-kade"}]));
    assert!(role_for_account("mallory", &d).unwrap_err().contains("no principal runs as account mallory"));
    // Jeff's account hosts the three: the agent socket cannot tell them apart, so it refuses.
    assert!(role_for_account("jeffbridwell", &d).unwrap_err().contains("hosts 3 principals"));
    assert!(role_for_account("x", &json!({})).unwrap_err().contains("no data list"));
}

/// A stand-in principals door on a free port, answering `body` to every request.
fn stand_in_door(body: Value) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        let body = body.to_string();
        for mut s in listener.incoming().flatten() {
            let mut buf = [0u8; 4096];
            let _ = s.read(&mut buf);
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
        }
    });
    url
}

fn post_over(sock: &std::path::Path, body: &Value) -> Value {
    let mut s = std::os::unix::net::UnixStream::connect(sock).unwrap();
    let b = body.to_string();
    write!(s, "POST /pre-tool-use HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}", b.len()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    serde_json::from_str(out.split_once("\r\n\r\n").unwrap().1).unwrap()
}

#[test]
fn the_role_comes_from_the_peer_uid_not_the_payload() {
    let me = account_name(unsafe { libc::getuid() }).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let sock = dir.path().join("agent.sock");
    let rt = tokio::runtime::Runtime::new().unwrap();
    let echo = Router::new()
        .route("/pre-tool-use", post(|Json(v): Json<Value>| async move { Json(json!({"seen_role": v["deploy_role"]})) }))
        .layer(axum::middleware::from_fn(identify));
    let listener = rt.block_on(async { tokio::net::UnixListener::bind(&sock).unwrap() });
    rt.spawn(async move { axum::serve(listener, echo.into_make_service_with_connect_info::<AgentPeer>()).await.unwrap() });

    // This test's own account is the agent: the door says it hosts abby-normal.
    std::env::set_var("ATHENA_MAKE_URL", stand_in_door(door(json!([{"name":"abby-normal","hostAccount":me,"holdsRole":"role-abby-normal"}]))));
    // NEGATIVE PROOF: the payload claims silas; the socket answers abby-normal.
    let r = post_over(&sock, &json!({"deploy_role":"silas","tool_name":"Write"}));
    assert_eq!(r["seen_role"], "abby-normal", "{r}");
    let r = post_over(&sock, &json!({"tool_name":"Write"}));
    assert_eq!(r["seen_role"], "abby-normal", "{r}");

    // NEGATIVE PROOF: an account the door doesn't know never reaches the handler.
    std::env::set_var("ATHENA_MAKE_URL", stand_in_door(door(json!([]))));
    let r = post_over(&sock, &json!({"deploy_role":"silas","tool_name":"Write"}));
    assert!(r.get("seen_role").is_none(), "{r}");
    assert!(r["stdout"].as_str().unwrap().contains("no principal runs as account"), "{r}");

    // and a door that can't answer refuses too
    std::env::set_var("ATHENA_MAKE_URL", "http://127.0.0.1:9");
    let r = post_over(&sock, &json!({"tool_name":"Write"}));
    assert!(r["stdout"].as_str().unwrap().contains("principals door is unreadable"), "{r}");
}
