// @test-type: unit
// @domain: tests
// Test-first (DEC-1674): these reference werk_pull lib functions that don't exist
// yet — RED until src/lib.rs is written.
use werk_pull::{branch_name, jsonl_line, parse_pull_args, trace_id, resolve_trace_in, spine_args};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

// Hermetic temp dir per test — the trace dir is injected (testable-core blueprint),
// so resolve_trace never touches the real /tmp during unit tests.
fn tmpdir(tag: &str) -> PathBuf {
    let ns = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let p = std::env::temp_dir().join(format!("wp-trace-{}-{}-{}", tag, std::process::id(), ns));
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn branch_is_role_slash_card() {
    assert_eq!(branch_name("kade", 3045), "kade/3045");
}

#[test]
fn trace_id_nonempty_and_shaped() {
    let t = trace_id();
    assert!(!t.is_empty(), "trace id must not be empty");
    assert!(t.contains('-'), "trace id must contain the time-pid separator");
}

#[test]
fn jsonl_line_is_valid_shape_with_card_and_trace() {
    let line = jsonl_line(1234, "pull.completed", "kade", 3045, "abc-1", ",\"branch\":\"kade/3045\"");
    assert!(line.starts_with('{') && line.trim_end().ends_with('}'));
    assert!(line.contains("\"card_id\":3045"));
    assert!(line.contains("\"trace_id\":\"abc-1\""));
    assert!(line.contains("\"event\":\"pull.completed\""));
    assert!(line.contains("\"branch\":\"kade/3045\""));
}

// --- resolve_trace (CONSISTENT, #3135): env -> /tmp/<card>-trace -> mint+persist.
// The persisted file is the cross-process carrier that threads ONE trace across
// pull -> demo -> acp. Replaces werk-pull's #3063 fresh-mint drift.

#[test]
fn resolve_trace_env_wins_over_file() {
    let d = tmpdir("env");
    fs::write(d.join("7-trace"), "file-trace").unwrap(); // present, but env must win
    assert_eq!(resolve_trace_in(7, Some("env-trace"), &d), "env-trace");
}

#[test]
fn resolve_trace_file_fallback_when_no_env() {
    let d = tmpdir("file");
    fs::write(d.join("8-trace"), "file-trace\n").unwrap();
    assert_eq!(resolve_trace_in(8, None, &d), "file-trace");
}

#[test]
fn resolve_trace_mints_and_persists_when_neither() {
    let d = tmpdir("mint");
    let t = resolve_trace_in(9, None, &d);
    assert!(!t.is_empty(), "minted trace must be non-empty");
    assert!(t.contains('-'), "minted trace keeps the time-pid shape");
    let persisted = fs::read_to_string(d.join("9-trace")).expect("trace file must be written");
    assert_eq!(persisted.trim(), t, "persisted file must equal the returned trace (downstream inherits it)");
}

#[test]
fn resolve_trace_blank_env_falls_through_to_mint() {
    let d = tmpdir("blank");
    let t = resolve_trace_in(10, Some("   "), &d);
    assert!(!t.is_empty(), "blank env must not be used as the trace");
    assert!(fs::read_to_string(d.join("10-trace")).is_ok(), "must persist the minted trace");
}

// --- spine (AUDITABLE, #3135): the lifecycle event reaches the ONE spine
// (~/.chorus/chorus.log via chorus-log) so a pull is queryable in Loki, carrying
// the shared trace + card. spine_args is the pure contract (testable); emit_spine
// shells chorus-log best-effort. Replaces werk-pull's jsonl-witness-only (Loki-invisible).

#[test]
fn spine_args_carry_event_role_card_trace() {
    let a = spine_args("card.pulled", "kade", 3135, "abc-1", &[]);
    assert_eq!(a[0], "card.pulled", "event is first");
    assert_eq!(a[1], "kade", "role is second");
    assert!(a.contains(&"card=3135".to_string()), "card stamped for borg-read-logs-card join");
    assert!(a.contains(&"trace=abc-1".to_string()), "trace stamped so pull correlates with demo+acp");
}

// #3161: failure emits carry disposition + reason so the #3165 rollup counts them
// and the trace reader shows WHY a pull was refused/rolled-back, not just that it was.
#[test]
fn spine_args_carry_disposition_and_reason_extras() {
    let a = spine_args(
        "pull.refused",
        "kade",
        3161,
        "abc-1",
        &[("disposition", "refuse"), ("reason", "wrong-status")],
    );
    assert_eq!(a[0], "pull.refused", "event is first");
    assert!(a.contains(&"card=3161".to_string()), "card still stamped");
    assert!(a.contains(&"trace=abc-1".to_string()), "trace still stamped");
    assert!(a.contains(&"disposition=refuse".to_string()), "rollup keys on disposition (#3165)");
    assert!(a.contains(&"reason=wrong-status".to_string()), "reason explains the refusal");
}

// #3294 — pull --atomic parse (the standalone-worktree door; same CLI-seam discipline
// as push #3296 / merge #3297 / accept #3298): recognize --atomic anywhere, never
// mis-read as the role.
#[test]
fn parse_pull_args_recognizes_atomic_anywhere() {
    let (c, r, a) = parse_pull_args(&["3294".into(), "kade".into(), "--atomic".into()], None).unwrap();
    assert_eq!((c, r.as_str(), a), (3294, "kade", true));
    let (c, r, a) = parse_pull_args(&["3294".into(), "--atomic".into(), "kade".into()], None).unwrap();
    assert_eq!((c, r.as_str(), a), (3294, "kade", true), "--atomic not mistaken for role");
    let (c, r, a) = parse_pull_args(&["3294".into()], Some("kade".into())).unwrap();
    assert_eq!((c, r.as_str(), a), (3294, "kade", false));
    assert!(parse_pull_args(&["notanum".into(), "kade".into()], None).is_err());
}

// --- #3842 repo-aware pull: card declares a target repo via a `repo:<name>` label ---

#[test]
fn target_repo_default_is_home_when_no_label() {
    let home = tmpdir("trh");
    let json = r#"{"index":1,"domains":["domain:chorus","type:enhance"]}"#;
    let got = werk_pull::target_repo_root(&home, json).unwrap();
    assert_eq!(got, home);
}

#[test]
fn target_repo_label_resolves_to_sibling_git_repo() {
    let parent = tmpdir("trs");
    let home = parent.join("chorus");
    let target = parent.join("shared-security");
    std::fs::create_dir_all(home.join(".git")).unwrap();
    std::fs::create_dir_all(target.join(".git")).unwrap();
    let json = r#"{"index":1,"domains":["domain:chorus","repo:shared-security"]}"#;
    let got = werk_pull::target_repo_root(&home, json).unwrap();
    assert_eq!(got, target);
}

/// #3734 negative proof: a declared target that is missing or not a git repo
/// refuses with a typed reason — never a silent fallback to chorus.
#[test]
fn target_repo_missing_or_not_git_refuses_typed() {
    let parent = tmpdir("trm");
    let home = parent.join("chorus");
    std::fs::create_dir_all(home.join(".git")).unwrap();
    let json = r#"{"index":1,"domains":["repo:not-there"]}"#;
    let err = werk_pull::target_repo_root(&home, json).unwrap_err();
    assert!(err.contains("target-repo-missing"), "typed reason, got: {err}");

    let flat = parent.join("flat"); // exists but not a git repo
    std::fs::create_dir_all(&flat).unwrap();
    let json2 = r#"{"index":1,"domains":["repo:flat"]}"#;
    let err2 = werk_pull::target_repo_root(&home, json2).unwrap_err();
    assert!(err2.contains("target-repo-not-git"), "typed reason, got: {err2}");
}

/// Path traversal in a label must refuse — `repo:../../etc` cannot escape
/// the projects directory.
#[test]
fn target_repo_label_with_traversal_refuses() {
    let parent = tmpdir("trt");
    let home = parent.join("chorus");
    std::fs::create_dir_all(home.join(".git")).unwrap();
    let json = r#"{"index":1,"domains":["repo:../evil"]}"#;
    let err = werk_pull::target_repo_root(&home, json).unwrap_err();
    assert!(err.contains("target-repo-invalid"), "typed reason, got: {err}");
}

// --- #4810: every werk package gets a real node_modules, never a symlink into canonical ---

fn git_track(dir: &std::path::Path, files: &[&str]) {
    let g = |args: &[&str]| {
        let ok = std::process::Command::new("git").arg("-C").arg(dir).args(args).output().unwrap().status.success();
        assert!(ok, "git {:?}", args);
    };
    g(&["init", "-q"]);
    let mut add = vec!["add", "--"];
    add.extend_from_slice(files);
    g(&add);
}

#[test]
fn materialize_clones_canonical_when_the_lock_matches_and_stamps_it() {
    let root = tmpdir("nm-4810");
    let (repo, werk) = (root.join("chorus"), root.join("werk"));
    for base in [&repo, &werk] {
        fs::create_dir_all(base.join("cards")).unwrap();
        fs::write(base.join("cards/package.json"), "{}").unwrap();
        fs::write(base.join("cards/package-lock.json"), "{\"v\":1}").unwrap();
    }
    fs::create_dir_all(repo.join("cards/node_modules/dep")).unwrap();
    fs::write(repo.join("cards/node_modules/dep/index.js"), "live").unwrap();
    // a stale symlink into canonical, as the old pull left it
    std::os::unix::fs::symlink(repo.join("cards/node_modules"), werk.join("cards/node_modules")).unwrap();
    git_track(&werk, &["cards/package.json"]);

    let (made, failed) = werk_pull::materialize_node_modules(&repo, &werk);
    let nm = werk.join("cards/node_modules");
    assert_eq!((made, failed.len()), (1, 0));
    assert!(!nm.is_symlink() && nm.join("dep/index.js").is_file(), "a real tree with canonical's contents");
    assert_eq!(fs::read(nm.join(".chorus-installed-lock")).unwrap(), b"{\"v\":1}");
    // negative proof: what npm ci does first (delete node_modules) now leaves canonical intact
    fs::remove_dir_all(&nm).unwrap();
    assert!(repo.join("cards/node_modules/dep/index.js").is_file(), "canonical untouched");
}

#[test]
fn materialize_installs_from_own_lock_when_it_differs_and_names_failures() {
    let root = tmpdir("nm-4810-lock");
    let (repo, werk) = (root.join("chorus"), root.join("werk"));
    for (base, lock) in [(&repo, "{\"v\":1}"), (&werk, "{\"v\":2}")] {
        fs::create_dir_all(base.join("api")).unwrap();
        fs::write(base.join("api/package.json"), "{}").unwrap();
        fs::write(base.join("api/package-lock.json"), lock).unwrap();
    }
    fs::create_dir_all(repo.join("api/node_modules/old")).unwrap();
    git_track(&werk, &["api/package.json"]);
    // a stand-in npm that fails: the lock differs, so canonical's tree is NOT borrowed
    let npm = root.join("npm-fails");
    fs::write(&npm, "#!/bin/sh\nexit 1\n").unwrap();
    let mut p = fs::metadata(&npm).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut p, 0o755);
    fs::set_permissions(&npm, p).unwrap();
    std::env::set_var("CHORUS_NPM", &npm);
    let (made, failed) = werk_pull::materialize_node_modules(&repo, &werk);
    std::env::remove_var("CHORUS_NPM");
    assert_eq!(made, 0);
    assert_eq!(failed, vec!["api".to_string()], "the package is named, not silently skipped");
    assert!(!werk.join("api/node_modules/old").exists(), "the old-lock tree was not cloned");
}
