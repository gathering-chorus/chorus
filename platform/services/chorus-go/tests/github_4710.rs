//! #4710 AC3 — the lander's GitHub calls, against a stand-in GitHub (a curl on PATH that
//! answers from canned replies and records what it was given). No network.
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
    let ok = std::process::Command::new("openssl").args(["genrsa", "-out"]).arg(&key).arg("2048")
        .stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
    if !ok { return None; }
    std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
    put600(&app.join("app-id"), "123\n");
    put600(&app.join("installation-id"), "456\n");
    Some(app)
}

/// A stand-in curl: logs argv, stdin and any --data-binary file per call; answers by URL.
/// `head` is the open PR's head sha GitHub reports; `merge_code` what the merge answers.
fn stand_in_github(d: &Path, head: &str, merge_code: u16) -> PathBuf {
    let bin = d.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = d.join("log");
    let script = format!(r#"#!/bin/sh
n=$(ls {log} 2>/dev/null | wc -l | tr -d ' '); c={log}/$n; mkdir -p {log}; mkdir -p $c
printf '%s\n' "$@" > $c/argv; cat > $c/stdin
for a in "$@"; do case "$a" in @*) cp "${{a#@}}" $c/body 2>/dev/null;; esac; done
url=""; for a in "$@"; do url="$a"; done
case "$url" in
  */access_tokens) printf '{{"token":"ghs_FAKE4710"}}';;
  */pulls\?*) printf '[{{"number":1967,"head":{{"ref":"wren/4953","sha":"{head}"}}}}]\n200';;
  */statuses/*) printf '{{}}\n201';;
  */merge) printf '{{"message":"x"}}\n{merge_code}';;
esac
"#, log = log.display());
    let f = bin.join("curl");
    std::fs::write(&f, script).unwrap();
    std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

/// The calls the stand-in saw, in order: (argv, stdin, body).
fn calls(d: &Path) -> Vec<(String, String, String)> {
    let mut v = Vec::new();
    for i in 0.. {
        let c = d.join("log").join(i.to_string());
        if !c.exists() { break; }
        let r = |n: &str| std::fs::read_to_string(c.join(n)).unwrap_or_default();
        v.push((r("argv"), r("stdin"), r("body")));
    }
    v
}

fn with_env<T>(app: &Path, bin: &Path, f: impl FnOnce() -> T) -> T {
    let old = std::env::var_os("PATH").unwrap_or_default();
    let mut p = std::ffi::OsString::from(bin.as_os_str()); p.push(":"); p.push(&old);
    std::env::set_var("PATH", &p);
    std::env::set_var("WERK_GITHUB_APP_DIR", app);
    let r = f();
    std::env::set_var("PATH", old);
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
    let pr = |n: u64, r: &str, sha: &str| format!(r#"{{"number":{n},"head":{{"ref":"{r}","sha":"{sha}"}}}}"#);
    let other = pr(1, "kade/4800", ROUND);
    assert_eq!(pr_from_list(&format!("[{other},{}]", pr(1967, "wren/4953", ROUND)), 4953, ROUND), Ok((1967, "wren".into())));
    assert!(pr_from_list(&format!("[{}]", pr(1967, "wren/4953", "0000000000000000000000000000000000000000")), 4953, ROUND).unwrap_err().contains("head moved"));
    assert!(pr_from_list(&format!("[{other}]"), 4953, ROUND).unwrap_err().contains("no open PR"));
    // a branch that merely ends in the number is not the card's (wren/14953, a/b/4953)
    assert!(pr_from_list(&format!("[{}]", pr(2, "wren/14953", ROUND)), 4953, ROUND).is_err());
    assert!(pr_from_list(&format!("[{}]", pr(2, "a/b/4953", ROUND)), 4953, ROUND).is_err());
    let two = format!("[{},{}]", pr(1967, "wren/4953", ROUND), pr(1968, "silas/4953", ROUND));
    assert!(pr_from_list(&two, 4953, ROUND).unwrap_err().contains("more than one"));
}

#[test]
fn a_signed_round_lands_as_the_app_status_then_squash_merge_no_credential_on_argv() {
    let _g = ENV.lock().unwrap_or_else(|p| p.into_inner());
    let d = tmp("land");
    let Some(app) = app_dir(&d) else { eprintln!("unmeasured: no openssl genrsa here"); return; };
    let bin = stand_in_github(&d, ROUND, 200);
    let r = with_env(&app, &bin, || github::land_as_app(4953, ROUND));
    assert_eq!(r, Ok(1967));
    let seen = calls(&d);
    let urls: Vec<&str> = seen.iter().map(|(a, _, _)| a.lines().last().unwrap_or("")).collect();
    assert!(urls[0].ends_with("/app/installations/456/access_tokens"), "{urls:?}");
    assert!(urls[1].contains("/pulls?state=open"), "{urls:?}");
    assert!(urls[2].ends_with(&format!("/statuses/{ROUND}")), "{urls:?}");
    assert!(urls[3].ends_with("/pulls/1967/merge"), "{urls:?}");
    assert!(seen[2].2.contains("\"context\":\"jeff-go\"") && seen[2].2.contains("\"state\":\"success\""), "{}", seen[2].2);
    assert!(seen[3].2.contains("\"merge_method\":\"squash\"") && seen[3].2.contains(ROUND), "{}", seen[3].2);
    for (argv, stdin, _) in &seen[1..] {
        assert!(!argv.contains("ghs_FAKE4710") && !argv.contains("Authorization"), "the credential reached argv: {argv}");
        assert!(stdin.contains("ghs_FAKE4710"), "the credential did not go on stdin: {stdin}");
    }
    assert!(!app.join("request.json").exists(), "a request body was left behind");
}

// NEGATIVE PROOF: when the head moved, no status is posted and no merge is asked for.
#[test]
fn a_moved_head_posts_nothing_and_merges_nothing() {
    let _g = ENV.lock().unwrap_or_else(|p| p.into_inner());
    let d = tmp("moved");
    let Some(app) = app_dir(&d) else { eprintln!("unmeasured: no openssl genrsa here"); return; };
    let bin = stand_in_github(&d, "1111111111111111111111111111111111111111", 200);
    let e = with_env(&app, &bin, || github::land_as_app(4953, ROUND)).unwrap_err();
    assert!(e.contains("head moved"), "{e}");
    let urls: Vec<String> = calls(&d).iter().map(|(a, _, _)| a.lines().last().unwrap_or("").to_string()).collect();
    assert!(!urls.iter().any(|u| u.contains("/statuses/") || u.ends_with("/merge")), "{urls:?}");
}

#[test]
fn a_refused_merge_is_an_error_naming_the_pr() {
    let _g = ENV.lock().unwrap_or_else(|p| p.into_inner());
    let d = tmp("refused");
    let Some(app) = app_dir(&d) else { eprintln!("unmeasured: no openssl genrsa here"); return; };
    let bin = stand_in_github(&d, ROUND, 409);
    let e = with_env(&app, &bin, || github::land_as_app(4953, ROUND)).unwrap_err();
    assert!(e.contains("PR #1967") && e.contains("409"), "{e}");
}

// NEGATIVE PROOF (Silas 17:48): a merge the lander could not make leaves the run's go step
// open; only a merge that happened answers it. A "no" never merges and still answers.
#[test]
fn a_refused_merge_leaves_the_go_step_open_and_a_no_never_merges() {
    use std::cell::Cell;
    let completed = Cell::new(false);
    let e = github::go_then_complete("go", || Err("409".into()), || { completed.set(true); Ok(()) }).unwrap_err();
    assert!(e.contains("did not merge") && e.contains("stays open"), "{e}");
    assert!(!completed.get(), "the go step was answered after a refused merge");
    let merged = Cell::new(false);
    assert_eq!(github::go_then_complete("go", || { merged.set(true); Ok(1967) }, || Ok(())), Ok(Some(1967)));
    assert!(merged.get());
    let merged = Cell::new(false);
    assert_eq!(github::go_then_complete("no", || { merged.set(true); Ok(1) }, || Ok(())), Ok(None));
    assert!(!merged.get(), "a no merged");
}
