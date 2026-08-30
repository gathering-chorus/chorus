//! #3787 — the lint-ratchet werk-test leg, proven against the REAL
//! `lint-ratchet.js` with a stub eslint (a test brings its own world, #3528).
//!
//! One test fn on purpose: the legs share process env (LINT_RATCHET_* vars),
//! and separate #[test] fns run as parallel threads in one process — a split
//! would race. Order inside: negative proof FIRST (#3734 — watch the check
//! fail against the violated state), then the green leg, then the loud-absence
//! guard.

use std::io::Write;
use std::path::Path;

fn write_exec(path: &Path, content: &str) {
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(content.as_bytes()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// Build a fixture werk: the REAL lint-ratchet.js + a baseline allowing 1
/// `demo/rule` hit + a stub eslint emitting `n` hits of that rule.
fn fixture(werk: &Path, eslint_json: &str, baseline: &str) {
    std::fs::create_dir_all(werk.join("platform/scripts")).unwrap();
    // The real script under test — copied from this werk's tree, not a mock.
    let real = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../scripts/lint-ratchet.js");
    std::fs::copy(&real, werk.join("platform/scripts/lint-ratchet.js")).unwrap();
    std::fs::write(werk.join(".eslint-baseline.json"), baseline).unwrap();
    write_exec(
        &werk.join("eslint-stub.sh"),
        &format!("#!/bin/sh\ncat <<'JSON'\n{}\nJSON\n", eslint_json),
    );
}

#[test]
fn lint_ratchet_refuses_over_baseline_and_passes_at_baseline() {
    let root = std::env::temp_dir().join(format!("lint-ratchet-3787-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);

    let baseline = r#"{ "counts": { "demo/rule": 1 } }"#;
    let over = r#"[ { "filePath": "src/a.ts", "messages": [
          { "ruleId": "demo/rule", "line": 12 }, { "ruleId": "demo/rule", "line": 40 } ] } ]"#;
    let at = r#"[ { "filePath": "src/a.ts", "messages": [ { "ruleId": "demo/rule", "line": 12 } ] } ]"#;

    // ── #3734 NEGATIVE PROOF: the violated state (a rule pushed over baseline)
    // must make the leg go RED — and the refusal must name rule, count, and
    // file:line (AC1), because an unnamed refusal just moves the archaeology
    // from the nightly to the land.
    let red = root.join("red-werk");
    fixture(&red, over, baseline);
    std::env::set_var("LINT_RATCHET_ESLINT_BIN", red.join("eslint-stub.sh"));
    assert!(
        !werk_test::run_lint_ratchet(red.to_str().unwrap()),
        "a diff pushing demo/rule over baseline MUST refuse the leg"
    );
    // Same fixture, run directly, to assert the refusal text carries the sites.
    let out = std::process::Command::new("node")
        .arg(red.join("platform/scripts/lint-ratchet.js"))
        .current_dir(&red)
        .env("LINT_RATCHET_ROOT", &red)
        .env("LINT_RATCHET_ESLINT_BIN", red.join("eslint-stub.sh"))
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("demo/rule: 2 (baseline 1, +1)"), "rule+count: {}", err);
    assert!(err.contains("src/a.ts:12") && err.contains("src/a.ts:40"), "file:line sites: {}", err);

    // ── GREEN: the same werk at baseline passes.
    let green = root.join("green-werk");
    fixture(&green, at, baseline);
    std::env::set_var("LINT_RATCHET_ESLINT_BIN", green.join("eslint-stub.sh"));
    assert!(
        werk_test::run_lint_ratchet(green.to_str().unwrap()),
        "the same werk without the violation must pass"
    );

    // ── Missing script in a PRE-ratchet tree (no baseline either) = pass.
    let bare = root.join("bare-werk");
    std::fs::create_dir_all(&bare).unwrap();
    assert!(werk_test::run_lint_ratchet(bare.to_str().unwrap()));

    // ── A guard whose target is deleted must fail LOUDLY, never pass
    // vacuously (#3734). A tree that carries the baseline but not the script
    // is a post-#3787 tree with the ratchet deleted — refuse.
    let deleted = root.join("deleted-werk");
    std::fs::create_dir_all(&deleted).unwrap();
    std::fs::write(deleted.join(".eslint-baseline.json"), baseline).unwrap();
    assert!(
        !werk_test::run_lint_ratchet(deleted.to_str().unwrap()),
        "baseline present + script deleted must refuse, not vacuously pass"
    );

    std::env::remove_var("LINT_RATCHET_ESLINT_BIN");
    let _ = std::fs::remove_dir_all(&root);
}
