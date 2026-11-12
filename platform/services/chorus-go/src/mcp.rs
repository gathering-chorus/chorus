//! #4710 AC3 — an approved card is filed through the MCP door as the lander (Silas 18:08: "cards
//! over HTTP as lander, never the cards script"). The go page used to run scripts/cards, a program
//! jeffbridwell and roles can write; as the lander it would run with the App key readable.
//!
//! One MCP session per call (streamable HTTP, measured 18:22: the door wants `initialize` first,
//! answers with an Mcp-Session-Id, then `notifications/initialized`, then `tools/call`). The
//! lander's login goes to curl on stdin (`-H @-`), never argv; bodies go in 0600 files.
use crate::R;
use serde_json::{json, Map, Value};
use std::io::Write;
use std::process::{Command, Stdio};

pub fn door() -> String {
    std::env::var("CHORUS_GO_MCP").unwrap_or_else(|_| "http://127.0.0.1:3342/mcp".into())
}

/// The clearing-card-jeff arguments from an approved `cards add` argv (lib cards_add_argv has
/// already refused anything that is not "add", a title and allow-listed `--flag value` pairs).
/// The cards CLI required domain, type, priority and origin, so an approved argv carries them;
/// --owner was optional there and is required by the tool, so it defaults to the role that asked
/// (review of 746df662).
pub fn card_args(argv: &[String], asker: &str) -> R<Value> {
    if argv.len() < 2 || argv[0] != "add" { return Err("not a cards add argv".into()); }
    let mut m = Map::new();
    m.insert("title".into(), json!(argv[1]));
    m.insert("owner".into(), json!(asker.trim_start_matches("principal-")));
    for pair in argv[2..].chunks(2) {
        let [flag, val] = pair else { return Err("a flag without a value".into()) };
        let key = flag.trim_start_matches("--");
        m.insert(key.into(), json!(val));
    }
    Ok(Value::Object(m))
}

/// curl's argv for one MCP request: what `ps` shows. The login comes from stdin; the body from
/// `body`, a 0600 file.
pub fn mcp_argv(url: &str, body: &std::path::Path, session: Option<&str>) -> Vec<String> {
    let mut a: Vec<String> = ["-sS", "--max-time", "30", "-D", "-", "-X", "POST", "-H", "@-",
        "-H", "Content-Type: application/json", "-H", "Accept: application/json, text/event-stream"]
        .iter().map(|s| s.to_string()).collect();
    if let Some(s) = session { a.extend(["-H".into(), format!("Mcp-Session-Id: {s}")]); }
    a.extend(["--data-binary".into(), format!("@{}", body.display()), url.into()]);
    a
}

fn post(url: &str, login: &str, msg: &Value, session: Option<&str>) -> R<(String, String)> {
    // a private, randomly named body file (review of 113c9fe51), never a predictable /tmp name
    let f = crate::github::private_body_file(msg.to_string().as_bytes())?;
    let child = Command::new("/usr/bin/curl").args(mcp_argv(url, &f, session))
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
    let out = child.and_then(|mut c| {
        if let Some(mut i) = c.stdin.take() { let _ = writeln!(i, "Authorization: Bearer {login}"); }
        c.wait_with_output()
    });
    let _ = std::fs::remove_file(&f);
    let out = out.map_err(|e| format!("mcp: curl: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let (head, body) = text.split_once("\n\n").unwrap_or((text.as_str(), ""));
    Ok((head.to_string(), body.to_string()))
}

/// The JSON-RPC answer inside a reply that may be plain JSON or one SSE `data:` line.
pub fn rpc_answer(body: &str) -> Option<Value> {
    serde_json::from_str(body.trim()).ok()
        .or_else(|| body.lines().filter_map(|l| l.strip_prefix("data:")).find_map(|d| serde_json::from_str(d.trim()).ok()))
}

/// Call one MCP tool as the lander; the tool's text, or why it refused.
pub fn call_tool(login: &str, tool: &str, args: Value) -> R<String> {
    let url = door();
    let (head, _) = post(&url, login, &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "chorus-lander", "version": "1"}}}), None)?;
    let session = head.lines().find_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case("mcp-session-id")).map(|(_, v)| v.trim().to_string()))
        .ok_or("the MCP door gave no session")?;
    post(&url, login, &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}), Some(&session))?;
    let (_, body) = post(&url, login, &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": {"name": tool, "arguments": args}}), Some(&session))?;
    let v = rpc_answer(&body).ok_or("the MCP door's answer is not JSON-RPC")?;
    if let Some(e) = v.get("error") { return Err(format!("{tool}: {}", e.get("message").and_then(Value::as_str).unwrap_or("refused"))); }
    let r = v.get("result").ok_or("no result")?;
    let text: String = r.get("content").and_then(Value::as_array).into_iter().flatten()
        .filter_map(|c| c.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n");
    if r.get("isError").and_then(Value::as_bool) == Some(true) { return Err(format!("{tool}: {text}")); }
    Ok(text)
}
