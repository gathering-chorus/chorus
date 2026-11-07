// @test-type: unit — stub chorus-go binaries in a temp dir; no git, no network
// @domain: security
// @card: #4710
// @owner: silas
//! #4710 — src/lib.rs `go_check`: werk-merge lands only what chorus-go says
//! Jeff's passkey signed. A refusal, and a missing checker, both stop the merge.
use std::fs;
use std::os::unix::fs::PermissionsExt;
use werk_merge::go_check;

fn stub(tag: &str, body: &str) -> String {
    let d = std::env::temp_dir().join(format!("wmerge-go-{tag}-{}", std::process::id()));
    fs::create_dir_all(&d).unwrap();
    let p = d.join("chorus-go");
    fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    p.to_str().unwrap().to_string()
}

// one test: CHORUS_GO_BIN is process-wide
#[test]
fn the_merge_follows_chorus_gos_answer_and_refuses_without_it() {
    std::env::set_var("CHORUS_GO_BIN", stub("ok", "echo \"chorus-go: #$2 round $3 — go signed by Jeff's passkey\""));
    let ok = go_check(4717, "da13a20c").unwrap();
    assert!(ok.contains("#4717 round da13a20c"), "the card and the round reach chorus-go: {ok}");

    // NEGATIVE PROOF: a go nobody's phone signed
    std::env::set_var("CHORUS_GO_BIN", stub("no", "echo 'chorus-go: refused: no go signed by Jeff passkey for #4717' >&2; exit 1"));
    let e = go_check(4717, "da13a20c").unwrap_err();
    assert!(e.starts_with("go-unsigned: chorus-go: refused"), "{e}");

    // NEGATIVE PROOF: the checker is gone — refuse, never pass
    std::env::set_var("CHORUS_GO_BIN", "/nonexistent/chorus-go");
    let e = go_check(4717, "da13a20c").unwrap_err();
    assert!(e.contains("cannot run /nonexistent/chorus-go"), "{e}");
}
