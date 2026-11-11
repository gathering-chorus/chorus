// @domain: cicd — picked by its owner, Kade, from the crawler's candidates (#4538)
//! Pure-helper unit tests (RED first) — the verb-contract helpers werk-push shares
//! with the blueprint: branch/card handling + the jsonl witness line shape, plus the
//! #3163 spine helpers (resolve_trace inheritance + the chorus-log args contract).

use werk_push::{branch_name, jsonl_line, parse_push_args, resolve_trace_in, spine_args};

#[test]
fn branch_name_is_role_slash_card() {
    assert_eq!(branch_name("kade", 3056), "kade/3056");
}

#[test]
fn jsonl_line_is_valid_witness_record() {
    let line = jsonl_line(1234, "push.started", "kade", 3056, "abc-1", "");
    assert!(line.ends_with('\n'));
    assert!(line.contains("\"event\":\"push.started\""));
    assert!(line.contains("\"card_id\":3056"));
    assert!(line.contains("\"trace_id\":\"abc-1\""));
    assert!(line.starts_with('{'));
}

#[test]
fn jsonl_line_appends_extra_fields_verbatim() {
    let line = jsonl_line(1, "push.completed", "kade", 1, "t", ",\"sha\":\"deadbeef\"");
    assert!(line.contains("\"sha\":\"deadbeef\""));
}

// #3163 — the spine helpers (the #3045 verb-contract observability). Mirror werk-commit:
// werk-push must surface push.failed/refused on the ONE spine, keyed by card + the
// INHERITED trace (not a fresh mint), so a rejected push shows on the card's thread.

#[test]
fn spine_args_is_the_chorus_log_contract() {
    let args = spine_args("push.failed", "kade", 3163, "trace-xyz", &[("reason", "non-fast-forward")]);
    assert_eq!(args, vec!["push.failed", "kade", "card=3163", "trace=trace-xyz", "reason=non-fast-forward"]);
}

#[test]
fn resolve_trace_inherits_the_env_trace() {
    let dir = std::env::temp_dir();
    // a present env trace is returned verbatim — the inheritance contract (not a mint).
    assert_eq!(resolve_trace_in(3163, Some("inherited-abc"), &dir), "inherited-abc");
    // blank env is NOT a trace — falls through to file/mint.
    assert_ne!(resolve_trace_in(3163, Some("   "), &dir), "   ");
}

#[test]
fn resolve_trace_mints_then_persists_so_one_trace_threads() {
    use std::fs;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = std::env::temp_dir().join(format!("wp-trace-{}-{}", std::process::id(), nanos));
    fs::create_dir_all(&dir).unwrap();
    let card = 4243;
    // no env + no file → mint AND persist the carrier (downstream verbs inherit it).
    let minted = resolve_trace_in(card, None, &dir);
    assert!(!minted.is_empty());
    assert!(dir.join(format!("{}-trace", card)).exists(), "the mint persists the carrier file");
    // second resolve (no env) reads the SAME persisted trace → ONE trace threads through.
    assert_eq!(resolve_trace_in(card, None, &dir), minted);
}

// #3296 — `push --atomic`: the flag must be RECOGNIZED at the CLI seam, not silently
// dropped (run_push ignored nth(3+)) or mis-parsed as the role (`werk-push 3296 --atomic`
// read --atomic as role). push is in the ADR-037 --atomic-FREE group (reversible, no
// approval), so --atomic does not change behavior — but it MUST parse cleanly: card +
// role correct, atomic=true, regardless of flag position.
#[test]
fn parse_push_args_recognizes_atomic_anywhere() {
    // flag trailing
    let (card, role, atomic) =
        parse_push_args(&["3296".into(), "kade".into(), "--atomic".into()], None).unwrap();
    assert_eq!((card, role.as_str(), atomic), (3296, "kade", true));

    // flag BETWEEN card and role — must NOT be mistaken for the role
    let (card, role, atomic) =
        parse_push_args(&["3296".into(), "--atomic".into(), "kade".into()], None).unwrap();
    assert_eq!((card, role.as_str(), atomic), (3296, "kade", true), "--atomic not mistaken for role");

    // no flag → atomic=false, role from the DEPLOY_ROLE fallback
    let (card, role, atomic) = parse_push_args(&["3296".into()], Some("kade".into())).unwrap();
    assert_eq!((card, role.as_str(), atomic), (3296, "kade", false));

    // non-numeric card → Err (the contract guard)
    assert!(parse_push_args(&["notanum".into(), "kade".into()], None).is_err());
}

// #4710 — the card's PR exists before Jeff's go, so the lander can find it. A gh stand-in
// answers `pr list` from $GH_OPEN and logs every call.
mod ensure_pr_4710 {
    use std::os::unix::fs::PermissionsExt;
    fn gh(open: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let d = std::env::temp_dir().join(format!("ep4710-{}-{}", std::process::id(), open.replace(' ', "_")));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let log = d.join("log");
        let f = d.join("gh");
        std::fs::write(&f, format!("#!/bin/sh\necho \"$@\" >> {}\ncase \"$2\" in list) printf '{}\\n';; create) echo https://github.com/gathering-chorus/chorus/pull/2001;; esac\n", log.display(), open.replace(' ', "\\n"))).unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        (d, log)
    }
    // PATH is process-wide and these tests run in parallel: one at a time (Silas 18:38 saw them race)
    static PATH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn with_path<T>(d: &std::path::Path, f: impl FnOnce() -> T) -> T {
        let _g = PATH_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let old = std::env::var_os("PATH").unwrap_or_default();
        let mut p = std::ffi::OsString::from(d.as_os_str()); p.push(":"); p.push(&old);
        std::env::set_var("PATH", &p);
        let r = f();
        std::env::set_var("PATH", old);
        r
    }

    #[test]
    fn ensure_pr_creates_one_when_none_is_open_and_reuses_one_that_is() {
        let (d, log) = gh("");
        let n = with_path(&d, || werk_push::ensure_pr(d.to_str().unwrap(), 4710, "wren", "t", "wren/4710")).unwrap();
        assert_eq!(n, 2001);
        let calls = std::fs::read_to_string(&log).unwrap();
        assert!(calls.contains("pr create --base main --head wren/4710 --title #4710 (wren)"), "{calls}");
        let (d, log) = gh("1999");
        assert_eq!(with_path(&d, || werk_push::ensure_pr(d.to_str().unwrap(), 4710, "wren", "t", "wren/4710")).unwrap(), 1999);
        assert!(!std::fs::read_to_string(&log).unwrap().contains("pr create"), "a second PR was created");
    }

    // NEGATIVE PROOF: two open PRs for one branch would make the lander's pick ambiguous: refused
    #[test]
    fn two_open_prs_are_refused() {
        let (d, _) = gh("1999 2000");
        let e = with_path(&d, || werk_push::ensure_pr(d.to_str().unwrap(), 4710, "wren", "t", "wren/4710")).unwrap_err();
        assert!(e.contains("2 open PRs"), "{e}");
    }
}
