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
use werk_demo::{demo, refuse_spine_args};

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

// #3281 — agent-side blocks become COUNTABLE spine events. A demo refusal must
// emit a `.refused` event (the suffix the pain board's PAIN_EVENT_SUFFIXES reads)
// carrying role + a reason= field, so the rollup groups by role·event·reason.
// This pins the wire shape of the refusal emit (the pure arg-builder behind
// emit_spine_reason); the live emit + pain-board count is the AC3 proof.
#[test]
fn refuse_spine_args_names_event_role_and_reason() {
    let a = refuse_spine_args("demo.refused", "silas", 3281, "tr-abc", "wrong-status");
    assert_eq!(a[0], "demo.refused", "event must be the .refused name the pain board reads");
    assert!(a[0].ends_with(".refused"), "must carry a PAIN_EVENT_SUFFIXES suffix so the rollup counts it");
    assert_eq!(a[1], "silas", "role present (AC1: role + reason)");
    assert!(a.contains(&"card=3281".to_string()), "card id present");
    assert!(a.contains(&"trace=tr-abc".to_string()), "trace propagated");
    assert!(a.contains(&"reason=wrong-status".to_string()), "reason present so rollup groups by reason");
}

#[test]
fn refuse_spine_args_distinguishes_reasons() {
    let no_ac = refuse_spine_args("demo.refused", "kade", 42, "t", "no-ac");
    assert!(no_ac.contains(&"reason=no-ac".to_string()));
    assert!(!no_ac.contains(&"reason=wrong-status".to_string()), "the two refusal reasons must be distinguishable in the data");
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
    // #3263: the variant-reachability check curls a live variant port; no variant
    // is up in the test, so skip it here (the check is live in the real pipeline).
    std::env::set_var("CHORUS_DEMO_SKIP_VARIANT_CHECK", "1");
    // #3263: the demo runs the card's tests in its werk; there's no real werk here,
    // so skip the run (the fixture seeds demo.test_result directly).
    std::env::set_var("CHORUS_DEMO_SKIP_TEST_RUN", "1");
    // #3443 AC1 — the suite tests gate ENFORCEMENT by seeding gate results; the
    // new self-RUN path (headless claude -p) is unit-tested separately. Skip it
    // here so e2e never spawns a real claude.
    std::env::set_var("CHORUS_DEMO_SKIP_GATE_RUN", "1");
    // #3443 AC2 — the suite seeds gather replies directly; skip the real send so
    // e2e never fires a live MCP nudge (keeps the happy path's zero-nudge contract).
    std::env::set_var("CHORUS_DEMO_SKIP_GATHER_SEND", "1");
    std::env::set_var("CHORUS_DEMO_ROUND", "e2e-r1");

    // #3284 (AC6): with NO gates recorded, the demo REFUSES to present (invariant gate
    // execution) — exit 1, typed gates-missing — BEFORE any announce. This is the
    // loud-stop that replaces #3279's silent "(none run — optional)" present.
    fs::create_dir_all(home.join("ops/logs")).unwrap();
    let ungated = demo(3046, "wren", &home).expect("demo() returns Ok even when it refuses");
    assert_eq!(ungated.exit, 1, "un-gated demo must REFUSE to present (AC6): {}", ungated.message);
    assert!(ungated.message.contains("gates not run"), "typed gates-missing refusal: {}", ungated.message);
    // #3318: under act/CI (ACT set) the same un-gated demo SKIPS enforcement and presents
    // — the pipeline-unbreak. Verified live (a chorus_werk Half A reaches present), not
    // here, because demo() fires the feedback gather and would pollute the nudge count.

    // #3237: the blocking demo step (a) refuses unless all 5 gates recorded a
    // demo.gate.result, and (b) BLOCKS until Jeff records a demo.decision. Seed
    // both — the 5 gates + a "go" — so the happy path reflects gates-ran +
    // accepted. (gates_missing / read_decision unit tests cover the refusal and
    // the no-go/more/timeout branches directly.)
    let mut gate_seed = String::new();
    for g in ["product", "code", "quality", "arch", "ops"] {
        gate_seed.push_str(&format!(
            "{{\"ts\":1,\"event\":\"demo.gate.result\",\"role\":\"wren\",\"card_id\":3046,\"trace_id\":\"seed\",\"gate\":\"{}\",\"round\":\"e2e-r1\",\"result\":\"pass\",\"findings\":\"{} reviewed, no concerns\"}}\n",
            g, g
        ));
    }
    // #3263: a recorded demo.test_result feeds the decision surface ("tests: pass").
    // The pipeline's test step records it via `werk-demo test-result`; demo() here is
    // called directly, so seed it. (#3279: the demo PRESENTS this on the surface and
    // EXITS — it no longer blocks for a demo.decision, so no "go" seed is needed; the
    // go runs Half B / werk-land separately.)
    gate_seed.push_str("{\"ts\":1,\"event\":\"demo.test_result\",\"role\":\"wren\",\"card_id\":3046,\"trace_id\":\"seed\",\"result\":\"pass\"}\n");
    // #3352 — the announce now ALSO requires both peer gathers REPLIED (the full
    // invariant: gates -> gathers -> announce -> go). Seed the replies the way the
    // live flow records them via `werk-demo gather <card> <peer> replied`.
    gate_seed.push_str("{\"ts\":1,\"event\":\"demo.gather.replied\",\"role\":\"wren\",\"card_id\":3046,\"trace_id\":\"seed\",\"peer\":\"silas\",\"round\":\"e2e-r1\",\"note\":\"ack\"}\n");
    gate_seed.push_str("{\"ts\":1,\"event\":\"demo.gather.replied\",\"role\":\"wren\",\"card_id\":3046,\"trace_id\":\"seed\",\"peer\":\"kade\",\"round\":\"e2e-r1\",\"note\":\"ack\"}\n");
    fs::write(home.join("ops/logs/werk-demo.jsonl"), &gate_seed).unwrap();

    // --- run the proving ceremony ---
    let result = demo(3046, "wren", &home).expect("demo should succeed");
    // #3279 — present-and-exit: the demo PRESENTS and returns exit 0 WITHOUT blocking
    // for a decision. The go is no longer consumed here (it runs Half B / werk-land).
    assert_eq!(result.exit, 0, "present-and-exit → exit 0 (Half A done); got {}", result.exit);
    // #3284 — the announce IS Jeff's 5-step demo contract, RETURNED as the message so
    // the agent pastes it into its end-of-turn reply (auto/focus mode: Jeff never sees
    // a Bridge post). Assert all five steps are present in what Jeff will read:
    assert!(result.message.contains("#3046"), "announce names the card: {}", result.message);
    assert!(result.message.contains("AC 2/2"), "ac count in the announce: {}", result.message);
    assert!(result.message.contains("gates:"), "(1) gates required + shown: {}", result.message);
    assert!(
        result.message.contains("feedback:") && result.message.contains("reviewed, no concerns"),
        "(2) feedback required — each gate's findings shown: {}",
        result.message
    );
    assert!(
        result.message.contains("Ask me anything") && result.message.contains("TEST"),
        "(4) announce invites your questions + a test: {}",
        result.message
    );
    assert!(result.message.contains("go / no"), "(5) ends in the go/no decision: {}", result.message);

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
        "demo.presented",
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

    // (5a) #3305 — this scenario seeds BOTH peers' demo.gather.replied for the
    //      round, so the announce fires with ZERO gather nudges: an acked peer is
    //      never re-asked, however many times the demo re-presents. (The old
    //      assertion here pinned exactly the re-fire bug — 2 unconditional nudges
    //      to already-replied peers; 8 live sightings on 2026-06-12 alone.)
    //      The MCP wire shape (tools/call + chorus_nudge_message) is pinned by
    //      the mcp_nudge_body unit test; the legacy-endpoint ban stays here.
    let curl_calls = fs::read_to_string(&curl_log).unwrap_or_default();
    assert!(
        !curl_calls.contains("http://localhost:3340/api/chorus/nudge"),
        "must NOT POST to the legacy 404 :3340/api/chorus/nudge; got:\n{}",
        curl_calls
    );
    let mcp_post_count = curl_calls.lines().filter(|l| l.contains("chorus_nudge_message")).count();
    assert_eq!(
        mcp_post_count, 0,
        "both peers already replied this round — re-presenting must re-fire ZERO gather nudges (#3305); got {} in:\n{}",
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
    // #3279 — present-and-exit: the demo records demo.presented (the variant is up,
    // here's the decision surface) and EXITS. It does NOT record demo.verdict — the
    // go is no longer made here; it's made in Half B (werk-land), which records the
    // verdict. Borg/accept read the verdict from the land run, not the present run.
    assert!(
        card_story.contains("\"event\":\"demo.presented\""),
        "demo must record demo.presented (#3279 present-and-exit); got:\n{}", card_story
    );
    assert!(
        !card_story.contains("\"event\":\"demo.verdict\""),
        "#3279: the present run must NOT record a verdict — the go (Half B) does; got:\n{}", card_story
    );

    // (7) #3279: the demo NO LONGER BLOCKS for a decision. The held block was what
    //     dropped the synchronous MCP call on long waits (#3277). Structural proof of
    //     present-and-exit: none of the blocking-decision events fire.
    for gone in [
        "demo.awaiting_decision",
        "demo.awaiting_decision.heartbeat",
        "demo.decision.honored",
        "demo.awaiting_comment",
    ] {
        assert!(
            !witness.contains(&format!("\"event\":\"{}\"", gone)),
            "#3279 present-and-exit removed the blocking decision step — {} must not fire; got:\n{}",
            gone, witness
        );
    }

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

// #3324 AUDIT — the #3237 read_decision/Decision tests were DELETED here: #3279
// retired the blocking-decision step (demo presents-and-exits; werk-land's
// shared-trace records the verdict), leaving read_decision/Decision::exit_code
// with zero production callers. The tests pinned retired behavior. Removing the
// orphaned lib code itself is a fill card (needs blast-radius pass, #3148).

// #3352 — the OTHER half of the invariant, end-to-end: gates recorded but peer
// gathers NOT replied → the demo stands by (typed gathers-pending), no announce.
// Pairs with the happy path above (gates + both replies → announce fires).
#[test]
fn e2e_gates_without_gathers_stands_by() {
    let home = tmp("home-gathers");
    fs::create_dir_all(home.join("platform/scripts")).unwrap();
    write_exec(
        &home.join("platform/scripts/cards"),
        r#"#!/bin/sh
if [ "$1" = "view" ] && [ "$3" = "--json" ]; then
  printf '{ "status": "WIP" }\n'
  exit 0
fi
if [ "$1" = "view" ]; then
  cat <<'CARDEOF'
#3052 gathers test card
  Status: WIP
  Owner: wren
  Desc:
    ## Acceptance Criteria
    - [x] done
  Domains: domain:chorus, type:fix
CARDEOF
  exit 0
fi
exit 0
"#,
    );
    write_exec(&home.join("platform/scripts/smoke-check.sh"), "#!/bin/sh\nexit 0\n");
    write_exec(&home.join("platform/scripts/chorus-log"), "#!/bin/sh\nexit 0\n");
    std::env::set_var("CHORUS_DEMO_COMMENT_WINDOW_SECS", "0");
    std::env::set_var("CHORUS_DEMO_ACK_WINDOW_SECS", "0");
    std::env::set_var("CHORUS_DEMO_GATE_WAIT_SECS", "0");
    std::env::set_var("CHORUS_DEMO_SKIP_VARIANT_CHECK", "1");
    std::env::set_var("CHORUS_DEMO_SKIP_TEST_RUN", "1");
    // #3443 AC1 — the suite tests gate ENFORCEMENT by seeding gate results; the
    // new self-RUN path (headless claude -p) is unit-tested separately. Skip it
    // here so e2e never spawns a real claude.
    std::env::set_var("CHORUS_DEMO_SKIP_GATE_RUN", "1");
    // #3443 AC2 — the suite seeds gather replies directly; skip the real send so
    // e2e never fires a live MCP nudge (keeps the happy path's zero-nudge contract).
    std::env::set_var("CHORUS_DEMO_SKIP_GATHER_SEND", "1");
    std::env::set_var("CHORUS_DEMO_ROUND", "e2e-r1");
    std::env::remove_var("ACT");

    fs::create_dir_all(home.join("ops/logs")).unwrap();
    let mut seed = String::new();
    for g in ["product", "code", "quality", "arch", "ops"] {
        seed.push_str(&format!(
            "{{\"ts\":1,\"event\":\"demo.gate.result\",\"role\":\"wren\",\"card_id\":3052,\"trace_id\":\"seed\",\"gate\":\"{}\",\"round\":\"e2e-r1\",\"result\":\"pass\",\"findings\":\"ok\"}}\n",
            g
        ));
    }
    seed.push_str("{\"ts\":1,\"event\":\"demo.test_result\",\"role\":\"wren\",\"card_id\":3052,\"trace_id\":\"seed\",\"result\":\"pass\"}\n");
    // one peer replied, one did not — still standby (the team's feedback, not a sample)
    seed.push_str("{\"ts\":1,\"event\":\"demo.gather.replied\",\"role\":\"wren\",\"card_id\":3052,\"trace_id\":\"seed\",\"peer\":\"kade\",\"round\":\"e2e-r1\",\"note\":\"ack\"}\n");
    fs::write(home.join("ops/logs/werk-demo.jsonl"), &seed).unwrap();

    let out = demo(3052, "wren", &home).expect("standby returns Ok");
    assert_eq!(out.exit, 0, "standby is a clean exit, not a failure: {}", out.message);
    assert!(out.message.contains("gathers-pending"), "typed standby reason: {}", out.message);
    assert!(out.message.contains("silas"), "names the missing peer: {}", out.message);
    let witness = fs::read_to_string(home.join("ops/logs/werk-demo.jsonl")).unwrap();
    assert!(
        witness.contains("\"reason\":\"gathers-pending\"") && witness.contains("\"gathers_missing\":\"silas\""),
        "standby witnessed with the missing peer named:\n{}",
        witness
    );
}
