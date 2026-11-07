// @domain: properties — the product domain this test guards (#4538)
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::thread;

fn spawn_stub(handler: impl Fn(&str) -> (u16, String) + Send + 'static, requests: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let addr = listener.local_addr().expect("local addr");
    thread::spawn(move || {
        for _ in 0..requests {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 2048];
                let n = stream.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]);
                let first_line = req.lines().next().unwrap_or("");
                let path = first_line.split_whitespace().nth(1).unwrap_or("/");
                let (code, body) = handler(path);
                let status = if code == 200 { "200 OK" } else { "404 Not Found" };
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
            }
        }
    });
    format!("http://{}", addr)
}

#[test]
fn resolve_fetches_effective_property_from_door_and_refuses_404() {
    let url = spawn_stub(
        |path| {
            if path == "/effective/role-borg-queen/response.word.cap" {
                (
                    200,
                    r#"{"node":"role-borg-queen","key":"response.word.cap","value":100,"valueType":"integer","winningScope":"https://jeffbridwell.com/chorus#role-borg-queen"}"#.to_string(),
                )
            } else if path == "/effective/pulse/testType" {
                (
                    200,
                    r#"{"node":"pulse","key":"testType","value":"unit","valueType":"string","winningScope":"https://jeffbridwell.com/chorus#pulse"}"#.to_string(),
                )
            } else {
                (
                    404,
                    r#"{"error":"no property sets key","node":"role-borg-queen","key":"missing.key"}"#.to_string(),
                )
            }
        },
        3,
    );

    let bin = env!("CARGO_BIN_EXE_properties-resolver");

    // 1. Direct node + key resolution (integer value)
    let out = Command::new(bin)
        .env("ATHENA_MAKE_URL", &url)
        .args(["resolve", "role-borg-queen", "response.word.cap"])
        .output()
        .expect("run properties-resolver");
    assert!(
        out.status.success(),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "100");

    // 2. Scope + name + key resolution (string value)
    let out_str = Command::new(bin)
        .env("ATHENA_MAKE_URL", &url)
        .args(["resolve", "--scope", "service", "--name", "pulse", "--key", "testType"])
        .output()
        .expect("run properties-resolver with flags");
    assert!(
        out_str.status.success(),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&out_str.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out_str.stdout).trim(), "unit");

    // 3. NEGATIVE PROOF (#3734): a 404 (unset key) fails loudly with non-zero exit and empty stdout.
    let miss = Command::new(bin)
        .env("ATHENA_MAKE_URL", &url)
        .args(["resolve", "role-borg-queen", "missing.key"])
        .output()
        .expect("run properties-resolver on missing key");
    assert!(
        !miss.status.success(),
        "unset property must exit non-zero, never silently succeed"
    );
    assert!(
        String::from_utf8_lossy(&miss.stdout).trim().is_empty(),
        "unset property must never emit a default on stdout"
    );
}

#[test]
fn roles_lists_agent_roles_in_slot_order_and_refuses_empty_payload() {
    let url = spawn_stub(
        |path| {
            if path.starts_with("/v1/roles/roles") {
                (
                    200,
                    r#"{ "apiVersion": "v1", "kind": "Role", "data": [
  { "name": "abby-normal", "label": "role-abby-normal", "roleKind": "agent", "rolePriority": "4" },
  { "name": "borg-queen", "label": "role-borg-queen", "roleKind": "agent", "rolePriority": "5" },
  { "name": "crawler", "label": "crawler", "roleKind": "automation", "rolePriority": "" },
  { "name": "jeff", "label": "Jeff", "roleKind": "human", "rolePriority": "0" },
  { "name": "kade", "label": "Kade", "roleKind": "agent", "rolePriority": "1" },
  { "name": "silas", "label": "Silas", "roleKind": "agent", "rolePriority": "2" },
  { "name": "wren", "label": "Wren", "roleKind": "agent", "rolePriority": "3" },
  { "name": "zz-probe-role", "label": "zz", "roleKind": "zz-probe", "rolePriority": "" }
]}"#.to_string(),
                )
            } else {
                (404, r#"{"error":"not found"}"#.to_string())
            }
        },
        1,
    );

    let bin = env!("CARGO_BIN_EXE_properties-resolver");
    let out = Command::new(bin)
        .env("ATHENA_MAKE_URL", &url)
        .args(["roles"])
        .output()
        .expect("run properties-resolver roles");
    assert!(
        out.status.success(),
        "expected exit 0, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines: Vec<&str> = std::str::from_utf8(&out.stdout)
        .unwrap()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(
        lines,
        vec!["silas", "kade", "wren", "abby-normal", "borg-queen"]
    );

    // NEGATIVE PROOF (#3734): a door response with no agent roles fails loudly.
    let empty_url = spawn_stub(
        |_| {
            (
                200,
                r#"{ "apiVersion": "v1", "kind": "Role", "data": [
  { "name": "jeff", "label": "Jeff", "roleKind": "human", "rolePriority": "0" }
]}"#.to_string(),
            )
        },
        1,
    );
    let empty_out = Command::new(bin)
        .env("ATHENA_MAKE_URL", &empty_url)
        .args(["roles"])
        .output()
        .expect("run properties-resolver roles on empty agent list");
    assert!(
        !empty_out.status.success(),
        "empty agent role set must fail non-zero"
    );
}
