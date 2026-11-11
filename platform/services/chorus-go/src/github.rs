//! #4710 AC2/AC3 — the lander's two GitHub calls, as the App (Jeff's yes 17:45 via Silas).
//!
//! The lander (this program, run by the chorus-lander account) lands a card only on a go Jeff's
//! passkey signed for that round. After it verifies the signature it:
//!   1 posts the `jeff-go` commit status on the signed round sha
//!   2 squash-merges the card's PR through the API with `sha` = that round sha, so GitHub itself
//!     refuses the merge if the PR head moved after Jeff signed
//! Merge method and title match today's main (squash, single parent, `#<card> (<role>)`; the
//! werk-merge 06-02 decision). No clone, no werk, no local main.
//!
//! The credential is the App's 1-hour installation credential (shared/app_credential.rs). It goes
//! to curl inside a header read from stdin (`-H @-`), never argv; a body goes on stdin too.
use crate::R;
use serde_json::{json, Value};
use std::io::Write;
use std::process::{Command, Stdio};

include!("../../shared/deploy_credential.rs");
include!("../../shared/app_credential.rs");

pub const REPO: &str = "gathering-chorus/chorus";
pub const STATUS_CONTEXT: &str = "jeff-go";

/// One GitHub API call: method, path under the API root, JSON body (or none).
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub method: &'static str,
    pub path: String,
    pub body: Option<Value>,
}

/// The card's open PR, by its head branch `<role>/<card>`.
pub fn find_pr_call(role: &str, card: u64) -> Call {
    let owner = REPO.split('/').next().unwrap_or("");
    Call { method: "GET", path: format!("/repos/{REPO}/pulls?state=open&head={owner}:{role}/{card}"), body: None }
}

/// The `jeff-go` status on the signed round sha.
pub fn status_call(round: &str, card: u64) -> Call {
    Call {
        method: "POST",
        path: format!("/repos/{REPO}/statuses/{round}"),
        body: Some(json!({"state": "success", "context": STATUS_CONTEXT,
                          "description": format!("#{card}: Jeff's passkey signed this round")})),
    }
}

/// The squash merge, pinned to the signed sha.
pub fn merge_call(pr: u64, round: &str, card: u64, role: &str) -> Call {
    Call {
        method: "PUT",
        path: format!("/repos/{REPO}/pulls/{pr}/merge"),
        body: Some(json!({"merge_method": "squash", "sha": round, "commit_title": format!("#{card} ({role}) (#{pr})")})),
    }
}

/// The PR number from the pulls list, only if exactly one open PR has that head and its head sha
/// is the signed round (a moved head is refused here before GitHub refuses it).
pub fn pr_from_list(body: &str, round: &str) -> R<u64> {
    let v: Value = serde_json::from_str(body).map_err(|_| "GitHub's PR list is not JSON".to_string())?;
    let prs = v.as_array().ok_or("GitHub's PR list is not a list")?;
    match prs.as_slice() {
        [one] => {
            let head = one.pointer("/head/sha").and_then(Value::as_str).unwrap_or("");
            if head != round {
                return Err(format!("the PR head is {} but Jeff signed {}: the head moved, no merge", crate::short(head), crate::short(round)));
            }
            one.get("number").and_then(Value::as_u64).ok_or_else(|| "the PR has no number".into())
        }
        [] => Err("no open PR for this card".into()),
        _ => Err("more than one open PR for this card's branch".into()),
    }
}

/// Run one call with the armed installation credential. Returns (http code, body).
pub fn run(call: &Call) -> R<(u16, String)> {
    let file = armed_credential().ok_or("auth-fail: no App credential armed")?;
    let tok = std::fs::read_to_string(&file).map_err(|e| format!("auth-fail: {}: {}", std::path::Path::new(&file).display(), e))?;
    let api = std::env::var(GITHUB_API_ENV).unwrap_or_else(|_| "https://api.github.com".into());
    let stdin = format!("Authorization: {} {}\n", "token", credential_line(&tok));
    let mut c = Command::new("curl");
    c.args(["-sS", "--max-time", "30", "-o", "-", "-w", "\n%{http_code}", "-X", call.method, "-H", "@-",
            "-H", "Accept: application/vnd.github+json", "-H", "X-GitHub-Api-Version: 2022-11-28"]);
    if let Some(b) = &call.body {
        // curl takes one stdin; the body goes in a 0600 file next to the credential, removed after
        let bf = std::path::Path::new(&file).with_file_name("request.json");
        std::fs::write(&bf, b.to_string()).map_err(|e| e.to_string())?;
        c.args(["-H", "Content-Type: application/json", "--data-binary"]).arg(format!("@{}", bf.display()));
    }
    c.arg(format!("{api}{}", call.path)).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = c.spawn().map_err(|e| format!("curl did not start: {e}"))?;
    child.stdin.take().ok_or("curl stdin")?.write_all(stdin.as_bytes()).map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(std::path::Path::new(&file).with_file_name("request.json"));
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (body, code) = text.rsplit_once('\n').unwrap_or(("", &text));
    Ok((code.trim().parse().unwrap_or(0), body.to_string()))
}

/// The whole land half the lander owns: arm the App, find the PR at the signed round, post
/// `jeff-go`, squash-merge pinned to that sha. Call only after the passkey go verified.
pub fn land_as_app(card: u64, role: &str, round: &str) -> R<u64> {
    arm_app_credential()?;
    let (code, body) = run(&find_pr_call(role, card))?;
    if code != 200 { return Err(format!("GitHub answered {code} to the PR lookup for #{card}")); }
    let pr = pr_from_list(&body, round)?;
    let (code, _) = run(&status_call(round, card))?;
    if code != 201 { return Err(format!("GitHub answered {code} to the jeff-go status on {}", crate::short(round))); }
    let (code, body) = run(&merge_call(pr, round, card, role))?;
    if code != 200 {
        let why = serde_json::from_str::<Value>(&body).ok().and_then(|v| v.get("message").and_then(Value::as_str).map(String::from)).unwrap_or_default();
        return Err(format!("GitHub refused the merge of PR #{pr} ({code}): {why}"));
    }
    Ok(pr)
}
