// @test-type: integration
// @domain: security
//! #4851 (from #4853 AC4) — `chorus-runner call` from any account but the
//! installer goes to the shared runner, so Abby's and Borg Queen's deploy-demo
//! writes their werk slot as the installer (runner-owned, which #4853 trusts),
//! never as the role. A long tool is waited on through task_status.
use chorus_runner::forward::{call_with, forwards, task_id, task_outcome, Forwarder};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[test]
fn only_a_call_from_another_account_than_the_installer_forwards() {
    assert!(forwards(505, Some(501)), "abby-normal (505) forwards");
    assert!(!forwards(501, Some(501)), "negative proof: the installer's own call runs in process");
    assert!(!forwards(505, None), "no installed bin dir: nothing to forward to by this rule");
}

#[test]
fn a_long_tools_task_id_and_outcome_are_read() {
    assert_eq!(task_id("started werk-deploy-demo as task werk-deploy-demo-abc: call task_status with this id").as_deref(), Some("werk-deploy-demo-abc"));
    assert_eq!(task_id("done"), None);
    assert_eq!(task_outcome("t1: done (exit 3)\nlast lines"), Some(Ok(3)));
    assert_eq!(task_outcome("t1: running\n…"), None);
    assert!(matches!(task_outcome("t1: lost: the process is gone and wrote no exit code"), Some(Err(_))));
}

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("call-4851-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A stand-in shared runner: answers initialize, starts a long tool as a task,
/// says "running" once, then "done (exit 3)"; logs every method it saw.
fn stub(dir: &Path) -> (std::process::Child, u16) {
    let script = dir.join("stub.py");
    std::fs::write(&script, r#"
import http.server, json, os, sys, socketserver
D = sys.argv[1]
polls = {'n': 0}
class H(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_POST(self):
        msg = json.loads(self.rfile.read(int(self.headers.get('Content-Length', 0))).decode())
        with open(os.path.join(D, 'seen.log'), 'a') as f: f.write(msg.get('method', '') + ' ' + json.dumps(msg.get('params', {}).get('name', '')) + '\n')
        if 'id' not in msg:
            self.send_response(202); self.end_headers(); return
        m = msg['method']
        if m == 'initialize': res = {'protocolVersion': '2025-06-18', 'capabilities': {}, 'serverInfo': {'name': 'stub', 'version': '1'}}
        elif msg['params']['name'] == 'task_status':
            polls['n'] += 1
            word = 'running' if polls['n'] == 1 else 'done (exit 3)'
            res = {'content': [{'type': 'text', 'text': 't1: ' + word + '\ninstalled 2 binaries'}]}
        else: res = {'content': [{'type': 'text', 'text': 'started werk-deploy-demo as task t1: call task_status with this id'}]}
        out = 'data: ' + json.dumps({'jsonrpc': '2.0', 'id': msg['id'], 'result': res}) + '\n\n'
        self.send_response(200); self.send_header('Content-Type', 'text/event-stream'); self.send_header('Mcp-Session-Id', 's-1'); self.end_headers()
        self.wfile.write(out.encode())
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

#[test]
fn a_long_call_goes_to_the_shared_runner_and_returns_the_tasks_exit_code() {
    let d = tmp("fwd");
    let (mut s, port) = stub(&d);
    let helper = d.join("headers");
    std::fs::write(&helper, "#!/bin/sh\necho '{\"Authorization\": \"Bearer t\"}'\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::env::set_var("CHORUS_RUNNER_URL", format!("http://127.0.0.1:{port}/mcp"));
    std::env::set_var("CHORUS_MCP_HEADERS_BIN", &helper);
    let (mut f, dir) = Forwarder::for_role("abby-normal", "test").unwrap();
    let code = call_with(&mut f, "werk-deploy-demo", &serde_json::json!({"card": "4851"}), std::time::Duration::from_millis(10));
    let _ = s.kill();
    let _ = std::fs::remove_dir_all(dir);
    assert_eq!(code, 3, "the task's own exit code");
    let seen = std::fs::read_to_string(d.join("seen.log")).unwrap();
    assert!(seen.contains("tools/call \"werk-deploy-demo\"") && seen.matches("task_status").count() == 2, "{seen}");

    // negative proof: a shared runner that is down is an error, and nothing ran locally
    std::env::set_var("CHORUS_RUNNER_URL", "http://127.0.0.1:1/mcp");
    let (mut f, dir) = Forwarder::for_role("abby-normal", "down").unwrap();
    let code = call_with(&mut f, "werk-deploy-demo", &serde_json::json!({}), std::time::Duration::from_millis(10));
    let _ = std::fs::remove_dir_all(dir);
    assert_ne!(code, 0);
    std::env::remove_var("CHORUS_RUNNER_URL");
    std::env::remove_var("CHORUS_MCP_HEADERS_BIN");
}
