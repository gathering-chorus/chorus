//! #4028/#4077 — the pulse's role rows come from chorus-api's derived
//! /api/chorus/context/roles, never from a declared file. Own world: a stub.

use chorus_hooks::shared::roles_api::roles_from_api;
use std::io::{Read, Write};
use std::net::TcpListener;

fn stub_server(response: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut sock, _)) = listener.accept() {
            let mut buf = [0u8; 2048];
            let _ = sock.read(&mut buf);
            let _ = sock.write_all(response.as_bytes());
        }
    });
    format!("http://{addr}")
}

fn json_ok(body: &str) -> &'static str {
    Box::leak(format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    ).into_boxed_str())
}

#[test]
fn pulse_roles_are_the_api_rows_keyed_by_role() {
    let base = stub_server(json_ok(r#"{"data":{"roles":[
      {"role":"silas","state":"building","card":4064,"source":"streams"},
      {"role":"wren","state":"waiting","card":4028,"source":"streams"},
      {"role":"kade","state":"idle","card":null,"source":"streams"}]}}"#));
    let v = roles_from_api(&format!("{base}/api/chorus/context/roles"));
    assert_eq!(v["wren"]["state"], "waiting");
    assert_eq!(v["silas"]["card"], 4064);
    assert_eq!(v["kade"]["source"], "streams");
    assert!(v.get("unknown").is_none());
}

/// Negative proof (#3734): with the API down the pulse says UNMEASURED and
/// names why — it does not say "unknown" and it does not read any file.
#[test]
fn unreachable_api_yields_unmeasured_rows_not_unknown() {
    let dead = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", l.local_addr().unwrap())
    };
    let v = roles_from_api(&format!("{dead}/api/chorus/context/roles"));
    for role in ["wren", "silas", "kade"] {
        assert_eq!(v[role]["state"], "unmeasured", "{role}");
        assert_eq!(v[role]["source"], "streams-unreachable");
        assert_ne!(v[role]["state"], "unknown");
    }
}
