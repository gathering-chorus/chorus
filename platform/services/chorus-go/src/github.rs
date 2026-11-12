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

/// The open PRs (a card's branch is `<role>/<card>`; the go names the card, not the role).
pub fn find_pr_call() -> Call {
    Call { method: "GET", path: format!("/repos/{REPO}/pulls?state=open&per_page=100"), body: None }
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

/// The card's PR from the open list: exactly one whose head branch is `<role>/<card>`, and whose
/// head sha is the signed round (a moved head is refused here before GitHub refuses it).
/// Returns (PR number, role).
pub fn pr_from_list(body: &str, card: u64, round: &str) -> R<(u64, String)> {
    let v: Value = serde_json::from_str(body).map_err(|_| "GitHub's PR list is not JSON".to_string())?;
    let prs = v.as_array().ok_or("GitHub's PR list is not a list")?;
    let suffix = format!("/{card}");
    let mine: Vec<&Value> = prs.iter().filter(|p| {
        let r = p.pointer("/head/ref").and_then(Value::as_str).unwrap_or("");
        // review of 746df662: only a branch in this repo — a fork's PR named <x>/<card> never matches
        let repo = p.pointer("/head/repo/full_name").and_then(Value::as_str).unwrap_or("");
        r.ends_with(&suffix) && r.matches('/').count() == 1 && repo == REPO
    }).collect();
    match mine.as_slice() {
        [one] => {
            let head = one.pointer("/head/sha").and_then(Value::as_str).unwrap_or("");
            if head != round {
                return Err(format!("the PR head is {} but Jeff signed {}: the head moved, no merge", crate::short(head), crate::short(round)));
            }
            let role = one.pointer("/head/ref").and_then(Value::as_str).unwrap_or("").split('/').next().unwrap_or("").to_string();
            let n = one.get("number").and_then(Value::as_u64).ok_or("the PR has no number")?;
            Ok((n, role))
        }
        [] => Err(format!("no open PR for #{card}")),
        _ => Err(format!("more than one open PR for #{card}")),
    }
}

/// curl's argv for one API call: what `ps` shows. Holds no credential — the Authorization
/// header is read from stdin (`-H @-`) and a body from a 0600 file (`body`).
pub fn call_argv(api: &str, call: &Call, body: Option<&std::path::Path>) -> Vec<String> {
    let mut a: Vec<String> = ["-sS", "--max-time", "30", "-o", "-", "-w", "\n%{http_code}", "-X", call.method, "-H", "@-",
        "-H", "Accept: application/vnd.github+json", "-H", "X-GitHub-Api-Version: 2022-11-28"]
        .iter().map(|s| s.to_string()).collect();
    if let (Some(_), Some(f)) = (&call.body, body) {
        a.extend(["-H".into(), "Content-Type: application/json".into(), "--data-binary".into(), format!("@{}", f.display())]);
    }
    a.push(format!("{api}{}", call.path));
    a
}

/// Run one call with the installation credential `tok` (in memory). The body goes in a 0600 file
/// of its own, named per call, removed after: the lander serves requests on parallel threads.
pub fn run(call: &Call, tok: &str) -> R<(u16, String)> {
    use std::os::unix::fs::OpenOptionsExt;
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let api = std::env::var(GITHUB_API_ENV).unwrap_or_else(|_| "https://api.github.com".into());
    let bf = std::env::temp_dir().join(format!("chorus-lander-req-{}-{}-{}.json", std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed), crate::now()));
    if let Some(b) = &call.body {
        std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&bf)
            .and_then(|mut f| f.write_all(b.to_string().as_bytes())).map_err(|e| e.to_string())?;
    }
    let child = Command::new("/usr/bin/curl").args(call_argv(&api, call, Some(&bf)))
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn();
    let out = child.and_then(|mut c| {
        if let Some(mut i) = c.stdin.take() { let _ = i.write_all(format!("Authorization: {} {}\n", "token", tok.trim()).as_bytes()); }
        c.wait_with_output()
    });
    let _ = std::fs::remove_file(&bf);
    let out = out.map_err(|e| format!("curl: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    let (body, code) = text.rsplit_once('\n').unwrap_or(("", &text));
    Ok((code.trim().parse().unwrap_or(0), body.to_string()))
}

/// #4710 — the go's order (Silas 17:48; review of round 746df662): Jeff's signed go merges first;
/// only a merge that happened is recorded and answers the run's go step, so a run never lands
/// unmerged and a refused merge leaves nothing recorded (Jeff can sign again). A "no" merges
/// nothing and is recorded and answered.
pub fn go_sequence(verb: &str, merge: impl FnOnce() -> R<u64>, record: impl FnOnce(), complete: impl FnOnce() -> R<()>) -> R<Option<u64>> {
    let pr = if verb == "go" {
        Some(merge().map_err(|e| format!("signed, but the lander did not merge: {e}; nothing was recorded and the go step stays open — sign again once it is fixed"))?)
    } else {
        None
    };
    record();
    complete()?;
    Ok(pr)
}

/// How long before the one 405 retry (tests set LANDER_RETRY_WAIT_SECS=0). Log evidence
/// (werk-merge.jsonl, 11-11): 1,464 merges, 29 refused merge-conflict — real conflicts are
/// common, so one retry, never a loop.
fn retry_wait_secs() -> u64 {
    std::env::var("LANDER_RETRY_WAIT_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(5)
}

/// The whole land half the lander owns: arm the App, find the PR at the signed round, post
/// `jeff-go`, squash-merge pinned to that sha. Call only after the passkey go verified.
pub fn land_as_app(card: u64, round: &str) -> R<u64> {
    let tok = app_credential()?;
    let (code, body) = run(&find_pr_call(), &tok)?;
    if code != 200 { return Err(format!("GitHub answered {code} to the PR lookup for #{card}")); }
    let (pr, role) = pr_from_list(&body, card, round)?;
    let role = role.as_str();
    let (code, _) = run(&status_call(round, card), &tok)?;
    if code != 201 { return Err(format!("GitHub answered {code} to the jeff-go status on {}", crate::short(round))); }
    let mut answer = run(&merge_call(pr, round, card, role), &tok)?;
    // #3926 — GitHub's mergeable flag can be stale right after a push: 405 "not mergeable".
    // Retry that once. Never a 409: that is the PR head moving off the signed sha (Silas 18:01).
    if answer.0 == 405 {
        std::thread::sleep(std::time::Duration::from_secs(retry_wait_secs()));
        answer = run(&merge_call(pr, round, card, role), &tok)?;
    }
    let (code, body) = answer;
    if code != 200 {
        let why = serde_json::from_str::<Value>(&body).ok().and_then(|v| v.get("message").and_then(Value::as_str).map(String::from)).unwrap_or_default();
        return Err(format!("GitHub refused the merge of PR #{pr} ({code}): {why}"));
    }
    Ok(pr)
}
