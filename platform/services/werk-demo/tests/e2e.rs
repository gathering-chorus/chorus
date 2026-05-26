//! End-to-end: PATH-shim every external CLI (cards / werk-build / werk-deploy /
//! curl / smoke-check.sh / chorus-log / gh / git), prove the act-spine runs
//! validate → AC-preflight (+evidence+trace) → gate-chain → smoke → signal →
//! build → deploy → demo.show.completed, and assert the jsonl witness records
//! the expected events. One env-mutating test fn (so PATH / env can't race).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use werk_demo::demo;

fn nanos() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
}

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("wd-{}-{}-{}", tag, std::process::id(), nanos()));
    fs::create_dir_all(&p).unwrap();
    p
}

fn write_exec(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    let mut perm = fs::metadata(path).unwrap().permissions();
    perm.set_mode(0o755);
    fs::set_permissions(path, perm).unwrap();
}

#[test]
fn e2e_demo_happy_path() {
    // --- PATH shims ---
    let bin = tmp("bin");
    // cards view --json → {"status":"WIP"}; view (no --json) → a card body with all AC
    // checked + all five gate-pass comments + type:fix tag. comment/demo → exit 0
    // and append the comment text so we can assert evidence was posted.
    let cards_log = tmp("cardslog").join("calls");
    write_exec(
        &bin.join("cards"),
        &format!(
            r#"#!/bin/sh
echo "$@" >> "{cards_log}"
if [ "$1" = "view" ] && [ "$3" = "--json" ]; then
  printf '{{ "status": "WIP" }}\n'
  exit 0
fi
if [ "$1" = "view" ]; then
  cat <<'EOF'
#3046 demo-v2 test card
  Status: WIP
  Owner: wren
  Desc:
    ## Acceptance Criteria
    - [x] one ported
    - [x] two ported
  Domains: domain:chorus, type:fix
  Comments:
    [jeff] gate:product-pass — Wren
    [jeff] gate:code-pass — Kade
    [jeff] gate:quality-pass — Kade
    [jeff] gate:arch-pass — Silas
    [jeff] gate:ops-pass — Silas
EOF
  exit 0
fi
exit 0
"#,
            cards_log = cards_log.display()
        ),
    );
    write_exec(&bin.join("werk-build"), "#!/bin/sh\necho built\nexit 0\n");
    // #3098: capture werk-deploy args so we can assert the env-up variant call
    let deploy_log = tmp("deploylog").join("calls");
    write_exec(
        &bin.join("werk-deploy"),
        &format!(
            "#!/bin/sh\necho \"$@\" >> \"{deploy_log}\"\necho deployed\nexit 0\n",
            deploy_log = deploy_log.display()
        ),
    );
    write_exec(&bin.join("curl"), "#!/bin/sh\nexit 0\n");
    write_exec(&bin.join("gh"), "#!/bin/sh\nexit 0\n");

    // --- $HOME-like layout: home/platform/scripts/{smoke-check.sh,chorus-log} ---
    let home = tmp("home");
    fs::create_dir_all(home.join("platform/scripts")).unwrap();
    write_exec(
        &home.join("platform/scripts/smoke-check.sh"),
        "#!/bin/sh\nexit 0\n",
    );
    write_exec(
        &home.join("platform/scripts/chorus-log"),
        "#!/bin/sh\nexit 0\n",
    );

    // --- werk base + a real git repo at home/<role>/<card> for register_gh's git rev-parse ---
    let werk_base = tmp("werkbase");
    let werk_dir = werk_base.join("wren-3046");
    fs::create_dir_all(&werk_dir).unwrap();
    let st = std::process::Command::new("git")
        .args(["init", "-q", "-b", "main", werk_dir.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(st.success(), "git init failed");
    fs::write(werk_dir.join("README"), "x").unwrap();
    let _ = std::process::Command::new("git")
        .args(["-C", werk_dir.to_str().unwrap(), "add", "."])
        .status();
    let _ = std::process::Command::new("git")
        .args([
            "-C", werk_dir.to_str().unwrap(),
            "-c", "user.name=t", "-c", "user.email=t@t",
            "commit", "-q", "-m", "init",
        ])
        .status();

    // --- PATH: shims first ---
    std::env::set_var(
        "PATH",
        format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()),
    );
    std::env::set_var("CHORUS_TRACE_ID", "e2e-trace-abc123");

    // --- run the act ---
    let result = demo(3046, "wren", &home, &werk_base).expect("demo should succeed");
    assert!(result.contains("demo #3046"), "ok message: {}", result);
    assert!(result.contains("2/2 AC"), "ac count in message: {}", result);

    // --- assert side effects ---

    // (1) evidence comment was posted (cards comment ran)
    let calls = fs::read_to_string(&cards_log).unwrap_or_default();
    assert!(
        calls.contains("comment 3046 demo:preflight-pass"),
        "cards comment for evidence not invoked; calls log:\n{}",
        calls
    );
    // (2) trace file written (#2897 cross-hook propagation)
    let trace_path = "/tmp/demo-trace-3046.txt";
    let trace = fs::read_to_string(trace_path).unwrap_or_default();
    assert_eq!(
        trace.trim(),
        "e2e-trace-abc123",
        "trace file should hold CHORUS_TRACE_ID"
    );
    // (3) jsonl witness recorded the act-spine events in order
    let witness = fs::read_to_string(home.join("ops/logs/werk-demo.jsonl")).unwrap_or_default();
    for evt in [
        "demo.started",
        "demo.preflight.passed",
        "demo.smoke.passed",
        "demo.signal.completed",
        "demo.built",
        "demo.deployed",
        "demo.completed",
    ] {
        assert!(
            witness.contains(&format!("\"event\":\"{}\"", evt)),
            "witness missing event {}; got:\n{}",
            evt,
            witness
        );
    }

    // (4) #3098: werk-deploy must be called as `env-up <role> <card>`, not bare `<card>`
    //     — demo brings up the per-role variant from #3092, NOT the canonical deploy path.
    let deploy_calls = fs::read_to_string(&deploy_log).unwrap_or_default();
    assert!(
        deploy_calls.contains("env-up wren 3046"),
        "werk-deploy should be invoked as `env-up wren 3046` (per-role variant); got:\n{}",
        deploy_calls
    );
    assert!(
        !deploy_calls.lines().any(|l| l.trim() == "3046"),
        "werk-deploy must NOT be invoked with bare card-id (canonical deploy path); got:\n{}",
        deploy_calls
    );

    // cleanup the trace file so re-runs are hermetic
    let _ = fs::remove_file(trace_path);
}
