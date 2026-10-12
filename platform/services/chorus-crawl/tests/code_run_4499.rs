//! #4499 reopen — the code crawl's decisions as data. The 10-12 nightly had
//! chorus-crawl coverage at 66.6% (floor 68%) because these lived untested in
//! main.rs; they are tested here instead of lowering the floor.
use chorus_crawl::code::{plan_units, Unit};
use chorus_crawl::code_run::*;

fn s(a: &str, b: &str) -> (String, String) {
    (a.to_string(), b.to_string())
}

fn u(path: &str, deps: &[&str]) -> Unit {
    Unit { path: path.to_string(), kind: "crate", deps: deps.iter().map(|d| d.to_string()).collect(), includes: vec![], name: String::new() }
}

fn units() -> Vec<Unit> {
    vec![u("platform/services/werk-accept", &["platform/services/werk-teardown"]), u("platform/services/werk-teardown", &[])]
}

#[test]
fn desired_rows_drop_the_name_from_fields_and_keep_one_dependson_per_edge() {
    let d = desired_rows(&units(), "chorus");
    assert_eq!(d.len(), 2);
    let (name, fields) = &d[0];
    assert!(!name.is_empty());
    assert!(fields.iter().all(|(k, _)| k != "name"));
    assert_eq!(fields.iter().filter(|(k, _)| k == "dependsOn").count(), 1);
    assert!(d[1].1.iter().all(|(k, _)| k != "dependsOn"));
}

#[test]
fn current_rows_match_desired_so_an_unchanged_store_plans_no_writes() {
    let d = desired_rows(&units(), "chorus");
    let stored: Vec<Vec<(String, String)>> = d
        .iter()
        .map(|(n, f)| {
            let mut r = vec![s("name", n)];
            r.extend(f.iter().cloned());
            r
        })
        .collect();
    let c = current_rows(stored);
    let p = plan_units(&d, &c);
    assert_eq!((p.create.len(), p.update.len(), p.delete.len(), p.unchanged), (0, 0, 0, 2));
    // negative proof: a stored row missing its edge is an update, not unchanged
    let mut stale = c.clone();
    stale[0].1.retain(|(k, _)| k != "dependsOn");
    let p = plan_units(&d, &stale);
    assert_eq!(p.update.len(), 1);
}

#[test]
fn instance_links_follow_binary_and_source_and_ask_each_source_once() {
    let home = "/Users/x";
    let root = "/repo";
    let bins = vec![s("werk-accept", "platform/services/werk-accept")];
    let rows = vec![
        vec![s("runsService", "service-werk"), s("binaryPath", "/Users/x/.chorus/bin/werk-accept")],
        vec![s("runsService", "service-teardown"), s("sourcePath", "code-file-teardown-main")],
        vec![s("runsService", "service-teardown2"), s("sourcePath", "code-file-teardown-main")],
        // no runsService: links nothing, even with a binary
        vec![s("binaryPath", "/Users/x/.chorus/bin/werk-accept")],
    ];
    let mut asked: Vec<String> = Vec::new();
    let mut file_of = |src: &str| {
        asked.push(src.to_string());
        "platform/services/werk-teardown/src/main.rs".to_string()
    };
    let links = instance_links(&rows, &mut file_of, root, home, &units(), &bins, &|_| None);
    assert_eq!(asked, vec!["code-file-teardown-main"]);
    assert!(links.contains(&s("werk", "platform/services/werk-accept")), "{links:?}");
    // a source file links as itself; built_from resolves it to its unit
    assert!(links.contains(&s("teardown", "platform/services/werk-teardown/src/main.rs")), "{links:?}");
    assert!(links.contains(&s("teardown2", "platform/services/werk-teardown/src/main.rs")), "{links:?}");
    assert!(links.iter().all(|(svc, _)| !svc.starts_with("service-")));
}

#[test]
fn instance_links_without_evidence_link_nothing() {
    let rows = vec![vec![s("runsService", "service-x"), s("binaryPath", "/usr/bin/env")]];
    let mut file_of = |_: &str| -> String { panic!("no sourcePath, nothing to ask") };
    assert!(instance_links(&rows, &mut file_of, "/repo", "/Users/x", &units(), &[], &|_| None).is_empty());
}

#[test]
fn code_file_door_name_strips_the_iri_prefix_only() {
    assert_eq!(code_file_door_name("code-file-server-ts"), "server-ts");
    assert_eq!(code_file_door_name("server-ts"), "server-ts");
}

#[test]
fn writes_go_bare_units_first_then_whole_units_then_deletes_then_builtfrom() {
    let d = desired_rows(&units(), "chorus");
    let current = vec![s("gone", "x")].into_iter().map(|(n, _)| (n, vec![s("unitPath", "old"), s("unitKind", "crate")])).collect::<Vec<_>>();
    let p = plan_units(&d, &current);
    let built = vec![("werk".to_string(), vec!["platform-services-werk-accept".to_string()])];
    let w = code_writes("units", Some("services"), &p, &built);
    let shape: Vec<(&str, &str)> = w.iter().map(|x| (x.method, x.path.as_str())).collect();
    assert_eq!(shape[0], ("POST", "units/batch"));
    assert!(w[0].rows.iter().flatten().all(|(k, _)| k != "dependsOn"), "the batch carries no edges");
    assert_eq!(w[0].rows.len(), 2);
    assert_eq!(shape.iter().filter(|(m, _)| *m == "PUT").count(), 2);
    assert!(w.iter().filter(|x| x.method == "PUT").any(|x| x.rows[0].iter().any(|(k, _)| k == "dependsOn")), "the whole unit carries its edge");
    assert_eq!(shape[3], ("DELETE", "units/gone"));
    assert!(w[3].rows.is_empty());
    assert_eq!(shape[4], ("PATCH", "services/werk"));
    assert_eq!(w[4].rows[0], vec![s("builtFrom", "platform-services-werk-accept")]);
    assert_eq!(w.len(), 5);
}

#[test]
fn no_service_collection_no_builtfrom_and_nothing_planned_nothing_written() {
    let built = vec![("werk".to_string(), vec!["u".to_string()])];
    let empty = plan_units(&[], &[]);
    assert!(code_writes("units", None, &empty, &built).is_empty());
    // negative proof: the same built list with the collection served does write
    assert_eq!(code_writes("units", Some("services"), &empty, &built).len(), 1);
}
