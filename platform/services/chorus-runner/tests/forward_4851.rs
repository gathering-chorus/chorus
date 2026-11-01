// @test-type: integration
// @domain: services
//! #4851 — `chorus-runner stdio` forwards to the HTTP runner and runs nothing
//! itself. What BQ and Abby see: their tool calls answered by the runner on
//! :3342 as themselves, an SSE reply unwrapped to one JSON line, a credential
//! that ages out re-minted once, and a runner that is down answered as an
//! error, never served locally.
use chorus_runner::forward::{error_for, messages, parse_headers};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn bin() -> PathBuf {
    // run-time, not env!(): the werk-test 4030 guard
    let me = std::env::current_exe().unwrap();
    me.parent().and_then(|d| d.parent()).unwrap().join("chorus-runner")
}

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("fwd-4851-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn an_sse_reply_is_one_json_line_per_event() {
    let got = messages("text/event-stream", "data: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"n\"}\n\n");
    assert_eq!(got, vec![r#"{"id":1,"jsonrpc":"2.0","result":{}}"#.to_string(), r#"{"jsonrpc":"2.0","method":"n"}"#.to_string()]);
    let multi = messages("text/event-stream; charset=utf-8", "data: {\"a\":\ndata: 1}\n\n");
    assert_eq!(multi, vec![r#"{"a":1}"#.to_string()], "multi-line data joins with a newline");
}

#[test]
fn a_json_reply_is_itself_and_an_empty_one_is_nothing() {
    assert_eq!(messages("application/json", " {\"id\":2} \n"), vec![r#"{"id":2}"#.to_string()]);
    assert!(messages("application/json", "").is_empty(), "a 202 to a notification prints nothing");
}

#[test]
fn the_last_status_line_and_the_session_id_are_read() {
    let r = parse_headers("HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nmcp-session-id: s-9\r\n\r\n");
    assert_eq!((r.status, r.content_type.as_str(), r.session.as_deref()), (200, "text/event-stream", Some("s-9")));
}

#[test]
fn a_request_gets_an_error_answer_and_a_notification_none() {
    let e = error_for(r#"{"jsonrpc":"2.0","id":7,"method":"tools/call"}"#, "down").unwrap();
    assert!(e.contains(r#""id":7"#) && e.contains("down"), "{e}");
    assert!(error_for(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#, "down").is_none());
}

/// A stand-in HTTP runner: logs each request's Authorization, session id and
/// body; answers 401 to the first request when `deny-once` exists; replies SSE.
fn stub_runner(dir: &Path) -> (std::process::Child, u16) {
    let script = dir.join("stub.py");
    std::fs::write(&script, r#"
import http.server, json, os, sys, socketserver
D = sys.argv[1]
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get('Content-Length', 0))).decode()
        with open(os.path.join(D, 'seen.log'), 'a') as f:
            f.write(json.dumps({'auth': self.headers.get('Authorization'), 'session': self.headers.get('Mcp-Session-Id'), 'body': body}) + '\n')
        deny = os.path.join(D, 'deny-once')
        if os.path.exists(os.path.join(D, 'deny-always')):
            self.send_response(401); self.end_headers(); return
        if os.path.exists(deny):
            os.remove(deny); self.send_response(401); self.end_headers(); return
        msg = json.loads(body)
        if 'id' not in msg:
            self.send_response(202); self.end_headers(); return
        out = 'event: message\ndata: ' + json.dumps({'jsonrpc': '2.0', 'id': msg['id'], 'result': {'ok': True}}) + '\n\n'
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.send_header('Mcp-Session-Id', 's-1')
        self.end_headers(); self.wfile.write(out.encode())
s = socketserver.TCPServer(('127.0.0.1', 0), H)
open(os.path.join(D, 'port'), 'w').write(str(s.server_address[1]))
s.serve_forever()
"#).unwrap();
    let child = Command::new("python3").arg(&script).arg(dir).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    for _ in 0..100 {
        if let Ok(p) = std::fs::read_to_string(dir.join("port")) {
            if let Ok(port) = p.trim().parse() { return (child, port); }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("stub runner never came up");
}

/// A header helper that mints a new token each call, and a curl wrapper that
/// logs its argv (to prove the credential never rides on it).
fn helpers(dir: &Path) -> (PathBuf, PathBuf) {
    let helper = dir.join("headers");
    std::fs::write(&helper, format!("#!/bin/sh\nn=$(cat '{d}/n' 2>/dev/null || echo 0); n=$((n+1)); echo $n > '{d}/n'\necho \"{{\\\"Authorization\\\": \\\"Bearer tok-$n\\\"}}\"\n", d = dir.display())).unwrap();
    let curl = dir.join("curl");
    std::fs::write(&curl, format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{d}/argv.log'\nexec /usr/bin/curl \"$@\"\n", d = dir.display())).unwrap();
    for f in [&helper, &curl] {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(f, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    (helper, curl)
}

fn run_stdio(dir: &Path, url: &str, input: &str) -> (i32, String, String) {
    let (helper, curl) = helpers(dir);
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let mut c = Command::new(bin());
    c.arg("stdio").env_remove("CHORUS_ROLE").env("ROLE", "abby-normal").env("HOME", &home)
        .env("CHORUS_RUNNER_URL", url).env("CHORUS_MCP_HEADERS_BIN", &helper).env("CHORUS_CURL", &curl)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = c.spawn().unwrap();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).to_string(), String::from_utf8_lossy(&out.stderr).to_string())
}

#[test]
fn a_call_is_forwarded_as_the_role_unwrapped_and_re_minted_once_on_401() {
    let d = tmp("fwd");
    let (mut stub, port) = stub_runner(&d);
    std::fs::write(d.join("deny-once"), "").unwrap();
    let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"werk-mob-verify\"}}\n{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n";
    let (code, out, err) = run_stdio(&d, &format!("http://127.0.0.1:{port}/mcp"), input);
    let _ = stub.kill();
    assert_eq!(code, 0, "{err}");
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines, vec![r#"{"id":1,"jsonrpc":"2.0","result":{"ok":true}}"#, r#"{"id":2,"jsonrpc":"2.0","result":{"ok":true}}"#],
        "one JSON line per answer, none for the notification: {out}");
    let seen: Vec<serde_json::Value> = std::fs::read_to_string(d.join("seen.log")).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(seen[0]["auth"], "Bearer tok-1");
    assert_eq!(seen[1]["auth"], "Bearer tok-2", "a 401 re-mints once and retries");
    assert_eq!(seen.last().unwrap()["session"], "s-1", "the session id is carried after the runner sets it");
    let argv = std::fs::read_to_string(d.join("argv.log")).unwrap();
    assert!(!argv.contains("tok-"), "the credential never rides on curl's argv: {argv}");
}

#[test]
fn negative_proof_a_runner_that_is_down_is_an_error_and_nothing_runs_locally() {
    let d = tmp("down");
    let (code, out, err) = run_stdio(&d, "http://127.0.0.1:1/mcp", "{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/call\"}\n");
    assert_eq!(code, 0, "{err}");
    assert!(out.contains(r#""id":5"#) && out.contains("did not answer"), "a request is answered with an error: {out}");
    assert!(err.contains("did not answer"), "and said on stderr: {err}");
    assert!(!d.join("home/.chorus/runner-tasks").exists(), "no task record under the role's own HOME");
}

#[test]
fn negative_proof_a_401_after_a_fresh_mint_is_an_error_naming_the_role_and_no_third_try() {
    let d = tmp("deny");
    let (mut stub, port) = stub_runner(&d);
    std::fs::write(d.join("deny-always"), "").unwrap();
    let (code, out, err) = run_stdio(&d, &format!("http://127.0.0.1:{port}/mcp"), "{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"tools/list\"}\n");
    let _ = stub.kill();
    assert_eq!(code, 0, "{err}");
    assert!(out.contains(r#""id":9"#) && out.contains("abby-normal"), "the request is answered with an error naming the role: {out}");
    assert!(err.contains("after a fresh mint"), "stderr says why: {err}");
    let posts = std::fs::read_to_string(d.join("seen.log")).unwrap().lines().count();
    assert_eq!(posts, 2, "one try, one re-minted retry, never a third");
}
