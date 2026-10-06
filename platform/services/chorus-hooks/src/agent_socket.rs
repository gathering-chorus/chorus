//! #4432 — the hooks socket for agent principals that run as their own Mac
//! account (Abby Normal, uid 505). The main socket stays owner-only for the
//! three roles on Jeff's account; this one is group `chorus`, and on it the OS
//! says who is calling, never the payload (Jeff's identity rule, asked three
//! times; Wren 2026-10-06 09:04):
//!
//! - peer uid → account name → the one principal whose hostAccount it is
//!   (principals door) → that principal's role;
//! - an account with no principal, or hosting more than one, is refused loudly;
//! - whatever deploy_role the payload claims is overwritten with that role.

use axum::{
    body::{to_bytes, Body},
    extract::{connect_info::Connected, ConnectInfo, Request},
    middleware::Next,
    response::{IntoResponse, Response},
    serve::IncomingStream,
    Json,
};
use serde_json::{json, Value};
use tokio::net::UnixListener;

/// The connecting process's uid, read from the socket (getpeereid), never from the request.
#[derive(Clone, Copy, Debug)]
pub struct AgentPeer {
    pub uid: Option<u32>,
}

impl Connected<IncomingStream<'_, UnixListener>> for AgentPeer {
    fn connect_info(stream: IncomingStream<'_, UnixListener>) -> Self {
        AgentPeer { uid: stream.io().peer_cred().ok().map(|c| c.uid()) }
    }
}

/// The account name for a uid (getpwuid_r).
pub fn account_name(uid: u32) -> Option<String> {
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buf = vec![0 as libc::c_char; 4096];
    let mut out: *mut libc::passwd = std::ptr::null_mut();
    let rc = unsafe { libc::getpwuid_r(uid, &mut pwd, buf.as_mut_ptr(), buf.len(), &mut out) };
    if rc != 0 || out.is_null() || pwd.pw_name.is_null() {
        return None;
    }
    Some(unsafe { std::ffi::CStr::from_ptr(pwd.pw_name) }.to_string_lossy().into_owned())
}

/// The role of the one principal the principals door says runs as `account`.
pub fn role_for_account(account: &str, door: &Value) -> Result<String, String> {
    let rows = door.get("data").and_then(Value::as_array)
        .ok_or("agent socket refuses: the principals door answered with no data list")?;
    let hosted: Vec<&Value> = rows.iter()
        .filter(|r| r.get("hostAccount").and_then(Value::as_str) == Some(account))
        .collect();
    match hosted.as_slice() {
        [] => Err(format!("agent socket refuses: no principal runs as account {account}")),
        [one] => one.get("holdsRole").and_then(Value::as_str).filter(|r| !r.is_empty())
            .map(|r| r.trim_start_matches("role-").to_string())
            .ok_or_else(|| format!("agent socket refuses: the principal on account {account} holds no role")),
        many => Err(format!("agent socket refuses: account {account} hosts {} principals, so the OS cannot say which one is calling", many.len())),
    }
}

/// The role for a peer uid, asking the principals door.
pub fn role_for_uid(uid: Option<u32>, door_base: &str) -> Result<String, String> {
    let uid = uid.ok_or("agent socket refuses: the peer's uid could not be read")?;
    let account = account_name(uid).ok_or_else(|| format!("agent socket refuses: uid {uid} has no account"))?;
    let url = format!("{door_base}/v1/identity/principals?limit=500");
    let door: Value = ureq::get(&url).timeout(std::time::Duration::from_millis(2000)).call()
        .map_err(|e| format!("agent socket refuses: the principals door is unreadable ({url}): {e}"))?
        .into_json().map_err(|e| format!("agent socket refuses: the principals door answered non-JSON: {e}"))?;
    role_for_account(&account, &door)
}

fn refuse(reason: &str) -> Response {
    let deny = json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}});
    Json(json!({"stdout": deny.to_string(), "exit_code": 0})).into_response()
}

/// Middleware for the agent socket: the payload's deploy_role is replaced by
/// the role the OS identifies. Any failure refuses the call (fail closed).
pub async fn identify(ConnectInfo(peer): ConnectInfo<AgentPeer>, req: Request, next: Next) -> Response {
    let base = std::env::var("ATHENA_MAKE_URL").unwrap_or_else(|_| "http://localhost:3360".into());
    let role = match tokio::task::spawn_blocking(move || role_for_uid(peer.uid, &base)).await {
        Ok(Ok(role)) => role,
        Ok(Err(why)) => return refuse(&why),
        Err(e) => return refuse(&format!("agent socket refuses: identity lookup failed: {e}")),
    };
    let (parts, body) = req.into_parts();
    let Ok(bytes) = to_bytes(body, 16 * 1024 * 1024).await else { return refuse("agent socket refuses: unreadable request body") };
    let mut payload: Value = match serde_json::from_slice(&bytes) {
        Ok(Value::Object(o)) => Value::Object(o),
        _ => return refuse("agent socket refuses: the request body is not a JSON object"),
    };
    payload["deploy_role"] = json!(role);
    next.run(Request::from_parts(parts, Body::from(payload.to_string()))).await
}
