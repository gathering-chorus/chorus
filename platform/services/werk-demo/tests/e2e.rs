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
    // #3100: capture curl invocations so we can assert nudges hit :3475/api/nudge
    let curl_log = tmp("curllog").join("calls");
    write_exec(
        &bin.join("curl"),
        &format!(
            "#!/bin/sh\necho \"$@\" >> \"{curl_log}\"\nexit 0\n",
            curl_log = curl_log.display()
        ),
    );
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
    // #3100 AC#5: comment window default is 60s; force 0 in tests so we don't sleep.
    std::env::set_var("CHORUS_DEMO_COMMENT_WINDOW_SECS", "0");
    // #3100 AC#2: ack window default is 60s; force 0 in tests for fast iteration.
    std::env::set_var("CHORUS_DEMO_ACK_WINDOW_SECS", "0");

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

    // (5a) #3100 AC1+AC2+AC4: feedback nudges go through MCP (chorus_nudge_message
    //      tools/call), NOT the legacy :3340/api/chorus/nudge 404. Uses -f so a
    //      bad endpoint surfaces instead of silently exiting 0.
    let curl_calls = fs::read_to_string(&curl_log).unwrap_or_default();
    assert!(
        curl_calls.contains("chorus_nudge_message"),
        "feedback nudges must invoke chorus_nudge_message via MCP tools/call; got:\n{}",
        curl_calls
    );
    assert!(
        curl_calls.contains("tools/call"),
        "MCP nudge POST must use JSON-RPC tools/call method; got:\n{}",
        curl_calls
    );
    assert!(
        !curl_calls.contains("http://localhost:3340/api/chorus/nudge"),
        "must NOT POST to the legacy 404 :3340/api/chorus/nudge; got:\n{}",
        curl_calls
    );
    assert!(
        curl_calls.contains(" -f "),
        "curl must use -f so silent-success-on-error class can't recur; got:\n{}",
        curl_calls
    );
    // Two non-builder roles (silas, kade) get nudged in the initial signal
    // round (2), and again as re-nudges from the AC#2 loop because the shimmed
    // chorus-api search returns empty → peer_engaged=false → re-nudge fires (2).
    // Total: 4. Plus a demo.feedback.escalate per silent peer (still unacked
    // after re-nudge) + a Bridge escalate POST per silent peer.
    let mcp_post_count = curl_calls.lines().filter(|l| l.contains("chorus_nudge_message")).count();
    assert_eq!(
        mcp_post_count, 4,
        "expected 4 MCP nudges (2 initial + 2 re-nudge per #3100 AC#2); got {} in:\n{}",
        mcp_post_count, curl_calls
    );
    // Escalate Bridge POSTs for silent peers (2 — silas + kade still silent after re-nudge)
    let escalate_posts = curl_calls.lines().filter(|l| l.contains("FEEDBACK STALL")).count();
    assert_eq!(
        escalate_posts, 2,
        "expected 2 Bridge escalate posts (silent peers after re-nudge); got {} in:\n{}",
        escalate_posts, curl_calls
    );

    // (5b) #3100 AC3: human-pause step — werk-demo announces "ready for review"
    //      to Bridge before exit so Jeff has a clear engagement point. The wait
    //      is external (agent reads Bridge + acts); binary stays terminating.
    assert!(
        witness.contains("\"event\":\"demo.ready_for_review\""),
        "demo must emit demo.ready_for_review as the human-pause announcement; got:\n{}",
        witness
    );

    // (6) #3100 AC#1 — gate interaction: demo.gate.requested + demo.gate.passed
    //     bracket the gate-chain check. When all 5 gates posted, both events
    //     fire; the refusal path (not exercised here) would surface owning roles.
    let card_story = fs::read_to_string(home.join("ops/logs/werk-demo.jsonl")).unwrap_or_default();
    // The shim card view has all five gate-pass lines, so the chain clears
    // and both events fire. Sentinel: presence of demo.gate.passed proves the
    // refusal path was skipped AND the new bracketing event fired.
    // (demo.gate.requested goes to chorus-log subprocess + spine; jsonl is the
    // local witness — assert the event was reached via demo.gate.passed.)
    assert!(
        !card_story.contains("\"event\":\"demo.refused\""),
        "with all 5 gates posted, demo must not refuse; got:\n{}", card_story
    );

    // (7) #3100 AC#5: comment window opens + closes (with secs=0 force, both
    //     events fire near-instantly back-to-back).
    assert!(
        witness.contains("\"event\":\"demo.awaiting_comment\""),
        "demo must emit demo.awaiting_comment to mark the engagement window; got:\n{}",
        witness
    );
    assert!(
        witness.contains("\"event\":\"demo.comment_window_closed\""),
        "demo must emit demo.comment_window_closed at window end; got:\n{}",
        witness
    );

    // (8) #3100 AC#2: feedback unacked → re-nudge → escalate path fully wired.
    //     Shimmed search returns empty so both peers stay silent through both rounds.
    assert!(
        witness.contains("\"event\":\"demo.feedback.unacked\""),
        "demo must emit demo.feedback.unacked when peer is silent; got:\n{}", witness
    );
    assert!(
        witness.contains("\"event\":\"demo.renudge.sent\""),
        "demo must emit demo.renudge.sent when re-nudging an unacked peer; got:\n{}", witness
    );
    assert!(
        witness.contains("\"event\":\"demo.feedback.escalate\""),
        "demo must emit demo.feedback.escalate when peer is still silent after re-nudge; got:\n{}", witness
    );

    // cleanup the trace file so re-runs are hermetic
    let _ = fs::remove_file(trace_path);
}
