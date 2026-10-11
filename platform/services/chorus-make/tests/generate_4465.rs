//! #4465 — chorus-make turns a pipeline's rows into the werk v2 workflow, the
//! same rows always give the same bytes, and a hand edit is caught.
use chorus_make::{check, drift, generate, generate_land, parse_rows};

fn fixture(name: &str) -> String {
    // run time, not env!(): the werk-test 4030 guard refuses a compile-time path
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{dir}/tests/{name}")).expect("fixture")
}

fn cicd() -> Vec<chorus_make::Row> {
    parse_rows(&fixture("fixtures/cicd-rows.tsv")).expect("fixture rows parse")
}

#[test]
fn the_cicd_rows_generate_the_golden_workflow() {
    let out = generate("cicd", &cicd()).expect("generates");
    assert_eq!(out, fixture("fixtures/cicd-golden.yaml"));
    let land = generate_land("cicd", &cicd()).expect("generates the land");
    assert_eq!(land, fixture("fixtures/cicd-land-golden.yaml"));
}

#[test]
fn the_same_rows_give_byte_identical_output_in_any_row_order() {
    let mut shuffled = cicd();
    shuffled.reverse();
    assert_eq!(generate("cicd", &cicd()).unwrap(), generate("cicd", &shuffled).unwrap());
}

#[test]
fn steps_run_in_order_and_the_go_waits_between_demo_and_land() {
    let out = generate("cicd", &cicd()).unwrap();
    let ids: Vec<&str> = out.lines().filter_map(|l| l.strip_prefix("  - name: ")).collect();
    assert_eq!(ids.first(), Some(&"skill-werk-commit"));
    let go = ids.iter().position(|i| *i == "skill-werk-go").expect("the go is a step");
    assert_eq!(ids[go - 1], "skill-werk-demo");
    // #4494 — after the go the card enqueues its land; the land steps are in the land half
    assert_eq!(&ids[go + 1..], &["cicd-land"]);
    let land = generate_land("cicd", &cicd()).unwrap();
    let land_ids: Vec<&str> = land.lines().filter_map(|l| l.strip_prefix("  - name: ")).collect();
    assert_eq!(land_ids.first(), Some(&"skill-werk-merge"));
    assert_eq!(land_ids.last(), Some(&"skill-werk-accept"));
    assert!(out.contains("  - name: skill-werk-go\n    description: \"Jeff's go\"\n    depends: [skill-werk-demo]\n    id: skill_werk_go\n    action: human.task\n"));
    for half in [&out, &land] {
        assert!(half.starts_with(chorus_make::HEADER));
        // #4474 run 1: dagu does not pass CHORUS_HOME through; every verb refused
        for v in ["CHORUS_HOME: ${CHORUS_HOME}", "CHORUS_WERK_BASE: ${CHORUS_WERK_BASE}"] {
            assert!(half.contains(&format!("  - {v}\n")), "workflow env is missing {v}");
        }
        // Jeff 21:06: DEPLOY_ROLE is identity; a run param must never assert it
        assert!(!half.contains("DEPLOY_ROLE") && !half.contains("CHORUS_ROLE"), "the workflow claims an identity");
    }
}

// #4494 — cards run on the werk queue, several at once; lands on werk-land, one
// at a time. The land is ENQUEUED by name (main's copy), never a called sub-run:
// on a scratch dagu a sub-run ignored its queue and two lands overlapped.
#[test]
fn cards_and_lands_run_on_their_own_queues() {
    let card = generate("cicd", &cicd()).unwrap();
    let land = generate_land("cicd", &cicd()).unwrap();
    assert!(card.contains("\nqueue: werk\n"), "{card}");
    assert!(land.contains("\nqueue: werk-land\n"), "{land}");
    let enq = card.split("  - name: ").find(|s| s.starts_with("cicd-land\n")).expect("the card enqueues its land");
    assert!(enq.contains("    action: dag.enqueue\n    with:\n      dag: cicd-land\n"), "{enq}");
    // NEGATIVE PROOF: never a call (dag.run) and never a file path, which dagu refuses
    assert!(!card.contains("dag.run") && !enq.contains(".yaml"), "{enq}");
}

#[test]
fn every_land_step_runs_after_the_go_and_none_before_it() {
    let rows = cicd();
    let card = generate("cicd", &rows).unwrap();
    let land = generate_land("cicd", &rows).unwrap();
    let card_names: Vec<&str> = card.lines().filter_map(|l| l.strip_prefix("  - name: ")).collect();
    let land_names: Vec<&str> = land.lines().filter_map(|l| l.strip_prefix("  - name: ")).collect();
    for n in &land_names {
        assert!(!card_names.contains(n), "{n} runs in both halves");
    }
    // every skill row is a step in exactly one half; the enqueue step is the one extra
    assert_eq!(card_names.len() + land_names.len(), rows.len() + 1);
    // NEGATIVE PROOF: rows with no go have no land and are refused, never half-written
    let no_go: Vec<_> = rows.iter().filter(|r| r.executor != "human").cloned().collect();
    assert!(generate("cicd", &no_go).is_err() && generate_land("cicd", &no_go).is_err());
}

#[test]
fn negative_proof_a_hand_edited_step_goes_red_in_the_drift_check() {
    let expected = generate("cicd", &cicd()).unwrap();
    assert!(drift(&expected, &expected).is_ok(), "an untouched file matches");
    let edited = expected.replace("werk-test ${CARD} ${ROLE}", "werk-test ${CARD} ${ROLE} --type=unit");
    assert_ne!(edited, expected, "the edit must change the file, or this proves nothing");
    let err = drift(&expected, &edited).expect_err("a hand edit must be caught");
    assert!(err.contains("--type=unit"), "the failure names what changed: {err}");
    let truncated: String = expected.lines().take(10).map(|l| format!("{l}\n")).collect();
    assert!(drift(&expected, &truncated).is_err(), "a cut-short file is drift too");
}

#[test]
fn negative_proof_a_deterministic_skill_with_no_binary_is_refused() {
    // The door does not run sh:sparql (athena-model lib.rs:4453), so this rule
    // lives here; without this test it would be a rule nothing enforces.
    let mut rows = cicd();
    rows[3].implemented_by.clear();
    let errs = generate("cicd", &rows).expect_err("refused, not written");
    assert!(errs.iter().any(|e| e.contains("skill-werk-test") && e.contains("needs implementedBy")), "{errs:?}");
    assert!(check(&cicd()).is_empty(), "the real rows pass, so the refusal is about the edit");
}

#[test]
fn negative_proof_two_skills_at_one_order_and_no_rows_are_refused() {
    let mut rows = cicd();
    rows[1].skill_order = 1; // push now collides with commit inside step commit
    assert!(check(&rows).iter().any(|e| e.contains("two skills hold order 1 in step commit")));
    assert!(generate("cicd", &[]).is_err(), "no rows is refused, never an empty workflow");
}

// Jeff via Silas 2026-10-09 20:34: the names in the workflow are the model's.
#[test]
fn every_step_is_named_by_its_skill_row_and_nothing_is_minted() {
    let rows = cicd();
    let skills: Vec<&str> = rows.iter().map(|r| r.skill.as_str()).collect();
    for half in [generate("cicd", &rows).unwrap(), generate_land("cicd", &rows).unwrap()] {
        for n in half.lines().filter_map(|l| l.strip_prefix("  - name: ")) {
            // the one step not a skill row is the card's land enqueue, named for the land DAG
            assert!(skills.contains(&n) || n == "cicd-land", "{n} is not a Skill row name");
        }
    }
    // the one id dagu demands (a human.task) is the row name with '_' for '-'
    let out = generate("cicd", &rows).unwrap();
    let ids: Vec<&str> = out.lines().filter_map(|l| l.strip_prefix("    id: ")).collect();
    assert_eq!(ids, vec!["skill_werk_go"]);
    assert!(out.contains("    description: \"werk-deploy --target werk\"\n"), "description is the row's label");
}

// #4474 run 6: werk-demo presented, exited 2 (held for the go) and dagu
// failed the run before the go step it was waiting for.
#[test]
fn only_the_step_before_the_go_treats_presented_as_success() {
    let out = generate("cicd", &cicd()).unwrap();
    let held = "    continue_on:\n      exit_code: [2]\n      mark_success: true\n";
    assert_eq!(out.matches(held).count(), 1, "exactly one step may hold");
    let demo = out.split("  - name: ").find(|s| s.starts_with("skill-werk-demo\n")).unwrap();
    assert!(demo.contains(held), "the demo step holds for the go");
}

#[test]
fn the_go_names_the_accepter_and_only_the_land_steps_read_it() {
    // #4474 run 9: werk-merge refused "no-approval" because no step set $ACCEPTER
    let out = generate("cicd", &cicd()).unwrap();
    let go = out.split("  - name: ").find(|s| s.starts_with("skill-werk-go\n")).unwrap();
    assert!(go.contains("        required: [accepter, decision]\n"), "the go asks who and go-or-no: {go}");
    assert!(go.contains("enum: [jeff, wren, kade, silas]"), "{go}");
    // #4494 — the go's accepter rides into the land as its ACCEPTER param (a
    // dagu param is env in every step: measured, a land step read envacc=jeff)
    let enq = out.split("  - name: ").find(|s| s.starts_with("cicd-land\n")).unwrap();
    assert!(enq.contains("ACCEPTER=${skill_werk_go.outputs.accepter}"), "{enq}");
    let land = generate_land("cicd", &cicd()).unwrap();
    assert!(land.contains("params:\n  - CARD: \"\"\n  - ROLE: \"\"\n  - ACCEPTER: \"\"\n"), "{land}");
    // negative proof: no step before the go may claim an accepter
    let before = out.split("  - name: ").skip(1).take_while(|s| !s.starts_with("skill-werk-go\n")).filter(|s| s.contains("ACCEPTER")).count();
    assert_eq!(before, 0, "a step before the go reads an accepter nobody gave");
}

#[test]
fn a_no_at_the_go_skips_the_land_and_ends_the_run() {
    // #4494 — a run waiting at the go could not be stopped, dequeued or
    // deleted (dagu 2.18, measured 2026-10-10 22:2x). "no" is the way out:
    // the enqueue only runs when the decision is go (measured in a scratch
    // dagu 23:40: decision=no → land skipped, run ended; go → landed).
    let out = generate("cicd", &cicd()).unwrap();
    let go = out.split("  - name: ").find(|s| s.starts_with("skill-werk-go\n")).unwrap();
    assert!(go.contains("          decision:\n            type: string\n            enum: [go, no]\n"), "{go}");
    let enq = out.split("  - name: ").find(|s| s.starts_with("cicd-land\n")).unwrap();
    assert!(enq.contains("    preconditions:\n      - condition: \"${skill_werk_go.outputs.decision}\"\n        expected: \"go\"\n"), "{enq}");
    // NEGATIVE PROOF: the enqueue is the only step after the go, so nothing
    // else can carry a no into a land
    let after: Vec<&str> = out.split("  - name: ").skip_while(|s| !s.starts_with("skill-werk-go\n")).skip(1).collect();
    assert_eq!(after.len(), 1, "steps after the go: {after:?}");
}

#[test]
fn the_cards_own_verbs_come_first_on_the_path() {
    // #4474 run 12: the scheduler's PATH found the installed werk-demo, not the card's
    let out = generate("cicd", &cicd()).unwrap();
    let base = out.find("  - CHORUS_WERK_BASE: ").expect("base");
    let path = out.find("  - PATH: ${CHORUS_WERK_BASE}/${ROLE}-bin:${PATH}\n").expect("the role slot leads PATH");
    assert!(base < path, "PATH reads CHORUS_WERK_BASE, so it must come after it");
}

#[test]
fn a_failed_run_nudges_the_builder() {
    // #4474 run 15 failed at 09:25; nobody knew until 10:06
    let out = generate("cicd", &cicd()).unwrap();
    let h = out.find("handler_on:\n  failure:\n    command: '${CHORUS_HOME}/platform/scripts/ops-nudge ${ROLE} ").expect("a failure handler");
    assert!(h < out.find("steps:\n").unwrap(), "handler_on is DAG-level, before the steps");
    // negative proof: the handler is on failure only, never on every exit or success
    assert!(!out.contains("  success:") && !out.contains("  exit:"), "{out}");
}

#[test]
fn negative_proof_one_skill_called_twice_is_refused() {
    let mut rows = cicd();
    rows[4].skill = rows[3].skill.clone(); // review now calls skill-werk-test again
    assert!(check(&rows).iter().any(|e| e.contains("skill-werk-test: called twice")), "{:?}", check(&rows));
}

#[test]
fn a_bad_row_is_refused_by_line_never_skipped() {
    let errs = parse_rows("step\tstepOrder\tskillOrder\tskill\tlabel\texecutor\timplementedBy\tmode\ncommit\tone\t1\ts\tl\tdeterministic\tb\t\n")
        .expect_err("a non-number order is refused");
    assert_eq!(errs, vec!["line 2: stepOrder and skillOrder must be numbers".to_string()]);
}

// Fixtures by their repo path, so the pipeline's test step sees which files these tests read.
const REPO_FIXTURES: [&str; 5] = [
    "platform/services/chorus-make/tests/fixtures/cicd-golden.yaml",
    "platform/services/chorus-make/tests/fixtures/cicd-land-golden.yaml",
    "platform/services/chorus-make/tests/fixtures/cicd-rows.tsv",
    "platform/services/chorus-make/tests/fixtures/cicd-graph.trig",
    "platform/services/chorus-make/tests/fixtures/mcp-server-names-20261009.txt",
];

#[test]
fn every_fixture_named_here_exists() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    for f in REPO_FIXTURES {
        let local = f.strip_prefix("platform/services/chorus-make/").unwrap();
        assert!(std::path::Path::new(&dir).join(local).is_file(), "missing fixture {f}");
    }
}
