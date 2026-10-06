//! Native entrypoint acceptance tests. Isolated HOME/UDS, no running services.
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};
use std::time::Duration;

/// #4432 — a stand-in roles door: the hook enrolls a role only when the door
/// lists it as an agent role, so every case brings its own door, never :3360.
fn roles_door() -> &'static str {
    static URL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    URL.get_or_init(|| {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let body = json!({"data":[
                {"name":"wren","roleKind":"agent"},{"name":"silas","roleKind":"agent"},{"name":"kade","roleKind":"agent"},
                {"name":"abby-normal","roleKind":"agent"},{"name":"jeff","roleKind":"human"},{"name":"nightly","roleKind":""}]}).to_string();
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
            }
        });
        url
    })
}

fn command(home: &std::path::Path, runtime: &str, event: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_chorus-hook-shim"));
    command.env_clear().env("HOME", home).env("ATHENA_MAKE_URL", roles_door()).env("CHORUS_ROLE", "wren")
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
    let (socket, _) = listener.accept().unwrap();
    answer(socket, reply)
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

// ---- #4424: fitting the hooks slice to main ----

fn token_for(webid: &str) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let payload = format!(r#"{{"webid":"{webid}","jti":"j","exp":9999999999}}"#);
    let mut out = String::new();
    for chunk in payload.as_bytes().chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |a, (i, b)| a | (*b as u32) << (16 - 8 * i));
        for i in 0..chunk.len() + 1 { out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char); }
    }
    format!("e30.{out}.sig")
}

fn logged_in(home: &std::path::Path, runtime: &str, event: &str, role: &str) -> Command {
    let tok = home.join("session.token");
    std::fs::write(&tok, token_for(&format!("https://id.example/{role}/profile/card#me"))).unwrap();
    let mut cmd = command(home, runtime, event);
    cmd.env_remove("CHORUS_ROLE").env_remove("DEPLOY_ROLE")
        .env("CHORUS_SESSION_TOKEN_FILE", tok).env("CHORUS_HOOKS_RUN_DIR", home);
    cmd
}

// A fake hooks daemon that answers "allow" once. It gives up after 5s and
// returns Null, so a test whose call never reaches the policy fails on its
// assert instead of hanging the suite.
fn allow_once(home: &std::path::Path) -> std::thread::JoinHandle<(String, Value)> {
    let listener = UnixListener::bind(home.join("chorus-hooks.sock")).unwrap();
    listener.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            match listener.accept() {
                Ok((socket, _)) => { socket.set_nonblocking(false).unwrap(); return answer(socket, json!({"exit_code":0,"stdout":null,"stderr":null})); }
                Err(_) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        (String::new(), Value::Null)
    })
}

fn answer(mut socket: std::os::unix::net::UnixStream, reply: Value) -> (String, Value) {
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
    let length: usize = headers.lines().find_map(|line| line.strip_prefix("Content-Length: ")).unwrap().parse().unwrap();
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
fn logged_in_session_names_the_role_without_role_env() {
    let home = tempfile::tempdir().unwrap();
    let server = allow_once(home.path());
    let answer = invoke(logged_in(home.path(), "codex", "PreToolUse", "wren"),
        json!({"cwd":"/workspace/own","tool_name":"read_file","tool_input":{"file_path":"a.rs"}}));
    assert_ne!(answer["hookSpecificOutput"]["permissionDecision"], "deny", "{answer}");
    let (_, body) = server.join().unwrap();
    assert_eq!(body["deploy_role"], "wren");
}

#[test]
fn session_token_wins_over_a_typed_role() {
    let home = tempfile::tempdir().unwrap();
    let server = allow_once(home.path());
    let mut cmd = logged_in(home.path(), "codex", "PreToolUse", "silas");
    cmd.env("CHORUS_ROLE", "wren").env("DEPLOY_ROLE", "wren");
    invoke(cmd, json!({"cwd":"/workspace/own","tool_name":"read_file","tool_input":{"file_path":"a.rs"}}));
    let (_, body) = server.join().unwrap();
    assert_eq!(body["deploy_role"], "silas");
}

#[test]
fn no_session_and_no_role_env_is_still_refused() {
    let home = tempfile::tempdir().unwrap();
    let mut cmd = command(home.path(), "codex", "PreToolUse");
    cmd.env_remove("CHORUS_ROLE").env_remove("DEPLOY_ROLE");
    let answer = invoke(cmd, json!({"cwd":"/workspace/own","tool_name":"read_file","tool_input":{"file_path":"a.rs"}}));
    assert_eq!(answer["hookSpecificOutput"]["permissionDecision"], "deny");
}

#[test]
fn tool_call_id_reaches_the_policy_so_start_and_end_join() {
    for (runtime, raw_key, id) in [("codex", "tool_use_id", "call-7"), ("opencode", "callID", "oc-9")] {
        let home = tempfile::tempdir().unwrap();
        let server = allow_once(home.path());
        let event = if runtime == "opencode" { "tool.execute.before" } else { "PreToolUse" };
        let mut raw = json!({"cwd":"/workspace/own","tool_name":"read_file","tool_input":{"file_path":"a.rs"}});
        raw[raw_key] = json!(id);
        invoke(logged_in(home.path(), runtime, event, "wren"), raw);
        let (_, body) = server.join().unwrap();
        assert_eq!(body["tool_use_id"], id, "{runtime}");
    }
}

#[test]
fn daemon_down_lets_only_the_hooks_restart_through() {
    let restart = "launchctl kickstart -k gui/501/com.chorus.hooks";
    for (command_text, allowed) in [(restart, true), ("ls -la", false)] {
        let home = tempfile::tempdir().unwrap(); // no chorus-hooks.sock = daemon down
        let answer = invoke(logged_in(home.path(), "codex", "PreToolUse", "silas"),
            json!({"cwd":"/workspace/own","tool_name":"exec_command","tool_input":{"cmd":command_text}}));
        let denied = answer["hookSpecificOutput"]["permissionDecision"] == "deny";
        assert_eq!(!denied, allowed, "{command_text}: {answer}");
    }
}

// ---- #4432: enrollment comes from the roles door ----

#[test]
fn abby_normal_logged_in_reaches_the_policy() {
    let home = tempfile::tempdir().unwrap();
    let server = allow_once(home.path());
    let answer = invoke(logged_in(home.path(), "gemini", "BeforeTool", "abby-normal"),
        json!({"cwd":"/tmp","tool_name":"write_file","tool_input":{"file_path":"/tmp/abby.txt","content":"x"}}));
    assert_ne!(answer["decision"], "deny", "{answer}");
    let (_, body) = server.join().unwrap();
    assert_eq!(body["deploy_role"], "abby-normal");
}

#[test]
fn negative_proof_a_role_the_door_does_not_list_as_agent_is_not_enrolled() {
    let home = tempfile::tempdir().unwrap();
    for who in ["nightly", "jeff", "mallory"] {
        let answer = invoke(logged_in(home.path(), "gemini", "BeforeTool", who),
            json!({"cwd":"/tmp","tool_name":"read_file","tool_input":{"file_path":"/tmp/a"}}));
        assert_eq!(answer["decision"], "deny", "{who}: {answer}");
        assert!(answer["reason"].as_str().unwrap().contains("not enrolled"), "{who}: {answer}");
    }
}

#[test]
fn negative_proof_an_unreadable_roles_door_refuses_never_guesses() {
    let home = tempfile::tempdir().unwrap();
    let mut cmd = logged_in(home.path(), "gemini", "BeforeTool", "wren");
    cmd.env("ATHENA_MAKE_URL", "http://127.0.0.1:9");
    let answer = invoke(cmd, json!({"cwd":"/tmp","tool_name":"read_file","tool_input":{"file_path":"/tmp/a"}}));
    assert_eq!(answer["decision"], "deny", "{answer}");
    assert!(answer["reason"].as_str().unwrap().contains("roles door is unreadable"), "{answer}");
}
