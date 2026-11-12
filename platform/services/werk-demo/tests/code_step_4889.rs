// @test-type: integration — a stand-in claude, a real git werk; no model, no network.
// @domain: cicd
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

/// a stand-in properties door: every request answers `body`
fn door(body: &'static str) -> String {
    use std::io::{Read, Write};
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || for mut c in l.incoming().flatten() {
        let mut b = [0u8; 4096];
        let _ = c.read(&mut b);
        let _ = write!(c, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
    });
    url
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
    // #4959 — every review runs its gate's policies; none carry a check in this fixture
    exec(&home.join("platform/scripts/policy-validate"), "#!/bin/sh\necho 'policy-validate: 0 checks, 0 above baseline, 0 could not run'\n");
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
    // the security-review skill gives no list when ./garbled exists; gate-code finds nothing
    let log = home.join("calls");
    let claude = home.join("claude");
    exec(&claude, &format!(r#"#!/bin/sh
cat >/dev/null
echo "$(pwd) | $*" >> "{log}"
case "$*" in
  *"/code-review"*) if [ -f "{h}/clean" ]; then printf '```json\n[]\n```\n'; else printf 'found 1\n```json\n[{{"file":"a.rs","line":1,"summary":"b is undefined"}}]\n```\n'; fi ;;
  *"security-review skill"*) if [ -f "{h}/garbled" ]; then echo 'all good'; else printf '```json\n[]\n```\n'; fi ;;
  *"Jeff's rule"*) if [ -f "{h}/breaksnothing" ]; then printf '```json\n[false]\n```\n'; elif [ -f "{h}/breaks" ]; then printf '```json\n[true]\n```\n'; else echo 'no answer'; fi ;;
  *) if [ -f "{h}/gatefinding" ]; then echo '{{"findings":[{{"item":7,"where":"a.rs:1","what":"too complex"}}]}}'; else echo '{{"findings":[]}}'; fi ;;
esac
"#, log = log.display(), h = home.display()));
    std::env::set_var("CHORUS_CLAUDE_BIN", &claude);
    // #4921 — the gate runs claude as the card's role account; the stand-in
    // runs here, so the test is that account (no sudo hop)
    std::env::set_var("USER", "chorus-silas");
    // no properties door until the breaks rounds below: never the live one
    std::env::set_var("OWL_URL", "http://127.0.0.1:9");
    std::env::remove_var("CHORUS_GATE_PROFILE");
    std::env::set_var("CHORUS_WERK_BASE", &base);
    std::env::set_var("CHORUS_MEM_FLOOR_DISABLE", "1");
    std::env::set_var("CHORUS_TRACE_ID", "t4889");
    std::env::set_var("CHORUS_HOOK_SHIM", "/nonexistent/chorus-hook-shim-4889");

    // a /code-review finding fails the step, named with file and line
    std::env::set_var("CHORUS_DEMO_ROUND", "r1");
    let red = run_gate_step(4889, "silas", &home, "code").unwrap();
    assert!(red.message.contains(": fail") && red.message.contains("/code-review: a.rs:1 — b is undefined"), "{}", red.message);
    // B7 (Wren 16:32): a red review fails its step, so nothing after it runs
    assert_eq!(red.exit, 1, "{}", red.message);
    let calls = fs::read_to_string(&log).unwrap();
    let w = werk.canonicalize().unwrap();
    assert!(calls.lines().any(|l| l.starts_with(&format!("{} |", w.display())) && l.contains(&format!("/code-review medium --max-findings all {main}...HEAD"))),
            "/code-review runs uncapped in the werk on the whole branch:\n{calls}");
    // /security-review runs as its skill through the Skill tool (its slash command is a zero-turn local command under -p)
    assert!(calls.lines().any(|l| l.contains(&format!("Invoke the security-review skill on the changes {main}...HEAD")) && l.contains("--allowedTools Skill,")), "{calls}");

    // #4889 (Jeff 11-05 10:06) — a round with no new commit reads nothing new: r1's finding
    // carries forward, red, even though the reviewer would now say clean (never dropped)
    fs::write(home.join("clean"), "").unwrap();
    std::env::set_var("CHORUS_DEMO_ROUND", "r1b");
    let still = run_gate_step(4889, "silas", &home, "code").unwrap();
    assert!(still.message.contains(": fail") && still.message.contains("carried (not yet fixed): a.rs:1 — b is undefined"), "{}", still.message);

    // a fix commit touches a.rs and nothing raises it again → fixed; all three clean → pass
    fs::write(werk.join("a.rs"), "fn a() { b(); }\nfn b() {}\n").unwrap();
    git(&werk, &["commit", "-q", "-am", "fix"]);
    std::env::set_var("CHORUS_DEMO_ROUND", "r2");
    let ok = run_gate_step(4889, "silas", &home, "code").unwrap();
    assert!(ok.message.contains(": pass"), "{}", ok.message);
    assert_eq!(ok.exit, 0, "NEGATIVE PROOF: a green review lets the run go on");

    // NEGATIVE PROOF: a review that gives no findings list is an error, never a pass
    fs::write(home.join("garbled"), "").unwrap();
    // a new commit, so the recorded pass is for other content (a pass is patch-keyed)
    fs::write(werk.join("a.rs"), "fn a() { c() }\n").unwrap();
    git(&werk, &["commit", "-q", "-am", "card 2"]);
    std::env::set_var("CHORUS_DEMO_ROUND", "r3");
    let bad = run_gate_step(4889, "silas", &home, "code").unwrap();
    assert_eq!(bad.exit, 1, "{}", bad.message);
    assert!(bad.message.contains("/security-review: no findings list"), "{}", bad.message);

    // #4889 bootstrap: a skill with no fail-items (main's, before this land) is strict.
    // Item 7 is a note under fail-items 1-2, and a fail when the skill names none.
    fs::remove_file(home.join("garbled")).unwrap();
    fs::write(home.join("gatefinding"), "").unwrap();
    fs::write(werk.join("a.rs"), "fn a() { d() }\n").unwrap();
    git(&werk, &["commit", "-q", "-am", "card 3"]);
    std::env::set_var("CHORUS_DEMO_ROUND", "r4");
    let note = run_gate_step(4889, "silas", &home, "code").unwrap();
    assert!(note.message.contains(": pass") && note.message.contains("note: a.rs:1 — item 7"), "{}", note.message);
    fs::write(home.join("skills/gate-code/SKILL.md"), "---\nname: gate-code\n---\n# gate-code\n").unwrap();
    fs::write(werk.join("a.rs"), "fn a() { e() }\n").unwrap();
    git(&werk, &["commit", "-q", "-am", "card 4"]);
    std::env::set_var("CHORUS_DEMO_ROUND", "r5");
    let strict = run_gate_step(4889, "silas", &home, "code").unwrap();
    assert!(strict.message.contains(": fail") && strict.message.contains("names no fail-items: every finding fails"), "{}", strict.message);

    // #4889 (Jeff 16:05) — a red round asks which findings break something Jeff uses, the
    // list read from the review.breaks.jeff_uses row (a stand-in door). NEGATIVE PROOF
    // first: with no door answering, no question is asked and the red stands
    fs::write(home.join("breaksnothing"), "").unwrap();
    std::env::set_var("OWL_URL", "http://127.0.0.1:9");
    fs::write(werk.join("a.rs"), "fn a() { e2() }\n").unwrap();
    git(&werk, &["commit", "-q", "-am", "card 4b"]);
    std::env::set_var("CHORUS_DEMO_ROUND", "r5b");
    let no_row = run_gate_step(4889, "silas", &home, "code").unwrap();
    assert!(no_row.message.contains(": fail"), "{}", no_row.message);
    std::env::set_var("OWL_URL", door(r#"{"data":[{"propertyKey":"review.breaks.jeff_uses","propertyScope":"global","propertyValue":"his messages"}]}"#));
    // breaks nothing → the finding is a note on his go page and the gate passes
    fs::write(werk.join("a.rs"), "fn a() { f() }\n").unwrap();
    git(&werk, &["commit", "-q", "-am", "card 5"]);
    std::env::set_var("CHORUS_DEMO_ROUND", "r6");
    let shipped = run_gate_step(4889, "silas", &home, "code").unwrap();
    assert!(shipped.message.contains(": pass") && shipped.message.contains("note: a.rs:1 — item 7"), "{}", shipped.message);
    assert_eq!(shipped.exit, 0, "notes only goes on to the go page");
    // NEGATIVE PROOF: a finding that breaks something Jeff uses still blocks
    fs::remove_file(home.join("breaksnothing")).unwrap();
    fs::write(home.join("breaks"), "").unwrap();
    fs::write(werk.join("a.rs"), "fn a() { g() }\n").unwrap();
    git(&werk, &["commit", "-q", "-am", "card 6"]);
    std::env::set_var("CHORUS_DEMO_ROUND", "r7");
    let blocked = run_gate_step(4889, "silas", &home, "code").unwrap();
    assert!(blocked.message.contains(": fail") && !blocked.message.contains("note: a.rs:1 — item 7"), "{}", blocked.message);
}
