//! #4419 reopened — a changed file's domains come from the tests that exercise
//! it, not from what its own text mentions; the card runs those tests. Jeff, 2026-10-06: "tagging the file
//! is the wrong spot … we are tagging the suites or cases". The three cases
//! Wren measured on #4438: server.ts read as 22 domains, pre-commit as 6,
//! service-instances.ttl as `tests`.
use std::collections::HashMap;
use werk_test::{domain_selection, exercisers_of, mention_key, mentions, TestRow, HUB_EXERCISERS};

fn row(f: &str, covers: &str) -> TestRow {
    TestRow { file_path: f.into(), covers: covers.into(), pyramid_layer: String::new(), hermeticity: String::new(), test_concern: String::new() }
}

fn crate_of(p: &str) -> Option<String> {
    let rest = p.strip_prefix("platform/services/")?;
    Some(format!("platform/services/{}", rest.split('/').next()?))
}

fn select(f: &str, rows: &[TestRow], texts: &HashMap<&str, &str>, routes: Option<&[String]>) -> werk_test::DomainSelection {
    let text_of = |p: &str| texts.get(p).map(|s| s.to_string());
    let ex = exercisers_of(f, rows, &text_of, &crate_of, routes);
    let mut m = HashMap::new();
    m.insert(f.to_string(), ex);
    domain_selection(&[f.to_string()], &m, rows)
}

#[test]
fn the_key_is_how_a_test_names_the_file() {
    assert_eq!(mention_key("platform/api/src/server.ts").as_deref(), Some("src/server"));
    assert_eq!(mention_key("platform/hooks/pre-commit").as_deref(), Some("hooks/pre-commit"));
    assert_eq!(mention_key("designing/data/service-instances.ttl").as_deref(), Some("data/service-instances"));
    assert_eq!(mention_key("platform/services/werk-test/src/lib.rs"), None);
}

#[test]
fn negative_proof_a_longer_name_is_not_a_mention() {
    assert!(mentions("import app from '../src/server';", "src/server"));
    assert!(!mentions("import h from '../src/server-helpers';", "src/server"));
    assert!(!mentions("x/xsrc/server", "src/server"));
}

#[test]
fn pre_commit_takes_the_domain_of_the_suite_that_runs_it_not_the_six_binaries_it_calls() {
    let rows = vec![
        row("platform/tests/pre-commit-gates.bats", "version-control"),
        row("platform/tests/deploy-x.bats", "deploys"),
        row("platform/tests/other-vc.bats", "version-control"),
    ];
    let texts = HashMap::from([
        ("platform/tests/pre-commit-gates.bats", "run \"$ROOT/platform/hooks/pre-commit\""),
        ("platform/tests/deploy-x.bats", "athena-deploy werk-test gitleaks"),
        ("platform/tests/other-vc.bats", "git-queue"),
    ]);
    let s = select("platform/hooks/pre-commit", &rows, &texts, None);
    assert_eq!(s.domains.iter().cloned().collect::<Vec<_>>(), vec!["version-control".to_string()]);
    assert!(s.tests.contains_key("platform/tests/pre-commit-gates.bats"));
    // Jeff 2026-10-06: the card runs the exercising tests, not their whole domain
    assert!(!s.tests.contains_key("platform/tests/other-vc.bats"), "{:?}", s.tests);
    assert!(!s.tests.contains_key("platform/tests/deploy-x.bats"));
}

#[test]
fn a_data_file_takes_the_domain_of_the_tests_that_read_it() {
    let rows = vec![row("platform/tests/services-load.bats", "services"), row("platform/tests/t.bats", "tests")];
    let texts = HashMap::from([
        ("platform/tests/services-load.bats", "load designing/data/service-instances.ttl"),
        ("platform/tests/t.bats", "chorus:Test chorus:TestCase"),
    ]);
    let s = select("designing/data/service-instances.ttl", &rows, &texts, None);
    assert_eq!(s.domains.iter().cloned().collect::<Vec<_>>(), vec!["services".to_string()]);
}

fn hub_rows() -> (Vec<TestRow>, HashMap<&'static str, String>) {
    let mut rows = Vec::new();
    let mut texts = HashMap::new();
    for i in 0..HUB_EXERCISERS + 5 {
        let p: &'static str = Box::leak(format!("platform/api/tests/t{i}.test.ts").into_boxed_str());
        rows.push(row(p, &format!("d{i}")));
        texts.insert(p, "import app from '../src/server';".to_string());
    }
    let p = "platform/api/tests/spine-event-endpoint.integration.test.ts";
    rows.push(row(p, "spine"));
    texts.insert(p, "import app from '../src/server'; await hit('/api/chorus/spine-events/abc')".to_string());
    (rows, texts)
}

#[test]
fn a_one_route_change_to_server_ts_selects_the_tests_that_call_that_route() {
    let (rows, texts) = hub_rows();
    let t: HashMap<&str, &str> = texts.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let routes = vec!["/api/chorus/spine-events/:id".to_string()];
    let s = select("platform/api/src/server.ts", &rows, &t, Some(&routes));
    assert_eq!(s.domains.iter().cloned().collect::<Vec<_>>(), vec!["spine".to_string()]);
}

#[test]
fn negative_proof_without_the_route_every_test_that_imports_server_ts_exercises_it() {
    let (rows, texts) = hub_rows();
    let t: HashMap<&str, &str> = texts.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let s = select("platform/api/src/server.ts", &rows, &t, None);
    assert_eq!(s.domains.len(), HUB_EXERCISERS + 6);
}

#[test]
fn a_rust_file_is_exercised_by_its_crates_tests() {
    let rows = vec![row("platform/services/werk-test/tests/a.rs", "tests"), row("platform/services/other/tests/b.rs", "code")];
    let s = select("platform/services/werk-test/src/lib.rs", &rows, &HashMap::new(), None);
    assert_eq!(s.domains.iter().cloned().collect::<Vec<_>>(), vec!["tests".to_string()]);
}

#[test]
fn negative_proof_a_file_no_test_exercises_is_named_not_given_a_domain() {
    let rows = vec![row("platform/api/tests/x.test.ts", "code")];
    let texts = HashMap::from([("platform/api/tests/x.test.ts", "import y from '../src/other';")]);
    let s = select("platform/api/src/new-thing.ts", &rows, &texts, None);
    assert!(s.domains.is_empty());
    assert_eq!(s.untagged, vec!["platform/api/src/new-thing.ts".to_string()]);
}

#[test]
fn a_new_unregistered_test_runs_itself_and_is_not_refused() {
    let rows = vec![row("platform/api/tests/x.test.ts", "code")];
    let s = domain_selection(&["platform/api/tests/event-types-4438.test.ts".to_string()], &HashMap::new(), &rows);
    assert!(s.untagged.is_empty());
    assert_eq!(s.tests.get("platform/api/tests/event-types-4438.test.ts").map(|s| s.as_str()), Some("changed"));
}

#[test]
fn negative_proof_a_source_file_is_not_mistaken_for_a_test() {
    assert!(!werk_test::is_test_file("platform/api/src/test-run-report.ts"));
    assert!(werk_test::is_test_file("platform/api/tests/a.test.ts"));
}

#[test]
fn a_sibling_import_is_a_mention() {
    assert_eq!(werk_test::relative_import("platform/pulse/src/a.test.ts", "platform/pulse/src/peers.ts").as_deref(), Some("./peers"));
    assert_eq!(werk_test::relative_import("platform/api/tests/h/x.test.ts", "platform/api/src/lib/y.ts").as_deref(), Some("../../src/lib/y"));
    let rows = vec![row("platform/pulse/src/agent-route.test.ts", "messages")];
    let texts = HashMap::from([("platform/pulse/src/agent-route.test.ts", "import { peers } from './peers';")]);
    let s = select("platform/pulse/src/peers.ts", &rows, &texts, None);
    assert!(s.untagged.is_empty(), "{:?}", s);
}

#[test]
fn negative_proof_a_sibling_with_a_longer_name_is_not_a_mention() {
    let rows = vec![row("platform/pulse/src/agent-route.test.ts", "messages")];
    let texts = HashMap::from([("platform/pulse/src/agent-route.test.ts", "import { x } from './peers-old';")]);
    let s = select("platform/pulse/src/peers.ts", &rows, &texts, None);
    assert_eq!(s.untagged, vec!["platform/pulse/src/peers.ts".to_string()]);
}

// #4419 reopened (Jeff 2026-10-06 "wtf") — #4420 run 8 changed chorus-werk-status;
// its only test was test-cws-3782.sh, which werk-test has no runner for; 0 ran, pass.
fn runnable(f: &str) -> bool {
    werk_test::runnable_test(f, f.starts_with("platform/api/"))
}

#[test]
fn negative_proof_the_4420_diff_is_refused() {
    let changed = vec!["platform/scripts/chorus-werk-status".to_string(), "platform/tests/test-cws-3782.sh".to_string()];
    let s = domain_selection(&changed, &HashMap::new(), &[]);
    let unrun = werk_test::unrun_changes(&changed, &s, &runnable);
    let names: Vec<&str> = unrun.iter().map(|(f, _)| f.as_str()).collect();
    assert_eq!(names, vec!["platform/scripts/chorus-werk-status", "platform/tests/test-cws-3782.sh"]);
}

#[test]
fn the_same_change_with_a_bats_suite_that_runs_it_passes() {
    let changed = vec!["platform/scripts/chorus-werk-status".to_string(), "platform/tests/cws-3782.bats".to_string()];
    let mut m = HashMap::new();
    m.insert(changed[0].clone(), vec!["platform/tests/cws-3782.bats".to_string()]);
    let s = domain_selection(&changed, &m, &[]);
    assert!(werk_test::unrun_changes(&changed, &s, &runnable).is_empty());
}

#[test]
fn docs_and_data_with_no_test_are_named_not_refused() {
    let changed = vec!["designing/docs/x.html".to_string(), "designing/data/y.ttl".to_string()];
    let s = domain_selection(&changed, &HashMap::new(), &[]);
    assert!(werk_test::unrun_changes(&changed, &s, &runnable).is_empty());
    assert_eq!(s.untagged.len(), 2);
}

#[test]
fn a_jest_test_in_a_package_runs_a_bare_shell_test_does_not() {
    assert!(werk_test::runnable_test("platform/api/tests/a.test.ts", true));
    assert!(werk_test::runnable_test("platform/tests/x.bats", false));
    assert!(werk_test::runnable_test("platform/services/werk-test/tests/a.rs", false));
    assert!(!werk_test::runnable_test("platform/tests/test-cws-3782.sh", false));
}

#[test]
fn a_shell_test_run_by_a_bats_suite_is_not_refused_and_the_suite_runs() {
    let changed = vec!["platform/tests/test-cws-3782.sh".to_string()];
    let mut m = HashMap::new();
    m.insert(changed[0].clone(), vec!["platform/tests/cws-3782.bats".to_string()]);
    let s = domain_selection(&changed, &m, &[]);
    assert!(werk_test::unrun_changes(&changed, &s, &runnable).is_empty());
    assert!(s.tests.contains_key("platform/tests/cws-3782.bats"), "{:?}", s.tests);
}

#[test]
fn negative_proof_another_packages_server_is_not_this_one() {
    let rows = vec![row("platform/api/tests/a.test.ts", "code")];
    let texts = HashMap::from([("platform/api/tests/a.test.ts", "import app from '../src/server';")]);
    let s = select("directing/clearing/src/server.ts", &rows, &texts, None);
    assert!(s.tests.is_empty(), "{:?}", s.tests);
    let s = select("platform/api/src/server.ts", &rows, &texts, None);
    assert!(s.tests.contains_key("platform/api/tests/a.test.ts"));
}

#[test]
fn a_route_change_selects_the_tests_that_call_it_over_http_too() {
    let (mut rows, mut texts) = hub_rows();
    let p = "directing/clearing/tests/tiles.test.ts";
    rows.push(row(p, "messages"));
    texts.insert(p, "await fetch(`${API}/api/chorus/spine-events?x=1`)".to_string());
    let t: HashMap<&str, &str> = texts.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let routes = vec!["/api/chorus/spine-events/:id".to_string()];
    let s = select("platform/api/src/server.ts", &rows, &t, Some(&routes));
    assert!(s.tests.contains_key(p), "{:?}", s.tests);
}
