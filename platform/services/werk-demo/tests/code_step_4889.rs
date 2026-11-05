// @test-type: integration — a stand-in claude, a real git werk; no model, no network.
// @domain: proving
// @card: #4889
//! AC4 + AC7: the code step runs Claude Code's existing /code-review (uncapped) and
//! /security-review in the card's werk over its whole branch, plus /gate-code for the
//! AC checks; any finding from any of them fails the step. One env-mutating test fn.
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use werk_demo::run_gate_step;

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("wd4889-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}
fn exec(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").arg("-C").arg(dir).args(args)
        .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t").output().unwrap();
    assert!(o.status.success(), "git {:?}", args);
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

#[test]
fn the_code_step_runs_code_review_and_security_review_and_fails_on_their_findings() {
    let home = tmp("home");
    let base = tmp("werks");
    let werk = base.join("silas-4889");
    for d in ["platform/scripts", "skills/gate-code", "ops/logs"] { fs::create_dir_all(home.join(d)).unwrap(); }
    fs::write(home.join("skills/gate-code/SKILL.md"), "---\nname: gate-code\nfail-items: 1-2\n---\n# gate-code\n").unwrap();
    exec(&home.join("platform/scripts/cards"), "#!/bin/sh\necho '#4889 card'\n");
    exec(&home.join("platform/scripts/chorus-log"), "#!/bin/sh\nexit 0\n");
    fs::create_dir_all(&werk).unwrap();
    git(&werk, &["init", "-q"]);
    fs::write(werk.join("a.rs"), "fn a() {}\n").unwrap();
    git(&werk, &["add", "a.rs"]);
    git(&werk, &["commit", "-q", "-m", "base"]);
    let main = git(&werk, &["rev-parse", "HEAD"]);
    git(&werk, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    fs::write(werk.join("a.rs"), "fn a() { b() }\n").unwrap();
    git(&werk, &["commit", "-q", "-am", "card"]);

    // stand-in claude: logs cwd + argv; /code-review finds a bug unless ./clean exists;
    // /security-review gives no list when ./garbled exists; gate-code finds nothing
    let log = home.join("calls");
    let claude = home.join("claude");
    exec(&claude, &format!(r#"#!/bin/sh
cat >/dev/null
echo "$(pwd) | $*" >> "{log}"
case "$*" in
  *"/code-review"*) if [ -f "{h}/clean" ]; then printf '```json\n[]\n```\n'; else printf 'found 1\n```json\n[{{"file":"a.rs","line":1,"summary":"b is undefined"}}]\n```\n'; fi ;;
  *"/security-review"*) if [ -f "{h}/garbled" ]; then echo 'all good'; else printf '```json\n[]\n```\n'; fi ;;
  *) echo '{{"findings":[]}}' ;;
esac
"#, log = log.display(), h = home.display()));
    std::env::set_var("CHORUS_CLAUDE_BIN", &claude);
    std::env::remove_var("CHORUS_GATE_PROFILE");
    std::env::set_var("CHORUS_WERK_BASE", &base);
    std::env::set_var("CHORUS_MEM_FLOOR_DISABLE", "1");
    std::env::set_var("CHORUS_TRACE_ID", "t4889");
    std::env::set_var("CHORUS_HOOK_SHIM", "/nonexistent/chorus-hook-shim-4889");

    // a /code-review finding fails the step, named with file and line
    std::env::set_var("CHORUS_DEMO_ROUND", "r1");
    let red = run_gate_step(4889, "silas", &home, "code").unwrap();
    assert!(red.message.contains(": fail") && red.message.contains("/code-review: a.rs:1 — b is undefined"), "{}", red.message);
    let calls = fs::read_to_string(&log).unwrap();
    let w = werk.canonicalize().unwrap();
    assert!(calls.lines().any(|l| l.starts_with(&format!("{} |", w.display())) && l.contains(&format!("/code-review medium --max-findings all {main}...HEAD"))),
            "/code-review runs uncapped in the werk on the whole branch:\n{calls}");
    assert!(calls.lines().any(|l| l.contains(&format!("/security-review {main}...HEAD"))), "{calls}");

    // all three clean → pass
    fs::write(home.join("clean"), "").unwrap();
    std::env::set_var("CHORUS_DEMO_ROUND", "r2");
    let ok = run_gate_step(4889, "silas", &home, "code").unwrap();
    assert!(ok.message.contains(": pass"), "{}", ok.message);

    // NEGATIVE PROOF: a review that gives no findings list is an error, never a pass
    fs::write(home.join("garbled"), "").unwrap();
    // a new commit, so the recorded pass is for other content (a pass is patch-keyed)
    fs::write(werk.join("a.rs"), "fn a() { c() }\n").unwrap();
    git(&werk, &["commit", "-q", "-am", "card 2"]);
    std::env::set_var("CHORUS_DEMO_ROUND", "r3");
    let bad = run_gate_step(4889, "silas", &home, "code").unwrap();
    assert_eq!(bad.exit, 1, "{}", bad.message);
    assert!(bad.message.contains("/security-review: no findings list"), "{}", bad.message);
}
