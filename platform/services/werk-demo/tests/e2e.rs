//! End-to-end (#3116): PATH-shim every external CLI (cards / werk-build /
//! werk-deploy / curl / chorus-log / gh), prove the stripped proving ceremony
//! runs validate → AC-preflight (+evidence+trace) → gate-delegated → feedback
//! gather → review window → demo.verdict, assert the jsonl witness records the
//! expected events, AND assert demo NEVER builds or deploys (the act is out).
//! One env-mutating test fn (so PATH / env can't race).

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
    // #3116/#3183: werk-demo resolves `cards` ABSOLUTELY at home/platform/scripts/cards
    // (not bare on PATH) — so the cards shim is written THERE (below, once home exists),
    // not on PATH. This is the bug the first live run caught: bare `cards` died under
    // werk-mcp.sh's step-4.5 PATH.
    // #3116: werk-build / werk-deploy are shimmed to LOG if called — demo must
    // NEVER call them (the act is out). Assertions below require both logs empty.
    let build_log = tmp("buildlog").join("calls");
    write_exec(
        &bin.join("werk-build"),
        &format!("#!/bin/sh\necho \"$@\" >> \"{}\"\necho built\nexit 0\n", build_log.display()),
    );
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
    // cards shim at the ABSOLUTE path werk-demo resolves (home/platform/scripts/cards).
    write_exec(
        &home.join("platform/scripts/cards"),
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
EOF
  exit 0
fi
exit 0
"#,
            cards_log = cards_log.display()
        ),
    );
    write_exec(
        &home.join("platform/scripts/smoke-check.sh"),
        "#!/bin/sh\nexit 0\n",
    );
    write_exec(
        &home.join("platform/scripts/chorus-log"),
        "#!/bin/sh\nexit 0\n",
    );

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
    // #3100 gate-request fan-out: wait default 120s; force 0 in tests.
    std::env::set_var("CHORUS_DEMO_GATE_WAIT_SECS", "0");

    // --- run the proving ceremony ---
    let result = demo(3046, "wren", &home).expect("demo should succeed");
    assert!(result.contains("demo #3046"), "ok message: {}", result);
    assert!(result.contains("2/2 AC"), "ac count in message: {}", result);
    assert!(result.contains("verdict recorded"), "ok message names the verdict: {}", result);

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
        "demo.signal.completed",
        "demo.ready_for_review",
        "demo.verdict",
        "demo.completed",
    ] {
        assert!(
            witness.contains(&format!("\"event\":\"{}\"", evt)),
            "witness missing event {}; got:\n{}",
            evt,
            witness
        );
    }

    // (4) #3116: the ACT is OUT of demo — werk-build and werk-deploy must NEVER
    //     be invoked by the binary. Build/deploy/env-up are the PRIOR verbs in the
    //     flat sequence; demo only points at the already-running instance.
    let deploy_calls = fs::read_to_string(&deploy_log).unwrap_or_default();
    assert!(
        deploy_calls.trim().is_empty(),
        "demo must NOT invoke werk-deploy (act is out, #3116); got:\n{}",
        deploy_calls
    );
    let build_calls = fs::read_to_string(&build_log).unwrap_or_default();
    assert!(
        build_calls.trim().is_empty(),
        "demo must NOT invoke werk-build (act is out, #3116); got:\n{}",
        build_calls
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
    // #3116: the feedback gather is FIRE-AND-MOVE-ON — ONE nudge per non-builder
    // peer (silas, kade), NO re-nudge, NO [FEEDBACK STALL] banner. The old #3100
    // re-nudge+escalate spammed peers and Jeff on every demo; it's removed.
    let mcp_post_count = curl_calls.lines().filter(|l| l.contains("chorus_nudge_message")).count();
    assert_eq!(
        mcp_post_count, 2,
        "expected exactly 2 MCP nudges (one gather per non-builder peer, no re-nudge); got {} in:\n{}",
        mcp_post_count, curl_calls
    );
    assert!(
        !curl_calls.contains("FEEDBACK STALL"),
        "the per-peer [FEEDBACK STALL] Jeff banner must be gone (#3116 anti-spam); got:\n{}",
        curl_calls
    );

    // (5b) #3100 AC3: human-pause step — werk-demo announces "ready for review"
    //      to Bridge before exit so Jeff has a clear engagement point. The wait
    //      is external (agent reads Bridge + acts); binary stays terminating.
    assert!(
        witness.contains("\"event\":\"demo.ready_for_review\""),
        "demo must emit demo.ready_for_review as the human-pause announcement; got:\n{}",
        witness
    );

    // (6) #3116 — the gate chain is DELEGATED to the /demo skill layer; the binary
    //     no longer blocks on gate comments. demo must not refuse on the happy path.
    let card_story = fs::read_to_string(home.join("ops/logs/werk-demo.jsonl")).unwrap_or_default();
    assert!(
        !card_story.contains("\"event\":\"demo.refused\""),
        "demo must not refuse on the happy path; got:\n{}", card_story
    );
    // demo.verdict is recorded; until #3212 wires a real prover it's a labeled STUB
    // (prover=stub, auto=true) so Borg/accept can tell a placeholder from a real proof.
    assert!(
        card_story.contains("\"event\":\"demo.verdict\"")
            && card_story.contains("\"prover\":\"stub\"")
            && card_story.contains("\"auto\":true"),
        "demo must record demo.verdict labeled prover=stub + auto=true (#3116); got:\n{}", card_story
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

    // (8) #3116: the re-nudge/escalate spam path is GONE (fire-and-move-on). None of
    //     those events should appear — proving the anti-spam fix structurally.
    for gone in ["demo.feedback.unacked", "demo.renudge.sent", "demo.feedback.escalate"] {
        assert!(
            !witness.contains(&format!("\"event\":\"{}\"", gone)),
            "#3116 removed the re-nudge/escalate spam path — {} must not fire; got:\n{}", gone, witness
        );
    }

    // cleanup the trace file so re-runs are hermetic
    let _ = fs::remove_file(trace_path);
}
