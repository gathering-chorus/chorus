//! #4295 — sign in and sign out, the decisions only. Pure; unit-tested.
//!
//! Jeff, 2026-09-25: "we wanted chorus-principal on|off to start and exit";
//! "i need the login and logout experienced designed first ... i dont want a
//! swat on every reboot"; "ok if u can automate all of that that is very
//! helpful for me my bar for friction here is low though"; "a negative signin
//! experience is a problem".
//!
//! The 09:44 reboot that morning is the case this answers: two roles came up
//! with no login, `chorus-awake` then said "already awake ... registered yes"
//! over both, and a peer's session started Silas. So:
//!   - the login state is WRITTEN DOWN per role and read back by `status`
//!     and by `on`, never inferred from a pid
//!   - a running role with no login is logged in, not blessed
//!   - a login that cannot happen yet retries on its own and says "pending"
//!   - only Jeff's shell (or the role itself) may start or stop a role

use serde_json::Value;

/// What `on` / `relogin` wrote about a role's login, read back from
/// ~/.chorus/identity/<role>/login.json.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginState {
    /// a Session row was written for this start
    Recorded { session: String, pid: Option<u64> },
    /// no row yet; `why` is the reason in words; a retry is running
    Pending { why: String, pid: Option<u64> },
    /// `off` (or /exit) closed it
    Closed,
    /// no file, or a file this build cannot read
    Unknown,
}

impl LoginState {
    pub fn pid(&self) -> Option<u64> {
        match self { LoginState::Recorded { pid, .. } | LoginState::Pending { pid, .. } => *pid, _ => None }
    }
    pub fn with_pid(self, p: u64) -> LoginState {
        match self {
            LoginState::Recorded { session, .. } => LoginState::Recorded { session, pid: Some(p) },
            LoginState::Pending { why, .. } => LoginState::Pending { why, pid: Some(p) },
            other => other,
        }
    }
}

pub fn parse_login_state(json: &str) -> LoginState {
    let Ok(v) = serde_json::from_str::<Value>(json) else { return LoginState::Unknown };
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let pid = v.get("pid").and_then(|p| p.as_u64());
    match s("state").as_str() {
        "recorded" if !s("session").is_empty() => LoginState::Recorded { session: s("session"), pid },
        "pending" => LoginState::Pending { why: s("why"), pid },
        "closed" => LoginState::Closed,
        _ => LoginState::Unknown,
    }
}

pub fn login_state_json(st: &LoginState, at: &str) -> String {
    let v = match st {
        LoginState::Recorded { session, pid } => serde_json::json!({"state":"recorded","session":session,"pid":pid,"at":at}),
        LoginState::Pending { why, pid } => serde_json::json!({"state":"pending","why":why,"pid":pid,"at":at}),
        LoginState::Closed => serde_json::json!({"state":"closed","at":at}),
        LoginState::Unknown => serde_json::json!({"state":"unknown","at":at}),
    };
    v.to_string()
}

/// Does the written-down login belong to THIS running session? A recorded
/// login from an earlier session (a crash, a bare `claude -c`) is not this
/// one's, and reading it as this one's is the "registered yes" lie again.
/// A login with no pid yet is the one `on` wrote just before the pane
/// registered; it is adopted by the session that registered.
pub fn login_belongs_to(st: &LoginState, live_pid: u64) -> bool {
    match st.pid() { Some(p) => p == live_pid, None => matches!(st, LoginState::Recorded { .. } | LoginState::Pending { .. }) }
}

/// The login word `status` and the tmux bar show.
pub fn login_word(st: &LoginState, live_pid: Option<u64>) -> &'static str {
    match (st, live_pid) {
        (_, None) => "",
        (LoginState::Recorded { .. }, Some(p)) if login_belongs_to(st, p) => "logged in",
        (LoginState::Pending { .. }, Some(p)) if login_belongs_to(st, p) => "login pending",
        _ => "NOT logged in",
    }
}

/// One line per role for `chorus-principal status`.
pub fn status_line(role: &str, live_pid: Option<u64>, st: &LoginState, answering: bool) -> String {
    let Some(pid) = live_pid else { return format!("{:<6} off", role) };
    let mut line = format!("{:<6} running  {:<14} {}", role, login_word(st, Some(pid)), if answering { "answering" } else { "NOT answering" });
    if let LoginState::Pending { why, .. } = st {
        if login_belongs_to(st, pid) && !why.is_empty() { line.push_str(&format!("   ({}, retrying)", why)); }
    }
    line
}

/// Only Jeff's shell, or the role's own session, may start or stop a role.
/// `in_agent` is true inside a Claude Code session (CLAUDECODE set); the 09-25
/// case is Kade's session running `chorus-awake silas` from roles/silas.
pub fn caller_may_act(in_agent: bool, caller_role: Option<&str>, target: &str, verb: &str) -> Result<(), String> {
    if !in_agent { return Ok(()); }
    match caller_role {
        Some(r) if r == target => Ok(()),
        Some(r) => Err(format!("a {} session cannot {} {}. Only Jeff's shell starts or stops a role.", r, verb, target)),
        None => Err(format!("an agent session with no role cannot {} {}. Only Jeff's shell starts or stops a role.", verb, target)),
    }
}

/// An HTTP answer that means the service is up. 000 is no answer; a 5xx is a
/// service that is running and not ready (the 502s after every reboot).
pub fn is_answering(code: &str) -> bool {
    let c = code.trim();
    c.len() == 3 && c.chars().all(|x| x.is_ascii_digit()) && c != "000" && !c.starts_with('5')
}

fn mmss(s: u64) -> String { format!("{}:{:02}", s / 60, s % 60) }

/// "waiting for identity :3001 ... 0:42 of 3:00"
pub fn countdown_line(service: &str, url: &str, elapsed: u64, bound: u64) -> String {
    let port = url.split("://").nth(1).and_then(|h| h.split('/').next()).and_then(|h| h.rsplit(':').next()).unwrap_or("");
    format!("waiting for {} :{} ... {} of {}", service, port, mmss(elapsed), mmss(bound))
}

/// AWAKE_SERVICES = "name=url,name=url" or "none".
pub fn parse_services(spec: &str) -> Vec<(String, String)> {
    if spec.trim() == "none" { return vec![]; }
    spec.split(',').filter_map(|p| { let (n, u) = p.split_once('=')?; Some((n.trim().to_string(), u.trim().to_string())) }).filter(|(n, u)| !n.is_empty() && !u.is_empty()).collect()
}

pub const DEFAULT_SERVICES: &str = "identity=http://localhost:3001/,chorus-api=http://localhost:3340/api/chorus/health,athena-make=http://localhost:3360/v1/identity/sessions?limit=1";

/// What each role ended up as after `up`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Came { LoggedIn, Pending(String), NotStarted(String) }

/// The one line Jeff gets after a reboot.
pub fn summary_line(results: &[(String, Came)]) -> String {
    let names: Vec<&str> = results.iter().filter(|(_, c)| !matches!(c, Came::NotStarted(_))).map(|(r, _)| r.as_str()).collect();
    let ok = results.iter().filter(|(_, c)| *c == Came::LoggedIn).count();
    let mut line = if names.is_empty() { "no role started".to_string() } else { format!("{} up, {} of {} logged in", names.join(" "), ok, results.len()) };
    for (r, c) in results {
        match c {
            Came::Pending(why) => line.push_str(&format!("; {} login pending ({}), retrying on its own", r, why)),
            Came::NotStarted(why) => line.push_str(&format!("; {} NOT started ({})", r, why)),
            Came::LoggedIn => {}
        }
    }
    line
}

/// Claude Code's SessionEnd reasons that mean the person left. `clear` starts a
/// new conversation in the same session and must not log the role out.
pub fn exit_reason_logs_out(reason: &str) -> bool { matches!(reason, "prompt_input_exit" | "logout") }

/// The Session row with its end written in, for the PUT that closes it.
pub fn closed_row(existing: &str, ended_at: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(existing).ok()?;
    let mut row = v.get("data").cloned().unwrap_or(v);
    let o = row.as_object_mut()?;
    o.get("name")?;
    o.insert("sessionState".into(), Value::String("closed".into()));
    o.insert("endedAt".into(), Value::String(ended_at.into()));
    Some(row)
}

/// The tmux bar's right side for a role pane.
pub fn tmux_status_right(word: &str) -> String {
    let w = if word.is_empty() { "off" } else { word };
    format!(" {} | %H:%M %d-%b ", w)
}

/// VS Code task that attaches Wren when VS Code opens the chorus folder.
pub fn vscode_tasks_json(principal_bin: &str) -> String {
    serde_json::to_string_pretty(&serde_json::json!({
        "version": "2.0.0",
        "tasks": [{
            "label": "wren",
            "type": "shell",
            "command": format!("{} on wren", principal_bin),
            "runOptions": {"runOn": "folderOpen"},
            "presentation": {"reveal": "always", "panel": "dedicated", "focus": true, "clear": true},
            "problemMatcher": []
        }]
    })).unwrap_or_default()
}
