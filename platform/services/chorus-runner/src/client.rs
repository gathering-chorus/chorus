//! #4514 AC4 — `chorus-runner call <tool> [json]`: one tool call through the
//! runner, signed as $ROLE. The cicd pipeline's steps call the verbs this way
//! (one path: dagu calls the runner too). A long tool's run id is followed
//! through task_status until it finishes, and the call exits with the verb's
//! own exit code, so a dagu step goes red exactly when the verb did.

use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

pub const PROTOCOL: &str = "2026-07-28";

/// The tools/call request body, in the 2026-07-28 stateless shape.
pub fn call_body(tool: &str, args: &Value) -> String {
    json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {
            "name": tool, "arguments": args,
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": PROTOCOL,
                "io.modelcontextprotocol/clientInfo": { "name": "chorus-runner-call", "version": env!("CARGO_PKG_VERSION") },
                "io.modelcontextprotocol/clientCapabilities": {}
            }
        }
    })
    .to_string()
}

/// `key=value` pairs as tool arguments, as text; the runner turns each into
/// the type its schema declares. A key given twice becomes a list, in order.
/// A pair without `=` is refused.
pub fn args_from_pairs(pairs: &[String]) -> Result<Value, String> {
    let mut m = serde_json::Map::new();
    for p in pairs {
        let (k, v) = p.split_once('=').ok_or_else(|| format!("{p}: arguments are key=value"))?;
        match m.get_mut(k) {
            Some(Value::Array(a)) => a.push(json!(v)),
            Some(prev) => *prev = json!([prev.clone(), v]),
            None => {
                m.insert(k.to_string(), json!(v));
            }
        }
    }
    Ok(Value::Object(m))
}

/// (is_error, text) from a tools/call response; an unreadable one is an error.
pub fn parse_result(resp: &str) -> (bool, String) {
    let body = resp.find('{').map(|i| &resp[i..]).unwrap_or("");
    let Ok(v) = serde_json::from_str::<Value>(body.lines().next().unwrap_or(body)).or_else(|_| serde_json::from_str::<Value>(body)) else {
        return (true, format!("the runner's answer was not JSON: {}", resp.chars().take(300).collect::<String>()));
    };
    if let Some(e) = v.get("error") {
        return (true, e.get("message").and_then(Value::as_str).unwrap_or("error").to_string());
    }
    let r = &v["result"];
    let text = r["content"].as_array().into_iter().flatten().filter_map(|c| c["text"].as_str()).collect::<Vec<_>>().join("\n");
    (r["isError"].as_bool().unwrap_or(false), text)
}

/// The run id in a long tool's "started <tool> as task <id>: …" answer.
pub fn task_id(text: &str) -> Option<String> {
    let rest = text.split(" as task ").nth(1)?;
    Some(rest.split(':').next()?.trim().to_string()).filter(|s| !s.is_empty())
}

/// From task_status text: Some(code) when done, Some(1) when lost, None while running.
pub fn finished(status_text: &str) -> Option<i32> {
    let first = status_text.lines().next().unwrap_or("");
    if let Some(rest) = first.split("done (exit ").nth(1) {
        return rest.split(')').next().and_then(|c| c.trim().parse().ok()).or(Some(1));
    }
    if first.contains(": lost") || first.contains("unknown task") {
        return Some(1);
    }
    None
}

fn post(port: u16, token: &str, tool_header: &str, body: &str) -> Result<String, String> {
    // the token and body go through stdin and a 0600 file, never the command
    // line, where any process listing would show them
    let dir = crate::tasks::tasks_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let body_file = dir.join(format!(".call-{}.json", std::process::id()));
    std::fs::write(&body_file, body).map_err(|e| e.to_string())?;
    let config = format!(
        "url = \"http://127.0.0.1:{port}/mcp\"\nsilent\nmax-time = 60\nheader = \"Content-Type: application/json\"\nheader = \"Accept: application/json, text/event-stream\"\nheader = \"MCP-Protocol-Version: {PROTOCOL}\"\nheader = \"Mcp-Method: tools/call\"\nheader = \"Mcp-Name: {tool_header}\"\nheader = \"Authorization: Bearer {token}\"\ndata-binary = \"@{}\"\n",
        body_file.display()
    );
    let mut child = Command::new("curl").args(["-K", "-"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    child.stdin.take().map(|mut s| s.write_all(config.as_bytes()));
    let out = child.wait_with_output().map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&body_file);
    let out = out?;
    if !out.status.success() {
        return Err(format!("the runner on :{port} did not answer ({})", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn token_for(role: &str) -> Result<String, String> {
    let home = std::env::var("CHORUS_HOME").unwrap_or_else(|_| format!("{}/CascadeProjects/chorus", std::env::var("HOME").unwrap_or_default()));
    let out = Command::new(format!("{home}/platform/scripts/chorus-identity-token")).arg(role).output().map_err(|e| e.to_string())?;
    let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !t.is_empty()).then_some(t).ok_or_else(|| format!("could not sign as {role}"))
}

/// The whole call; returns the process exit code.
pub fn run(tool: &str, args: &Value) -> i32 {
    let role = std::env::var("ROLE").or_else(|_| std::env::var("CHORUS_ROLE")).unwrap_or_default();
    if role.is_empty() {
        eprintln!("chorus-runner call: no ROLE to sign as");
        return 2;
    }
    let port: u16 = std::env::var("CHORUS_RUNNER_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3342);
    let token = match token_for(&role) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("chorus-runner call: {e}");
            return 2;
        }
    };
    let (is_error, text) = match post(port, &token, tool, &call_body(tool, args)) {
        Ok(r) => parse_result(&r),
        Err(e) => (true, e),
    };
    println!("{text}");
    if is_error {
        return 1;
    }
    let Some(id) = task_id(&text) else { return 0 };
    let poll = Duration::from_secs(std::env::var("CHORUS_RUNNER_POLL_S").ok().and_then(|s| s.parse().ok()).unwrap_or(5));
    loop {
        std::thread::sleep(poll);
        let (err, status) = match post(port, &token, "task_status", &call_body("task_status", &json!({ "id": id }))) {
            Ok(r) => parse_result(&r),
            // the runner may be restarting: the Task keeps going, so keep asking
            Err(_) => continue,
        };
        if err {
            eprintln!("{status}");
            return 1;
        }
        if let Some(code) = finished(&status) {
            println!("{status}");
            return code;
        }
    }
}
