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

/// #4786 AC4: payload role ≠ peer role → refuse.
/// Returns Ok(peer_role_clean) if valid, or Err(refusal_reason) if payload claims a different role.
pub fn verify_peer_role(peer_role: &str, payload: &Value) -> Result<String, String> {
    let peer_clean = peer_role.trim().trim_start_matches("chorus-");
    for key in ["deploy_role", "role"] {
        if let Some(claimed) = payload.get(key).and_then(Value::as_str) {
            let claimed_clean = claimed.trim().trim_start_matches("chorus-");
            if !claimed_clean.is_empty() && claimed_clean != peer_clean {
                return Err(format!(
                    "agent socket refuses: payload claims role '{claimed}' but socket peer is '{peer_role}'"
                ));
            }
        }
    }
    Ok(peer_clean.to_string())
}

fn refuse(path: &str, reason: &str) -> Response {
    if path == "/bouncer-pending" || path == "/launchagent-ops" {
        return (
            axum::http::StatusCode::FORBIDDEN,
            Json(json!({"error": reason})),
        )
            .into_response();
    }
    let deny = json!({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":reason}});
    Json(json!({"stdout": deny.to_string(), "exit_code": 0})).into_response()
}

/// Middleware for the agent socket: the payload's deploy_role is replaced by
/// the role the OS identifies. Any failure refuses the call (fail closed).
pub async fn identify(ConnectInfo(peer): ConnectInfo<AgentPeer>, req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let base = std::env::var("ATHENA_MAKE_URL").unwrap_or_else(|_| "http://localhost:3360".into());
    let role = match tokio::task::spawn_blocking(move || role_for_uid(peer.uid, &base)).await {
        Ok(Ok(role)) => role,
        Ok(Err(why)) => return refuse(&path, &why),
        Err(e) => return refuse(&path, &format!("agent socket refuses: identity lookup failed: {e}")),
    };
    let (parts, body) = req.into_parts();
    let Ok(bytes) = to_bytes(body, 16 * 1024 * 1024).await else {
        return refuse(&path, "agent socket refuses: unreadable request body");
    };
    let mut payload: Value = match serde_json::from_slice(&bytes) {
        Ok(Value::Object(o)) => Value::Object(o),
        _ => return refuse(&path, "agent socket refuses: the request body is not a JSON object"),
    };

    // #4786 AC4 / #4787 AC2: on /bouncer-pending and /launchagent-ops, verify that payload role matches the peer role.
    // For standard hooks (e.g. /pre-tool-use), the agent socket overwrites the payload
    // role with the OS peer role instead of denying.
    if path == "/bouncer-pending" || path == "/launchagent-ops" {
        if let Err(why) = verify_peer_role(&role, &payload) {
            return refuse(&path, &why);
        }
    }

    let role_clean = role.trim().trim_start_matches("chorus-");
    payload["deploy_role"] = json!(role_clean);
    payload["role"] = json!(role_clean);
    next.run(Request::from_parts(parts, Body::from(payload.to_string()))).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_peer_role_allows_matching_role() {
        let payload = json!({"role": "kade", "deploy_role": "kade"});
        assert_eq!(verify_peer_role("kade", &payload), Ok("kade".to_string()));

        let payload_prefixed = json!({"role": "chorus-kade", "deploy_role": "chorus-kade"});
        assert_eq!(verify_peer_role("kade", &payload_prefixed), Ok("kade".to_string()));

        let payload_empty = json!({});
        assert_eq!(verify_peer_role("kade", &payload_empty), Ok("kade".to_string()));
    }

    #[test]
    fn negative_proof_peer_role_refuses_mismatched_deploy_role() {
        // #4786 AC4 / #3734 negative proof:
        // kade's socket sends deploy_role=silas -> refused
        let payload = json!({"deploy_role": "silas"});
        let err = verify_peer_role("kade", &payload).unwrap_err();
        assert!(
            err.contains("payload claims role 'silas' but socket peer is 'kade'"),
            "got {:?}",
            err
        );

        let payload_role = json!({"role": "silas"});
        let err2 = verify_peer_role("kade", &payload_role).unwrap_err();
        assert!(
            err2.contains("payload claims role 'silas' but socket peer is 'kade'"),
            "got {:?}",
            err2
        );

        // #4787 AC2 negative proof: payload role spoofed for launchagent-ops refused
        let payload_launchagent = json!({"verb": "kickstart", "label": "com.chorus.clearing", "role": "silas"});
        let err3 = verify_peer_role("kade", &payload_launchagent).unwrap_err();
        assert!(
            err3.contains("payload claims role 'silas' but socket peer is 'kade'"),
            "got {:?}",
            err3
        );
    }
}
