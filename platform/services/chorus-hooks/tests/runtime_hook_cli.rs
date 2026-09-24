//! Native entrypoint acceptance tests. Isolated HOME/UDS, no running services.
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};
use std::time::Duration;

fn command(home: &std::path::Path, runtime: &str, event: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_chorus-hook-shim"));
    command.env_clear().env("HOME", home).env("CHORUS_ROLE", "wren")
        .env("CHORUS_SESSION_ID", "hermetic-session")
        .env("DEPLOY_ROLE", "wren").arg("runtime-hook").arg(runtime).arg(event)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    command
}

fn invoke(mut command: Command, raw: Value) -> Value {
    let mut child = command.spawn().unwrap();
    child.stdin.take().unwrap().write_all(raw.to_string().as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn raw_trust_or_role_cannot_authorize_unknown_tool() {
    let home = tempfile::tempdir().unwrap();
    let raw = json!({"cwd":"/workspace", "session_id":"attacker", "trusted":true,
        "deploy_role":"silas", "tool_name":"write_stdin", "tool_input":{"chars":"touch forbidden"}});
    let answer = invoke(command(home.path(), "codex", "PreToolUse"), raw);
    assert_eq!(answer["hookSpecificOutput"]["permissionDecision"], "deny");
    assert!(answer.to_string().contains("capability gap"));
}

#[test]
fn canonical_paths_and_delete_in_patches_remain_denied() {
    let home = tempfile::tempdir().unwrap();
    for patch in [
        "*** Begin Patch\n*** Delete File: /workspace/canonical/secret\n*** End Patch",
        "*** Begin Patch\n*** Add File: /workspace/canonical/new\n+new\n*** End Patch",
    ] {
        let mut cmd = command(home.path(), "codex", "PreToolUse");
        cmd.env("CHORUS_HOME", "/workspace/canonical");
        let answer = invoke(cmd, json!({"cwd":"/workspace/own", "tool_name":"apply_patch", "tool_input":{"command":patch}}));
        assert_eq!(answer["hookSpecificOutput"]["permissionDecision"], "deny");
        assert!(answer.to_string().contains("canonical"));
    }
}

#[test]
fn patch_move_destination_is_checked_after_allowed_source() {
    let home = tempfile::tempdir().unwrap();
    let listener = UnixListener::bind(home.path().join("chorus-hooks.sock")).unwrap();
    let server = std::thread::spawn(move || request(&listener,json!({"exit_code":0,"stdout":null,"stderr":null})));
    let mut cmd = command(home.path(), "codex", "PreToolUse");
    cmd.env("CHORUS_HOME","/workspace/canonical").env("CHORUS_HOOKS_RUN_DIR",home.path());
    let patch = "*** Begin Patch\n*** Update File: /workspace/own/a.rs\n*** Move to: /workspace/canonical/a.rs\n@@\n-old\n+new\n*** End Patch";
    let answer = invoke(cmd,json!({"cwd":"/workspace/own","tool_name":"apply_patch","tool_input":{"command":patch}}));
    assert_eq!(answer["hookSpecificOutput"]["permissionDecision"],"deny");
    assert!(answer.to_string().contains("canonical"));
    let (_,source) = server.join().unwrap();
    assert_eq!(source["tool_input"]["file_path"],"/workspace/own/a.rs");
}

fn request(listener: &UnixListener, reply: Value) -> (String, Value) {
    let (mut socket, _) = listener.accept().unwrap();
    socket.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut received = Vec::new();
    let boundary = loop {
        let mut buf = [0; 1024];
        let count = socket.read(&mut buf).unwrap();
        assert!(count > 0);
        received.extend_from_slice(&buf[..count]);
        if let Some(index) = received.windows(4).position(|b| b == b"\r\n\r\n") { break index + 4; }
    };
    let headers = String::from_utf8(received[..boundary].to_vec()).unwrap();
    let length: usize = headers.lines().find_map(|line| line.strip_prefix("Content-Length: "))
        .unwrap().parse().unwrap();
    while received.len() < boundary + length {
        let mut buf = [0; 1024];
        let count = socket.read(&mut buf).unwrap();
        received.extend_from_slice(&buf[..count]);
    }
    let body = serde_json::from_slice(&received[boundary..boundary + length]).unwrap();
    let reply = reply.to_string();
    write!(socket, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", reply.len(), reply).unwrap();
    (headers.lines().next().unwrap().to_string(), body)
}

#[test]
fn enrolled_start_publishes_claims_context_then_acknowledges_delivery() {
    let home = tempfile::tempdir().unwrap();
    let socket = home.path().join("agent.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let event = request(&listener, json!({"ok":true}));
        let boundary = request(&listener, json!({"context":["Hello Wren", "A pending message"],"context_token":"context-hash","messages":[{"id":17,"message_id":"pulse:17","from":"silas","kind":"peer_message","content":"Delivery fixture"}]}));
        let ack = request(&listener, json!({"ok":true}));
        (event,boundary,ack)
    });
    let mut cmd = command(home.path(), "gemini", "SessionStart");
    cmd.env("CHORUS_SESSION_ID", "chorus-registered").env("CHORUS_AGENT_SOCKET", socket);
    let answer = invoke(cmd, json!({"session_id":"native-session", "trusted":true, "token":"must-not-forward"}));
    assert!(answer["hookSpecificOutput"]["additionalContext"].as_str().unwrap().contains("Hello Wren\nA pending message"));
    let (event,boundary,ack) = server.join().unwrap();
    assert!(event.0.starts_with("POST /v1/events "));
    assert_eq!(event.1["session_id"], "chorus-registered");
    assert_eq!(event.1["native_session_id"], "native-session");
    assert_eq!(event.1["data"]["runtime"], "gemini");
    assert!(!event.1.to_string().contains("must-not-forward"));
    assert!(boundary.0.contains("/v1/sessions/chorus-registered/boundary"));
    assert!(ack.0.contains("/v1/sessions/chorus-registered/ack"));
    assert_eq!(ack.1["ids"], json!([17]));
    assert_eq!(ack.1["context_token"], "context-hash");
}

#[test]
fn closed_native_stdout_does_not_acknowledge_claimed_context() {
    let home = tempfile::tempdir().unwrap();
    let socket = home.path().join("agent.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        request(&listener, json!({"ok":true}));
        request(&listener, json!({"context":["undelivered handoff"],"context_token":"context-hash","messages":[]}));
        listener
    });
    let mut cmd = command(home.path(), "gemini", "SessionStart");
    cmd.env("CHORUS_AGENT_SOCKET", socket);
    let mut child = cmd.spawn().unwrap();
    // The native client has disappeared before consuming additionalContext.
    drop(child.stdout.take());
    child.stdin.take().unwrap().write_all(b"{\"session_id\":\"native\"}").unwrap();
    assert!(!child.wait().unwrap().success());
    let listener = server.join().unwrap();
    listener.set_nonblocking(true).unwrap();
    assert!(matches!(listener.accept(), Err(error) if error.kind()==std::io::ErrorKind::WouldBlock));
}

#[test]
fn native_payload_cannot_self_enroll_without_launcher_identity() {
    let home = tempfile::tempdir().unwrap();
    let mut cmd = command(home.path(), "gemini", "BeforeTool");
    cmd.env_remove("CHORUS_SESSION_ID");
    let answer = invoke(cmd, json!({"cwd":"/workspace","session_id":"raw-native", "trusted":true,
        "tool_name":"read_file","tool_input":{"file_path":"example"}}));
    assert_eq!(answer["decision"],"deny");
    assert!(answer["reason"].as_str().unwrap().contains("supervisor-issued"));
}
