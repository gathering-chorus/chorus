// @test-type: integration
// @domain: builds
//! End-to-end (#3116, #4502): PATH-shim every external CLI (cards / werk-build /
//! werk-deploy / curl / chorus-log / gh), prove the demo steps run as dagu calls
//! them — validate (AC preflight + evidence + trace) → the gates and peer
//! reviews (seeded here; their runners are tested in steps_4502.rs) → announce
//! — assert the jsonl witness records the expected events, AND assert the demo
//! NEVER builds or deploys (the act is out). One env-mutating test fn (so PATH /
//! env can't race).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use werk_demo::{announce_step, refuse_spine_args, validate_step};

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
fn e2e_demo_steps() {
    // --- PATH shims ---
    let bin = tmp("bin");
    let cards_log = tmp("cardslog").join("calls");
    // #3116: werk-build / werk-deploy are shimmed to LOG if called — the demo
    // must NEVER call them (the act is out). Both logs must stay empty.
    let build_log = tmp("buildlog").join("calls");
    write_exec(&bin.join("werk-build"), &format!("#!/bin/sh\necho \"$@\" >> \"{}\"\nexit 0\n", build_log.display()));
    let deploy_log = tmp("deploylog").join("calls");
    write_exec(&bin.join("werk-deploy"), &format!("#!/bin/sh\necho \"$@\" >> \"{}\"\nexit 0\n", deploy_log.display()));
    let curl_log = tmp("curllog").join("calls");
    write_exec(&bin.join("curl"), &format!("#!/bin/sh\necho \"$@\" >> \"{}\"\nexit 0\n", curl_log.display()));
    write_exec(&bin.join("gh"), "#!/bin/sh\nexit 0\n");

    // werk-demo resolves `cards` ABSOLUTELY at home/platform/scripts/cards (#3183).
    let home = tmp("home");
    fs::create_dir_all(home.join("platform/scripts")).unwrap();
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
  cat <<'CARDEOF'
#3046 demo-v2 test card
  Status: WIP
  Owner: wren
  Desc:
    ## Acceptance Criteria
    - [x] one ported
    - [x] two ported
  Domains: domain:chorus, type:fix
CARDEOF
  exit 0
fi
exit 0
"#,
            cards_log = cards_log.display()
        ),
    );
    write_exec(&home.join("platform/scripts/chorus-log"), "#!/bin/sh\nexit 0\n");

    std::env::set_var("PATH", format!("{}:{}", bin.display(), std::env::var("PATH").unwrap_or_default()));
    std::env::set_var("CHORUS_TRACE_ID", "e2e-trace-abc123");
    // #3995 — never speak to live services from a test: dead-port both HTTP seams.
    // #4502 — the shim LOGS each send, so the test counts the round's asks
    let shim_log = tmp("shimlog").join("calls");
    write_exec(&bin.join("chorus-hook-shim-stub"), &format!("#!/bin/sh\necho \"$@\" >> \"{}\"\nexit 0\n", shim_log.display()));
    std::env::set_var("CHORUS_HOOK_SHIM", bin.join("chorus-hook-shim-stub"));
    // no variant is up in the test; the reachability check is live in the pipeline
    std::env::set_var("CHORUS_DEMO_SKIP_VARIANT_CHECK", "1");
    // no real werk here; the fixture seeds demo.test_result directly
    std::env::set_var("CHORUS_DEMO_SKIP_TEST_RUN", "1");
    std::env::set_var("CHORUS_DEMO_ROUND", "e2e-r1");
    std::env::remove_var("CHORUS_WERK_BASE");
    fs::create_dir_all(home.join("ops/logs")).unwrap();
    let witness_path = home.join("ops/logs/werk-demo.jsonl");

    // --- step: validate ---
    let v = validate_step(3046, "wren", &home).expect("validate passes a WIP card with AC");
    assert_eq!(v.exit, 0, "validate: {}", v.message);
    assert!(v.message.contains("AC 2/2"), "validate reports the AC count: {}", v.message);

    // --- step: announce, NEGATIVE PROOF 1: nothing recorded → refused, every step named ---
    let none = announce_step(3046, "wren", &home).expect("announce returns Ok when it refuses");
    assert_eq!(none.exit, 1, "announce with no gates/reviews must refuse: {}", none.message);
    for step in ["gate-product", "gate-code", "gate-quality", "gate-arch", "gate-ops", "review-silas", "review-kade", "review-abby-normal"] {
        assert!(none.message.contains(step), "refusal names {}: {}", step, none.message);
    }

    // seed the 5 gates + the tests verdict + two of the three peer reviews
    let mut seed = fs::read_to_string(&witness_path).unwrap_or_default();
    for g in ["product", "code", "quality", "arch", "ops"] {
        seed.push_str(&format!(
            "{{\"ts\":1,\"event\":\"demo.gate.result\",\"role\":\"wren\",\"card_id\":3046,\"trace_id\":\"seed\",\"gate\":\"{}\",\"round\":\"e2e-r1\",\"result\":\"pass\",\"findings\":\"{} reviewed, no concerns\"}}\n",
            g, g
        ));
    }
    seed.push_str("{\"ts\":1,\"event\":\"demo.test_result\",\"role\":\"wren\",\"card_id\":3046,\"trace_id\":\"seed\",\"result\":\"pass\"}\n");
    seed.push_str("{\"ts\":1,\"event\":\"demo.gather.replied\",\"role\":\"wren\",\"card_id\":3046,\"trace_id\":\"seed\",\"peer\":\"kade\",\"round\":\"e2e-r1\",\"verdict\":\"pass\",\"note\":\"ack\"}\n");
    seed.push_str("{\"ts\":1,\"event\":\"demo.gather.replied\",\"role\":\"wren\",\"card_id\":3046,\"trace_id\":\"seed\",\"peer\":\"abby-normal\",\"round\":\"e2e-r1\",\"verdict\":\"pass\",\"note\":\"ack\"}\n");
    fs::write(&witness_path, &seed).unwrap();

    // --- NEGATIVE PROOF 2: one review missing → refused, naming only that review ---
    let one = announce_step(3046, "wren", &home).expect("announce returns Ok when it refuses");
    assert_eq!(one.exit, 1, "announce with a review missing must refuse: {}", one.message);
    assert!(one.message.contains("review-silas") && !one.message.contains("review-kade") && !one.message.contains("review-abby-normal") && !one.message.contains("gate-"),
            "names only the missing review: {}", one.message);
    let w = fs::read_to_string(&witness_path).unwrap();
    assert!(!w.contains("\"event\":\"demo.presented\""), "a refused announce presents nothing:\n{}", w);

    // an "error" review is no review: it still refuses
    let mut err = fs::read_to_string(&witness_path).unwrap();
    err.push_str("{\"ts\":1,\"event\":\"demo.gather.replied\",\"role\":\"wren\",\"card_id\":3046,\"trace_id\":\"seed\",\"peer\":\"silas\",\"round\":\"e2e-r1\",\"verdict\":\"error\",\"note\":\"claude exit 1\"}\n");
    fs::write(&witness_path, &err).unwrap();
    let errored = announce_step(3046, "wren", &home).unwrap();
    assert_eq!(errored.exit, 1, "an errored review is missing: {}", errored.message);

    // --- happy path: the last review lands → announce presents, exit 0 ---
    let mut both = fs::read_to_string(&witness_path).unwrap();
    both.push_str("{\"ts\":2,\"event\":\"demo.gather.replied\",\"role\":\"wren\",\"card_id\":3046,\"trace_id\":\"seed\",\"peer\":\"silas\",\"round\":\"e2e-r1\",\"verdict\":\"concerns\",\"note\":\"watch the land\"}\n");
    fs::write(&witness_path, &both).unwrap();
    let result = announce_step(3046, "wren", &home).expect("announce should succeed");
    assert_eq!(result.exit, 0, "#4502 announce step succeeds as a dagu step: {}", result.message);
    // #3511 — the four things Jeff named: gates, team review, card + AC, a claim
    assert!(result.message.contains("#3046"), "(3) names the card: {}", result.message);
    assert!(result.message.contains("AC 2/2"), "(3) AC count: {}", result.message);
    assert!(result.message.contains("AC:"), "(3) the AC items are listed: {}", result.message);
    assert!(result.message.contains("gates:"), "(1) gate status shown: {}", result.message);
    assert!(result.message.contains("feedback:") && result.message.contains("reviewed, no concerns"),
            "per-gate findings shown: {}", result.message);
    assert!(result.message.contains("team review:"), "(2) role feedback summarized: {}", result.message);
    assert!(result.message.contains("🔬"), "(4) a claim of how to prove it works: {}", result.message);
    assert!(result.message.contains("werk-demo go 3046"), "(5) ends in Jeff's go path: {}", result.message);

    // --- side effects ---
    let calls = fs::read_to_string(&cards_log).unwrap_or_default();
    assert!(calls.contains("comment 3046 demo:preflight-pass"), "validate posts the evidence comment; calls:\n{}", calls);
    let trace_path = "/tmp/demo-trace-3046.txt";
    assert_eq!(fs::read_to_string(trace_path).unwrap_or_default().trim(), "e2e-trace-abc123", "trace file holds CHORUS_TRACE_ID");
    let witness = fs::read_to_string(&witness_path).unwrap_or_default();
    for evt in ["demo.started", "demo.preflight.passed", "demo.signal.completed", "demo.ready_for_review", "demo.presented"] {
        assert!(witness.contains(&format!("\"event\":\"{}\"", evt)), "witness missing event {}; got:\n{}", evt, witness);
    }
    assert!(fs::read_to_string(&deploy_log).unwrap_or_default().trim().is_empty(), "demo must NOT invoke werk-deploy (#3116)");
    assert!(fs::read_to_string(&build_log).unwrap_or_default().trim().is_empty(), "demo must NOT invoke werk-build (#3116)");
    // #4502 — nobody is asked to reply: no gather nudge, no gather wait, ever
    let curl_calls = fs::read_to_string(&curl_log).unwrap_or_default();
    assert!(!curl_calls.contains("clearing-nudge"), "no gather nudges: {}", curl_calls);
    for gone in ["demo.gather.sent", "demo.gather.block", "demo.gather.timeout", "demo.prework.standby", "demo.verdict"] {
        assert!(!witness.contains(&format!("\"event\":\"{}\"", gone)), "{} must not fire; got:\n{}", gone, witness);
    }
    // #4502 — ONE message per round (Jeff 11:33 "really noisy and repetitive";
    // #3971 folded): the round's ask went out once, and no room posts at all.
    let asks = |p: &PathBuf| fs::read_to_string(p).unwrap_or_default().lines().filter(|l| l.contains("Demo ready — #3046")).count();
    assert_eq!(asks(&shim_log), 1, "one ask for the round: {}", fs::read_to_string(&shim_log).unwrap_or_default());
    let sent = fs::read_to_string(&shim_log).unwrap_or_default();
    assert!(sent.contains("gates ✓") && sent.contains("reviews 3/3") && sent.contains("AC 2/2") && sent.contains("go / no / more"),
            "the one ask carries the tally and the go: {}", sent);
    assert!(!curl_calls.contains("/api/message"), "no room posts: {}", curl_calls);
    // NEGATIVE PROOF 1: a second announce for the same round sends nothing
    let again = announce_step(3046, "wren", &home).unwrap();
    assert_eq!(again.exit, 0, "{}", again.message);
    assert_eq!(asks(&shim_log), 1, "a re-present of the same round asks 0 more times");
    // NEGATIVE PROOF 2: after Jeff's go, nothing is sent even for a fresh announce
    let mut w2 = fs::read_to_string(&witness_path).unwrap();
    w2 = w2.lines().filter(|l| !l.contains("\"event\":\"demo.announced\"")).collect::<Vec<_>>().join("\n") + "\n";
    w2.push_str("{\"ts\":3,\"event\":\"demo.go\",\"role\":\"wren\",\"card_id\":3046,\"trace_id\":\"t\",\"round\":\"e2e-r1\"}\n");
    fs::write(&witness_path, &w2).unwrap();
    let after_go = announce_step(3046, "wren", &home).unwrap();
    assert_eq!(after_go.exit, 0, "{}", after_go.message);
    assert_eq!(asks(&shim_log), 1, "after the go, no ask is sent");
    let _ = fs::remove_file(trace_path);
}
