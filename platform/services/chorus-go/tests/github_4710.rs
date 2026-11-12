//! #4710 AC3 — the lander's GitHub calls, against a stand-in GitHub on loopback that answers by
//! path and keeps every request. The real /usr/bin/curl makes the calls (the lander runs nothing
//! else); no network.
use chorus_go::github::{self, merge_call, pr_from_list};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static ENV: Mutex<()> = Mutex::new(());
const ROUND: &str = "f39d803dd09794e645778328a44acf2b9ca7b3a3";

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gh4710-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn put600(p: &Path, body: &str) {
    std::fs::write(p, body).unwrap();
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600)).unwrap();
}

/// The App directory with a fresh key, or None when this box has no openssl.
fn app_dir(d: &Path) -> Option<PathBuf> {
    let app = d.join("app");
    std::fs::create_dir_all(&app).unwrap();
    let key = app.join("key.pem");
    let ok = std::process::Command::new("/usr/bin/openssl").args(["genrsa", "-out"]).arg(&key).arg("2048")
        .stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
    if !ok { return None; }
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
    put600(&app.join("app-id"), "123\n");
    put600(&app.join("installation-id"), "456\n");
    Some(app)
}

/// A stand-in GitHub: `head` is the open PR's head sha; `merge_codes` answer successive merge
/// calls (the last repeats). Keeps every request as (request line, headers, body).
struct Gh { api: String, seen: std::sync::Arc<Mutex<Vec<(String, String, String)>>> }

fn stand_in_github(head: &str, merge_codes: &[u16]) -> Gh {
    use std::io::{Read, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let api = format!("http://{}", l.local_addr().unwrap());
    let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
    let (seen2, head, codes) = (seen.clone(), head.to_string(), merge_codes.to_vec());
    std::thread::spawn(move || {
        let mut merges = 0usize;
        for mut s in l.incoming().flatten() {
            let mut got = Vec::new(); let mut buf = [0u8; 65536];
            loop {
                let n = s.read(&mut buf).unwrap_or(0); got.extend_from_slice(&buf[..n]);
                let t = String::from_utf8_lossy(&got).to_string();
                if let Some((h, b)) = t.split_once("\r\n\r\n") {
                    let len = h.lines().find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                    if b.len() >= len { break; }
                }
                if n == 0 { break; }
            }
            let t = String::from_utf8_lossy(&got).to_string();
            let (h, b) = t.split_once("\r\n\r\n").unwrap_or((t.as_str(), ""));
            let line = h.lines().next().unwrap_or("").to_string();
            seen2.lock().unwrap().push((line.clone(), h.to_string(), b.to_string()));
            let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
            let (code, body) = if path.ends_with("/access_tokens") { (201, r#"{"token":"ghs_FAKE4710"}"#.to_string()) }
                else if path.contains("/pulls?") { (200, format!(r#"[{{"number":1967,"head":{{"ref":"wren/4953","sha":"{head}","repo":{{"full_name":"gathering-chorus/chorus"}}}}}}]"#)) }
                else if path.contains("/statuses/") { (201, "{}".to_string()) }
                else if path.ends_with("/merge") { let c = codes[merges.min(codes.len() - 1)]; merges += 1; (c, r#"{"message":"x"}"#.to_string()) }
                else { (404, "{}".to_string()) };
            let _ = write!(s, "HTTP/1.1 {code} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    Gh { api, seen }
}

impl Gh {
    fn lines(&self) -> Vec<String> { self.seen.lock().unwrap().iter().map(|(l, _, _)| l.clone()).collect() }
    fn merges(&self) -> usize { self.lines().iter().filter(|l| l.contains("/merge ")).count() }
    fn req(&self, i: usize) -> (String, String, String) { self.seen.lock().unwrap()[i].clone() }
}

fn with_env<T>(app: &Path, gh: &Gh, f: impl FnOnce() -> T) -> T {
    std::env::set_var("WERK_GITHUB_API", &gh.api);
    std::env::set_var("WERK_GITHUB_APP_DIR", app);
    std::env::set_var("LANDER_RETRY_WAIT_SECS", "0");
    let r = f();
    std::env::remove_var("WERK_GITHUB_API");
    std::env::remove_var("WERK_GITHUB_APP_DIR");
    std::env::remove_var("WERK_DEPLOY_CREDENTIAL_FILE");
    r
}

// Pinned to a real main subject: 0a7d00e82 "#4953 (wren) (#1967)" — squash, single parent.
#[test]
fn the_merge_is_a_squash_pinned_to_the_signed_sha_titled_like_main() {
    let c = merge_call(1967, ROUND, 4953, "wren");
    assert_eq!(c.method, "PUT");
    assert_eq!(c.path, "/repos/gathering-chorus/chorus/pulls/1967/merge");
    let b = c.body.unwrap();
    assert_eq!(b["merge_method"], "squash");
    assert_eq!(b["sha"], ROUND);
    assert_eq!(b["commit_title"], "#4953 (wren) (#1967)");
}

// NEGATIVE PROOF: a PR whose head moved after Jeff signed is refused before any merge call.
#[test]
fn a_moved_head_is_refused_and_one_pr_is_required() {
    let pr = |n: u64, r: &str, sha: &str| format!(r#"{{"number":{n},"head":{{"ref":"{r}","sha":"{sha}","repo":{{"full_name":"gathering-chorus/chorus"}}}}}}"#);
    let other = pr(1, "kade/4800", ROUND);
    assert_eq!(pr_from_list(&format!("[{other},{}]", pr(1967, "wren/4953", ROUND)), 4953, ROUND), Ok((1967, "wren".into())));
    assert!(pr_from_list(&format!("[{}]", pr(1967, "wren/4953", "0000000000000000000000000000000000000000")), 4953, ROUND).unwrap_err().contains("head moved"));
    assert!(pr_from_list(&format!("[{other}]"), 4953, ROUND).unwrap_err().contains("no open PR"));
    // a branch that merely ends in the number is not the card's (wren/14953, a/b/4953)
    assert!(pr_from_list(&format!("[{}]", pr(2, "wren/14953", ROUND)), 4953, ROUND).is_err());
    assert!(pr_from_list(&format!("[{}]", pr(2, "a/b/4953", ROUND)), 4953, ROUND).is_err());
    // NEGATIVE PROOF (review of 746df662): a fork's PR with the card's branch name never matches
    let fork = format!(r#"[{{"number":9,"head":{{"ref":"wren/4953","sha":"{ROUND}","repo":{{"full_name":"someone/chorus"}}}}}}]"#);
    assert!(pr_from_list(&fork, 4953, ROUND).unwrap_err().contains("no open PR"));
    let two = format!("[{},{}]", pr(1967, "wren/4953", ROUND), pr(1968, "silas/4953", ROUND));
    assert!(pr_from_list(&two, 4953, ROUND).unwrap_err().contains("more than one"));
}

#[test]
fn a_signed_round_lands_as_the_app_status_then_squash_merge_credential_only_in_headers() {
    let _g = ENV.lock().unwrap_or_else(|p| p.into_inner());
    let d = tmp("land");
    let Some(app) = app_dir(&d) else { eprintln!("unmeasured: no openssl genrsa here"); return; };
    let gh = stand_in_github(ROUND, &[200]);
    assert_eq!(with_env(&app, &gh, || github::land_as_app(4953, ROUND)), Ok(1967));
    let l = gh.lines();
    assert!(l[0].starts_with("POST /app/installations/456/access_tokens"), "{l:?}");
    assert!(l[1].starts_with("GET /repos/gathering-chorus/chorus/pulls?state=open"), "{l:?}");
    assert!(l[2].starts_with(&format!("POST /repos/gathering-chorus/chorus/statuses/{ROUND}")), "{l:?}");
    assert!(l[3].starts_with("PUT /repos/gathering-chorus/chorus/pulls/1967/merge"), "{l:?}");
    let (_, h2, b2) = gh.req(2);
    assert!(b2.contains("\"context\":\"jeff-go\"") && b2.contains("\"state\":\"success\""), "{b2}");
    assert!(h2.contains("ghs_FAKE4710"), "the installation credential did not reach GitHub as a header");
    let (_, _, b3) = gh.req(3);
    assert!(b3.contains("\"merge_method\":\"squash\"") && b3.contains(ROUND), "{b3}");
    assert!(!app.join("request.json").exists(), "a request body was left behind");
    // what ps sees: curl's argv for an API call holds no credential (github::call_argv)
    let argv = github::call_argv(&gh.api, &github::status_call(ROUND, 4953), Some(&app.join("request.json"))).join(" ");
    assert!(!argv.contains("ghs_FAKE4710") && !argv.to_lowercase().contains("authorization"), "{argv}");
}

// NEGATIVE PROOF: when the head moved, no status is posted and no merge is asked for.
#[test]
fn a_moved_head_posts_nothing_and_merges_nothing() {
    let _g = ENV.lock().unwrap_or_else(|p| p.into_inner());
    let d = tmp("moved");
    let Some(app) = app_dir(&d) else { eprintln!("unmeasured: no openssl genrsa here"); return; };
    let gh = stand_in_github("1111111111111111111111111111111111111111", &[200]);
    let e = with_env(&app, &gh, || github::land_as_app(4953, ROUND)).unwrap_err();
    assert!(e.contains("head moved"), "{e}");
    assert!(!gh.lines().iter().any(|l| l.contains("/statuses/") || l.contains("/merge ")), "{:?}", gh.lines());
}

#[test]
fn a_refused_merge_is_an_error_naming_the_pr() {
    let _g = ENV.lock().unwrap_or_else(|p| p.into_inner());
    let d = tmp("refused");
    let Some(app) = app_dir(&d) else { eprintln!("unmeasured: no openssl genrsa here"); return; };
    let gh = stand_in_github(ROUND, &[409]);
    let e = with_env(&app, &gh, || github::land_as_app(4953, ROUND)).unwrap_err();
    assert!(e.contains("PR #1967") && e.contains("409"), "{e}");
}

// #3926 — a stale "not mergeable" (405) is retried once and then merges; a 409 (the head
// moved off the signed sha) is never retried: one merge call, an error.
#[test]
fn a_405_is_retried_once_and_a_409_never() {
    let _g = ENV.lock().unwrap_or_else(|p| p.into_inner());
    let d = tmp("r405");
    let Some(app) = app_dir(&d) else { eprintln!("unmeasured: no openssl genrsa here"); return; };
    let gh = stand_in_github(ROUND, &[405, 200]);
    assert_eq!(with_env(&app, &gh, || github::land_as_app(4953, ROUND)), Ok(1967));
    assert_eq!(gh.merges(), 2, "405 then 200 is two merge calls");
    // NEGATIVE PROOF: 409 is not retried
    let gh = stand_in_github(ROUND, &[409, 200]);
    let e = with_env(&app, &gh, || github::land_as_app(4953, ROUND)).unwrap_err();
    assert!(e.contains("409"), "{e}");
    assert_eq!(gh.merges(), 1, "a 409 was retried");
}

// NEGATIVE PROOF (Silas 17:48; review of 746df662): a merge the lander could not make records
// nothing and leaves the go step open; only a merge that happened is recorded and answered. A
// "no" never merges, and is recorded and answered.
#[test]
fn a_refused_merge_records_nothing_and_leaves_the_step_open() {
    use std::cell::Cell;
    let (recorded, completed) = (Cell::new(false), Cell::new(false));
    let e = github::go_sequence("go", || Err("409".into()), |_| recorded.set(true), || { completed.set(true); Ok(()) }).unwrap_err();
    assert!(e.contains("did not merge") && e.contains("nothing was recorded"), "{e}");
    assert!(!recorded.get() && !completed.get(), "a refused merge was recorded or answered");
    let order = std::cell::RefCell::new(Vec::new());
    assert_eq!(github::go_sequence("go", || { order.borrow_mut().push("merge"); Ok(1967) }, |pr| { assert_eq!(pr, Some(1967)); order.borrow_mut().push("record") },
        || { order.borrow_mut().push("complete"); Ok(()) }), Ok(Some(1967)));
    assert_eq!(*order.borrow(), vec!["merge", "record", "complete"]);
    let merged = Cell::new(false);
    assert_eq!(github::go_sequence("no", || { merged.set(true); Ok(1) }, |pr| assert_eq!(pr, None), || Ok(())), Ok(None));
    assert!(!merged.get(), "a no merged");
}

// review of 113c9fe51 (C) — a request body lives in <HOME>/.chorus/lander-req (0700, this
// account's), under a random name: a file another account pre-creates in /tmp changes nothing.
#[test]
fn negative_proof_a_precreated_tmp_body_file_has_no_effect() {
    let _g = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let home = std::env::temp_dir().join(format!("gh-4710-body-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let old = std::env::var("HOME").ok();
    std::env::set_var("HOME", &home);
    // the old predictable names, pre-created by "another account"
    for n in 0..4u64 {
        let _ = std::fs::write(std::env::temp_dir().join(format!("chorus-lander-req-{}-{n}-{}.json", std::process::id(), chorus_go::now())), "x");
    }
    let a = github::private_body_file(b"{}").unwrap();
    let b = github::private_body_file(b"{}").unwrap();
    assert_ne!(a, b, "every body gets its own random name");
    let dir = home.join(".chorus/lander-req");
    assert_eq!(a.parent().unwrap(), dir);
    assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
    assert_eq!(std::fs::metadata(&a).unwrap().permissions().mode() & 0o777, 0o600);
    // a lander-req that is not this account's own directory (a planted symlink) is refused
    let h2 = home.join("h2");
    std::fs::create_dir_all(h2.join(".chorus")).unwrap();
    std::os::unix::fs::symlink(std::env::temp_dir(), h2.join(".chorus/lander-req")).unwrap();
    std::env::set_var("HOME", &h2);
    assert!(github::private_body_file(b"{}").is_err(), "a symlinked body dir must be refused");
    match old { Some(h) => std::env::set_var("HOME", h), None => std::env::remove_var("HOME") }
}

// review of 113c9fe51 (D) — the credential's first line only: a newline can never add a header.
#[test]
fn negative_proof_a_credential_with_a_newline_sends_one_header_line() {
    let h = github::auth_header("ghs_A\nX-Injected: 1");
    assert_eq!(h.lines().count(), 1, "{h:?}");
    assert!(!h.contains("X-Injected"), "{h:?}");
    assert_eq!(h, "Authorization: token ghs_A\n");
}

// review of 113c9fe51 (E) — a resent go whose PR is not open counts only when GitHub shows the
// card's PR merged at exactly the signed round.
#[test]
fn negative_proof_a_resent_go_with_no_merge_at_the_round_is_refused() {
    let r = "5174a0592261bbd5b452ec1287cda4e5e2fda45a";
    let pr = |rf: &str, repo: &str, sha: &str, merged: serde_json::Value| serde_json::json!(
        {"number": 7, "head": {"ref": rf, "sha": sha, "repo": {"full_name": repo}}, "merged_at": merged});
    let ok = serde_json::json!([pr("wren/4710", github::REPO, r, "2026-11-11T20:00:00Z".into())]).to_string();
    assert_eq!(github::merged_pr_from_list(&ok, 4710, r).unwrap(), 7);
    for (why, body) in [
        ("closed unmerged", serde_json::json!([pr("wren/4710", github::REPO, r, serde_json::Value::Null)])),
        ("another card", serde_json::json!([pr("wren/4711", github::REPO, r, "2026-11-11T20:00:00Z".into())])),
        ("a fork", serde_json::json!([pr("wren/4710", "someone/chorus", r, "2026-11-11T20:00:00Z".into())])),
        ("another head", serde_json::json!([pr("wren/4710", github::REPO, "0000000000000000000000000000000000000000", "2026-11-11T20:00:00Z".into())])),
        ("none", serde_json::json!([])),
    ] {
        assert!(github::merged_pr_from_list(&body.to_string(), 4710, r).is_err(), "{why} must be refused");
    }
}
