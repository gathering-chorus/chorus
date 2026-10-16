// @domain: builds — placed by chorus-crawl's domain rules (#4538)
//! #4412 run 14 — a changed cucumber step definition was refused as "no
//! registered test exercises it": nothing imports a step file, the features
//! that use its steps run it.

use werk_test::TestRow;

fn row(p: &str) -> TestRow {
    TestRow { file_path: p.into(), covers: String::new(), pyramid_layer: String::new(), hermeticity: String::new(), test_concern: String::new(), needs: String::new() }
}

const STEPS: &str = "When('Jeff loads the LAN URL without auth', function () {});\n\
Then('the page redirects to the public Clearing', function () {});\n\
When('Jeff enters the name {string} via LAN', function (n) {});\n\
Then('ok', function () {});\n";

fn text(p: &str) -> Option<String> {
    match p {
        "platform/tests/features/step_definitions/clearing_steps.ts" => Some(STEPS.into()),
        "platform/tests/features/clearing/clearing-access.feature" => Some("Feature: x\n  Scenario: y\n    When Jeff loads the LAN URL without auth\n    Then the page redirects to the public Clearing\n".into()),
        "platform/tests/features/clearing/names.feature" => Some("Feature: n\n  Scenario: m\n    And Jeff enters the name \"Bob\" via LAN\n".into()),
        "platform/tests/features/seeds/seed-media.feature" => Some("Feature: s\n  Scenario: t\n    Given a seed arrives\n    Then ok\n".into()),
        "platform/tests/unit/mentions-steps.test.ts" => Some("Jeff loads the LAN URL without auth".into()),
        _ => None,
    }
}

#[test]
fn a_step_definition_is_exercised_by_the_features_that_use_its_steps() {
    let rows = vec![
        row("platform/tests/features/clearing/clearing-access.feature"),
        row("platform/tests/features/clearing/names.feature"),
        row("platform/tests/features/seeds/seed-media.feature"),
        row("platform/tests/unit/mentions-steps.test.ts"),
    ];
    let got = werk_test::exercisers_of("platform/tests/features/step_definitions/clearing_steps.ts", &rows, &text, &|_| None, None);
    assert_eq!(got, vec![
        "platform/tests/features/clearing/clearing-access.feature".to_string(),
        "platform/tests/features/clearing/names.feature".to_string(),
    ]);
}

#[test]
fn negative_proof_a_feature_using_none_of_its_steps_is_not_selected() {
    // seed-media uses only "ok", a step too short to name one step
    let rows = vec![row("platform/tests/features/seeds/seed-media.feature")];
    let got = werk_test::exercisers_of("platform/tests/features/step_definitions/clearing_steps.ts", &rows, &text, &|_| None, None);
    assert!(got.is_empty(), "{got:?}");
    // and a step file outside the bdd lane is not a step definition
    assert!(!werk_test::is_step_definition("platform/tests/unit/clearing_steps.ts"));
}

// #4530 (Wren 10-15 15:24) — the card ADDS the features and the step file
// together; the registry holds neither until the land, so the step file was
// refused. The card's own new features are searched too.
#[test]
fn a_new_step_file_is_exercised_by_the_new_features_in_the_same_card() {
    let changed = vec![
        "platform/tests/features/clearing/clearing-access.feature".to_string(),
        "platform/tests/features/step_definitions/clearing_steps.ts".to_string(),
    ];
    let cands = werk_test::card_candidates(&changed, &[row("platform/tests/unit/mentions-steps.test.ts")]);
    let got = werk_test::exercisers_of("platform/tests/features/step_definitions/clearing_steps.ts", &cands, &text, &|_| None, None);
    assert_eq!(got, vec!["platform/tests/features/clearing/clearing-access.feature".to_string()]);
    assert!(werk_test::is_card_test("platform/tests/features/clearing-v2/rooms.feature"));
    // and the bdd lane runs it, so it is not dropped as unrunnable
    assert!(werk_test::runnable_test("platform/tests/features/clearing-v2/rooms.feature", false));
}

/// NEGATIVE PROOF: a step file whose features are neither registered nor in the card is still refused.
#[test]
fn negative_proof_a_step_file_with_no_feature_anywhere_stays_unexercised() {
    let changed = vec!["platform/tests/features/step_definitions/clearing_steps.ts".to_string()];
    let cands = werk_test::card_candidates(&changed, &[row("platform/tests/unit/mentions-steps.test.ts")]);
    let got = werk_test::exercisers_of("platform/tests/features/step_definitions/clearing_steps.ts", &cands, &text, &|_| None, None);
    assert!(got.is_empty(), "{got:?}");
    assert!(!werk_test::is_card_test("platform/tests/cucumber-clearing-v2.js"));
    // a .feature outside the bdd lane's features dir has no runner
    assert!(!werk_test::runnable_test("docs/specs/rooms.feature", false));
}
