//! #4851 — `chorus-runner stdio` is a forwarder, not a second runner.
//!
//! Abby's and Borg Queen's agy takes an MCP server's headers once, at launch,
//! and a role credential lives about ten minutes (#4515), so agy cannot speak
//! to the HTTP runner (:3342) directly. Until 11-01 the stdio mode ran every
//! tool itself, as the role's own account: its task records landed in a store
//! under the role's HOME, a store the role can write, so a mob check (which
//! trusts only the runner's own store, #4789) never counted their verdicts.
//!
//! Now stdio runs nothing. Each JSON-RPC line from stdin is POSTed to the HTTP
//! runner with the role's credential, minted as this process's own account
//! from its own home by `chorus-mcp-headers <role>` (re-run once on a 401/403,
//! as Claude Code's headersHelper does). The reply goes to stdout one JSON
//! line per message: a text/event-stream reply is unwrapped event by event.
//! The runner, not this process, verifies the caller and writes the record.
//!
//! Nothing secret is put on a command line: the Authorization header reaches
//! curl on stdin (`-H @-`). The request body goes through a file in a 0700
//! scratch dir, since stdin is already taken by the header.
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::process::{Command, Stdio};

/// What one HTTP reply was: its status, content type and session id.
#[derive(Debug, Default, PartialEq)]
pub struct Reply {
    pub status: u16,
    pub content_type: String,
    pub session: Option<String>,
}

/// Parse curl's `-D` header dump. The LAST status line wins (a 100-continue or
/// a redirect leaves an earlier one).
pub fn parse_headers(dump: &str) -> Reply {
    let mut r = Reply::default();
    for line in dump.lines() {
        let line = line.trim_end_matches('\r');
        if line.starts_with("HTTP/") {
            r = Reply { status: line.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0), ..Default::default() };
        } else if let Some((k, v)) = line.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "content-type" => r.content_type = v.trim().to_string(),
                "mcp-session-id" => r.session = Some(v.trim().to_string()).filter(|s| !s.is_empty()),
                _ => {}
            }
        }
    }
    r
}

/// The JSON-RPC messages in a reply body: one per SSE `data:` event (multi-line
/// data joined with newlines, per the SSE spec), or the body itself when it is
/// plain JSON. Empty for an empty body (a 202 to a notification).
pub fn messages(content_type: &str, body: &str) -> Vec<String> {
    if content_type.to_ascii_lowercase().starts_with("text/event-stream") {
        let mut out = Vec::new();
        let mut data: Vec<&str> = Vec::new();
        for line in body.lines().chain(std::iter::once("")) {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                if !data.is_empty() {
                    let joined = data.join("\n");
                    if let Ok(v) = serde_json::from_str::<Value>(&joined) {
                        out.push(v.to_string());
                    }
                    data.clear();
                }
            } else if let Some(d) = line.strip_prefix("data:") {
                data.push(d.strip_prefix(' ').unwrap_or(d));
            }
        }
        out
    } else {
        let t = body.trim();
        if t.is_empty() { return Vec::new(); }
        match serde_json::from_str::<Value>(t) {
            Ok(v) => vec![v.to_string()],
            Err(_) => Vec::new(),
        }
    }
}

/// A JSON-RPC error answer for the request `line` (nothing for a notification:
/// it has no id to answer).
pub fn error_for(line: &str, message: &str) -> Option<String> {
    let v: Value = serde_json::from_str(line).ok()?;
    let id = v.get("id")?.clone();
    Some(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": message}}).to_string())
}

struct Forwarder {
    role: String,
    url: String,
    helper: String,
    curl: String,
    dir: std::path::PathBuf,
    headers: Option<String>,
    session: Option<String>,
}

impl Forwarder {
    /// `{"Authorization": "..."}` from the helper, as `Name: value` lines.
    fn mint(&mut self) -> Result<String, String> {
        let out = Command::new(&self.helper).arg(&self.role).output()
            .map_err(|e| format!("could not mint {}'s credential as this account ({}: {e})", self.role, self.helper))?;
        if !out.status.success() {
            return Err(format!("could not mint {}'s credential as this account", self.role));
        }
        let v: Value = serde_json::from_slice(&out.stdout).map_err(|_| "the header helper did not print JSON".to_string())?;
        let lines: Vec<String> = v.as_object().ok_or("the header helper did not print an object")?
            .iter().filter_map(|(k, v)| v.as_str().map(|s| format!("{k}: {s}"))).collect();
        if lines.is_empty() { return Err("the header helper printed no headers".into()); }
        Ok(lines.join("\n"))
    }

    fn post(&mut self, line: &str) -> Result<(Reply, String), String> {
        if self.headers.is_none() { self.headers = Some(self.mint()?); }
        let body = self.dir.join("body.json");
        let dump = self.dir.join("headers.txt");
        std::fs::write(&body, line).map_err(|e| format!("body: {e}"))?;
        let mut args: Vec<String> = vec!["-sS".into(), "--max-time".into(), "120".into(), "-X".into(), "POST".into(),
            "-H".into(), "@-".into(), "-H".into(), "Content-Type: application/json".into(),
            "-H".into(), "Accept: application/json, text/event-stream".into(),
            "-D".into(), dump.display().to_string(), "--data-binary".into(), format!("@{}", body.display())];
        if let Some(s) = &self.session { args.push("-H".into()); args.push(format!("Mcp-Session-Id: {s}")); }
        args.push(self.url.clone());
        let mut child = Command::new(&self.curl).args(&args).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
            .spawn().map_err(|e| format!("could not run curl: {e}"))?;
        {
            let mut stdin = child.stdin.take().ok_or("curl: no stdin")?;
            writeln!(stdin, "{}", self.headers.as_deref().unwrap_or("")).map_err(|e| format!("curl: {e}"))?;
        }
        let out = child.wait_with_output().map_err(|e| format!("curl: {e}"))?;
        let _ = std::fs::remove_file(&body);
        if !out.status.success() {
            return Err(format!("the HTTP runner at {} did not answer: {}", self.url, String::from_utf8_lossy(&out.stderr).trim()));
        }
        let reply = parse_headers(&std::fs::read_to_string(&dump).unwrap_or_default());
        Ok((reply, String::from_utf8_lossy(&out.stdout).to_string()))
    }

    /// One stdin line → the stdout lines for it. A 401/403 re-mints once.
    fn forward(&mut self, line: &str) -> Result<Vec<String>, String> {
        let (mut reply, mut body) = self.post(line)?;
        if reply.status == 401 || reply.status == 403 {
            self.headers = Some(self.mint()?);
            (reply, body) = self.post(line)?;
        }
        if reply.status == 401 || reply.status == 403 {
            return Err(format!("the HTTP runner refused {}'s credential ({}) after a fresh mint", self.role, reply.status));
        }
        if reply.status >= 400 || reply.status == 0 {
            return Err(format!("the HTTP runner answered {}", reply.status));
        }
        if let Some(s) = reply.session.take() { self.session = Some(s); }
        Ok(messages(&reply.content_type, &body))
    }
}

/// Run the forwarder until stdin closes. Every failure is said on stderr and,
/// for a request, answered as a JSON-RPC error; nothing is ever run locally.
pub fn run() -> Result<(), String> {
    let role = std::env::var("ROLE").or_else(|_| std::env::var("CHORUS_ROLE")).unwrap_or_default();
    if role.is_empty() { return Err("no ROLE to forward as; nothing runs as nobody".into()); }
    let home = std::env::var("CHORUS_HOME").unwrap_or_else(|_| format!("{}/CascadeProjects/chorus", std::env::var("HOME").unwrap_or_default()));
    let dir = std::env::temp_dir().join(format!("chorus-runner-stdio-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("scratch dir: {e}"))?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).map_err(|e| format!("scratch dir: {e}"))?;
    }
    let mut f = Forwarder {
        role: role.clone(),
        url: std::env::var("CHORUS_RUNNER_URL").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| "http://localhost:3342/mcp".into()),
        helper: std::env::var("CHORUS_MCP_HEADERS_BIN").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| format!("{home}/platform/scripts/chorus-mcp-headers")),
        curl: std::env::var("CHORUS_CURL").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| "/usr/bin/curl".into()),
        dir: dir.clone(),
        headers: None,
        session: None,
    };
    // Fail loud at start: a role that cannot mint never pretends to serve.
    f.headers = Some(f.mint()?);
    eprintln!("chorus-runner stdio: forwarding as {role} to {}", f.url);
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line.map_err(|e| format!("stdin: {e}"))?;
        if line.trim().is_empty() { continue; }
        match f.forward(&line) {
            Ok(msgs) => for m in msgs { writeln!(stdout, "{m}").map_err(|e| format!("stdout: {e}"))?; },
            Err(why) => {
                eprintln!("chorus-runner stdio: {why}");
                if let Some(e) = error_for(&line, &why) { writeln!(stdout, "{e}").map_err(|e| format!("stdout: {e}"))?; }
            }
        }
        stdout.flush().map_err(|e| format!("stdout: {e}"))?;
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
