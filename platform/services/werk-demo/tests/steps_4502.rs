// @test-type: integration
// @domain: builds
//! #4502 — each demo step is its own dagu step with an exit contract dagu can
//! read: a gate or review that recorded a verdict exits 0 (it informs, Jeff's
//! go decides, #3263); one that could not run exits non-zero and says why.
//! A stand-in `claude` (CHORUS_CLAUDE_BIN) answers; the card's werk is a real
//! git repo so branch_diff reads it. One env-mutating test fn (no races).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_demo::{announce_missing, demo_role, run_gate_step, step_outcome, step_result};

fn tmp(tag: &str) -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("wd4502-{}-{}-{}", tag, std::process::id(), n));
    fs::create_dir_all(&p).unwrap();
    p
}

fn write_exec(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    let mut perm = fs::metadata(path).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(path, perm).unwrap();
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git").arg("-C").arg(dir).args(args)
        .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
        .output().unwrap().status.success();
    assert!(ok, "git {:?} failed", args);
}

fn witness(home: &Path) -> String {
    fs::read_to_string(home.join("ops/logs/werk-demo.jsonl")).unwrap_or_default()
}

#[test]
fn each_step_exits_by_what_it_recorded() {
    let home = tmp("home");
    let base = tmp("werks");
    let werk = base.join("wren-4502");
    fs::create_dir_all(home.join("platform/scripts")).unwrap();
    fs::create_dir_all(home.join("skills/gate-code")).unwrap();
    fs::create_dir_all(home.join("roles/silas")).unwrap();
    fs::create_dir_all(home.join("ops/logs")).unwrap();
    fs::write(home.join("skills/gate-code/SKILL.md"), "---\nname: gate-code\nfail-items: 1\n---\n# gate-code\n1. Logic.\n").unwrap();
    fs::write(home.join("roles/silas/CLAUDE.md"), "# Silas\nArchitect.\n").unwrap();
    // #4502 — abby-normal runs on agy: her persona is AGENTS.md, not CLAUDE.md
    fs::create_dir_all(home.join("roles/abby-normal")).unwrap();
    fs::write(home.join("roles/abby-normal/AGENTS.md"), "# Abby\nProgram manager.\n").unwrap();
    write_exec(&home.join("platform/scripts/cards"), "#!/bin/sh\necho '#4502 card'\nexit 0\n");
    write_exec(&home.join("platform/scripts/chorus-log"), "#!/bin/sh\nexit 0\n");
    // #4959 — every review runs its gate's policies; none carry a check here
    write_exec(&home.join("platform/scripts/policy-validate"), "#!/bin/sh\necho 'policy-validate: 0 checks, 0 above baseline, 0 could not run'\n");
    // the card's werk: origin/main, then one commit on top — a real branch diff
    fs::create_dir_all(&werk).unwrap();
    git(&werk, &["init", "-q"]);
    fs::write(werk.join("a.txt"), "one\n").unwrap();
    git(&werk, &["add", "a.txt"]);
    git(&werk, &["commit", "-q", "-m", "base"]);
    git(&werk, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    fs::write(werk.join("a.txt"), "two\n").unwrap();
    git(&werk, &["commit", "-q", "-am", "card"]);

    // the stand-in model: says pass, or fails when ./fail exists
    let fail_flag = home.join("fail");
    let claude = home.join("claude-stub");
    write_exec(&claude, &format!(
        "#!/bin/sh\ncat >/dev/null\nif [ -f \"{}\" ]; then echo 'model down' >&2; exit 1; fi\n# #4889 the code step also runs /code-review and /security-review (prompt in argv)\ncase \"$*\" in *-review*) printf 'done\\n```json\\n[]\\n```\\n'; exit 0;; esac\nif [ -f \"{}\" ]; then echo '{{\"findings\":[{{\"item\":1,\"where\":\"a.txt:1\",\"what\":\"door not answering\"}}]}}'; exit 0; fi\necho '{{\"findings\":[]}}'\n",
        fail_flag.display(), home.join("red").display()
    ));
    std::env::set_var("CHORUS_CLAUDE_BIN", &claude);
    // #4921 — the gate runs claude as the card's role account; the stand-in
    // runs here, so the test is that account (no sudo hop)
    std::env::set_var("USER", "chorus-wren");
    std::env::remove_var("CHORUS_GATE_PROFILE");
    std::env::set_var("CHORUS_WERK_BASE", &base);
    std::env::set_var("CHORUS_MEM_FLOOR_DISABLE", "1");
    std::env::set_var("CHORUS_DEMO_ROUND", "r4502");
    std::env::set_var("CHORUS_TRACE_ID", "t4502");
    std::env::set_var("CHORUS_HOOK_SHIM", "/nonexistent/chorus-hook-shim-4515");

    // NEGATIVE PROOF: the model fails → the gate records "error" → exit 1, named
    fs::write(&fail_flag, "").unwrap();
    let down = run_gate_step(4502, "wren", &home, "code").unwrap();
    assert_eq!(down.exit, 1, "a gate that could not run exits non-zero: {}", down.message);
    assert!(down.message.contains("could not run") && down.message.contains("model down"), "names why: {}", down.message);

    // #4851 — a recorded FAIL is not final for the round: the retry runs the gate
    // again (the 13:51 ops red was a door blip). The no-op below is the negative
    // proof: a recorded pass is never re-run.
    fs::remove_file(&fail_flag).unwrap();
    fs::write(home.join("red"), "").unwrap();
    let red = run_gate_step(4502, "wren", &home, "code").unwrap();
    assert!(red.message.contains(": fail"), "{}", red.message);
    fs::remove_file(home.join("red")).unwrap();
    // #4889 (Jeff 11-05 10:06) — at the same commit the finding carries forward: a re-run
    // never clears a red by the reviewer changing its mind
    let same = run_gate_step(4502, "wren", &home, "code").unwrap();
    assert!(same.message.contains(": fail") && same.message.contains("carried (not yet fixed): a.txt:1"), "{}", same.message);
    // a commit that touches a.txt and is not raised again fixes it
    fs::write(werk.join("a.txt"), "three\n").unwrap();
    git(&werk, &["commit", "-q", "-am", "fix the door"]);

    // the model answers → the gate records pass → exit 0 (a re-run, not "already recorded")
    let up = run_gate_step(4502, "wren", &home, "code").unwrap();
    assert_eq!(up.exit, 0, "a recorded verdict exits 0: {}", up.message);
    assert!(up.message.contains("gate code for #4502: pass") && !up.message.contains("already recorded"), "{}", up.message);
    let after = witness(&home).matches("\"event\":\"demo.gate.result\"").count();

    // a retry after a pass is a no-op: no second model run, no new record
    fs::write(&fail_flag, "").unwrap();
    let again = run_gate_step(4502, "wren", &home, "code").unwrap();
    assert_eq!(again.exit, 0, "{}", again.message);
    assert!(again.message.contains("already recorded"), "{}", again.message);
    assert_eq!(witness(&home).matches("\"event\":\"demo.gate.result\"").count(), after, "no re-run");
    fs::remove_file(&fail_flag).unwrap();

    // NEGATIVE PROOF: an unknown gate is refused before anything runs
    assert!(run_gate_step(4502, "wren", &home, "vibes").is_err());

    // #4889 — announce still needs the other two reviews (arch, quality); no peer reviews
    let need = announce_missing(&witness(&home), 4502, "wren", "r4502", "");
    assert_eq!(need, vec!["gate-arch", "gate-quality"], "{:?}", need);

}


#[test]
fn step_outcome_reads_error_and_absence_as_could_not_run() {
    assert_eq!(step_outcome("gate x", 1, None).exit, 1);
    assert_eq!(step_outcome("gate x", 1, Some(("error".into(), "timed out".into()))).exit, 1);
    // gates inform, the go decides: a fail or a block verdict still exits 0
    assert_eq!(step_outcome("gate x", 1, Some(("fail".into(), "red".into()))).exit, 0);
    assert_eq!(step_outcome("review by y", 1, Some(("block".into(), "no".into()))).exit, 0);
}

#[test]
fn step_result_takes_the_latest_line_for_this_round_only() {
    let w = concat!(
        "{\"event\":\"demo.gate.result\",\"card_id\":7,\"gate\":\"ops\",\"round\":\"r1\",\"result\":\"error\",\"findings\":\"down\"}\n",
        "{\"event\":\"demo.gate.result\",\"card_id\":7,\"gate\":\"ops\",\"round\":\"r1\",\"result\":\"pass\",\"findings\":\"ok\"}\n",
        "{\"event\":\"demo.gate.result\",\"card_id\":7,\"gate\":\"ops\",\"round\":\"r0\",\"result\":\"fail\",\"findings\":\"old\"}\n",
    );
    assert_eq!(step_result(w, 7, "demo.gate.result", "gate", "ops", "result", "findings", "r1", ""),
               Some(("pass".to_string(), "ok".to_string())));
    // NEGATIVE PROOF: another round, or a card id that only shares a prefix, matches nothing
    assert_eq!(step_result(w, 7, "demo.gate.result", "gate", "ops", "result", "findings", "r9", ""), None);
    assert_eq!(step_result(w, 70, "demo.gate.result", "gate", "ops", "result", "findings", "r1", ""), None);
}

#[test]
fn a_step_takes_its_role_from_its_third_word() {
    let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(demo_role(&a(&["run-gate", "4502", "kade", "code"]), "").unwrap(), "kade");
    assert_eq!(demo_role(&a(&["announce", "4502", "wren"]), "silas").unwrap(), "wren");
    // NEGATIVE PROOF: a step with no role and no DEPLOY_ROLE is refused, never blank
    assert!(demo_role(&a(&["validate", "4502"]), "").is_err());
}

/// #4502 — PEER_ROLES is the set of roles with a tracked CLAUDE.md source
/// (designing/claudemd/roles/<role>/; the roles/<role>/CLAUDE.md beside each is
/// generated and gitignored, and abby-normal's agy persona is roles/abby-normal/
/// AGENTS.md). A role added or removed without the list is caught here.
/// Negative proof: a source set with one more role does not match.
fn source_roles(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn peer_roles_match_the_role_dirs_with_a_persona() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let src = Path::new(&manifest).join("../../../designing/claudemd/roles");
    let mut want: Vec<String> =
        werk_demo::PEER_ROLES.iter().chain(werk_demo::PROVER_ROLES.iter()).map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(source_roles(&src), want, "PEER_ROLES + PROVER_ROLES drifted from designing/claudemd/roles/*");

    // negative proof: one more role source is a mismatch
    let t = std::env::temp_dir().join(format!("peers4502-{}", std::process::id()));
    for r in werk_demo::PEER_ROLES.iter().chain(werk_demo::PROVER_ROLES.iter()).chain(["newrole"].iter()) {
        fs::create_dir_all(t.join(r)).unwrap();
    }
    assert_ne!(source_roles(&t), want, "a fifth role must not match the four peers");
    let _ = fs::remove_dir_all(&t);
}

/// #4502 — the demo DAG can run alone ("more"): with no TRACE dagu sets
/// CHORUS_TRACE_ID to "", which must mean a fresh trace, not an empty one.
/// Negative proof: a real trace passed by cicd is kept as is.
#[test]
fn a_demo_run_alone_gets_its_own_trace_and_a_parent_trace_is_kept() {
    let fresh = werk_demo::pick_trace(Some(String::new()));
    assert!(!fresh.trim().is_empty(), "empty TRACE became an empty trace");
    assert!(!werk_demo::pick_trace(Some("  ".into())).trim().is_empty());
    assert!(!werk_demo::pick_trace(None).is_empty());
    assert_eq!(werk_demo::pick_trace(Some("034iParent".into())), "034iParent");
}
