//! #4405 reopen — repositories nest and depend like products and domains
//! (Jeff, 2026-10-12: "like domain and product", "chorus repo depends on
//! shared-security and shared-observability and gathering"), the Repository
//! row is created when it is new (the first real crawl of another repo got a
//! 404 from a PUT), and a unit with no repository is never assumed to be
//! chorus's (the leak Jeff named: "i need to be able to cleanly separate
//! chorus from other apps").
use chorus_crawl::code::{plan_units, Unit};
use chorus_crawl::code_run::{desired_rows, parse_repo_decl, repository_fields, repository_write, rows_of_repo, unowned, RepoDecl};

fn u(path: &str) -> Unit {
    Unit { path: path.to_string(), kind: "package", deps: vec![], includes: vec![], name: String::new() }
}

fn legacy(name: &str, path: &str) -> (String, Vec<(String, String)>) {
    (name.into(), vec![("unitPath".into(), path.into()), ("unitKind".into(), "package".into())])
}

#[test]
fn a_repository_says_what_it_depends_on_and_what_it_is_part_of() {
    let d = parse_repo_decl("# chorus\n\ndependsOn shared-security\ndependsOn  shared-observability\npartOf gathering-system\n").unwrap();
    assert_eq!(d.depends_on, vec!["shared-security".to_string(), "shared-observability".to_string()]);
    assert_eq!(d.part_of.as_deref(), Some("gathering-system"));
    assert_eq!(parse_repo_decl("").unwrap(), RepoDecl::default());
}

#[test]
fn negative_proof_a_second_parent_or_an_unknown_line_is_refused() {
    // single parent, the same as partOf on products and domains
    let e = parse_repo_decl("partOf a\npartOf b\n").unwrap_err();
    assert!(e.contains("line 2"), "{e}");
    let e = parse_repo_decl("dependsOn x\ndepends-on y\n").unwrap_err();
    assert!(e.contains("line 2") && e.contains("depends-on"), "{e}");
    assert!(parse_repo_decl("dependsOn\n").is_err(), "a keyword with no repository is refused");
}

#[test]
fn edges_name_only_repositories_the_store_holds_and_the_rest_are_reported() {
    let d = parse_repo_decl("dependsOn shared-security\ndependsOn gathering\npartOf system-x\n").unwrap();
    let stored = vec!["shared-security".to_string()];
    let (f, missing) = repository_fields(Some("https://github.com/gathering-chorus/chorus.git"), &d, &stored);
    assert!(f.contains(&("repoRemote".into(), "https://github.com/gathering-chorus/chorus.git".into())), "{f:?}");
    assert!(f.contains(&("dependsOn".into(), "shared-security".into())), "{f:?}");
    assert!(!f.iter().any(|(_, v)| v == "gathering" || v == "system-x"), "a dangling edge is never written: {f:?}");
    assert_eq!(missing, vec!["gathering".to_string(), "system-x".to_string()]);
}

#[test]
fn a_new_repository_is_created_and_a_stored_one_is_replaced() {
    let f = vec![("repoRemote".to_string(), "r".to_string())];
    let w = repository_write("/code/repositories", "jeff-bridwell-personal-site", &f, false);
    assert_eq!((w.method, w.path.as_str()), ("POST", "/code/repositories/batch"));
    assert_eq!(w.rows[0][0], ("name".to_string(), "jeff-bridwell-personal-site".to_string()));
    let w = repository_write("/code/repositories", "chorus", &f, true);
    assert_eq!((w.method, w.path.as_str()), ("PUT", "/code/repositories/chorus"));
}

#[test]
fn negative_proof_a_unit_with_no_repository_is_never_taken_as_chorus() {
    let mut current = desired_rows(&[u("platform/api")], "chorus");
    current.push(legacy("platform-api", "platform/api"));
    // the chorus pass no longer plans to retire it: nothing claims it silently
    let p = plan_units(&desired_rows(&[u("platform/api")], "chorus"), &rows_of_repo(current.clone(), "chorus", false));
    assert!(p.delete.is_empty(), "{p:?}");
    // it is named instead, and only an explicit adopt claims it
    assert_eq!(unowned(&current), vec!["platform-api".to_string()]);
    let p = plan_units(&desired_rows(&[u("platform/api")], "chorus"), &rows_of_repo(current, "chorus", true));
    assert_eq!(p.delete, vec!["platform-api".to_string()]);
}

#[test]
fn negative_proof_the_doors_empty_edge_is_no_repository() {
    // what the door actually returns for a row written before #4405: the field
    // present and empty (measured 2026-10-12 on /code/units, 49 of 49 rows)
    let door = vec![("designing-data".to_string(), vec![("inRepository".to_string(), String::new()), ("unitPath".to_string(), "designing/data".to_string())])];
    assert_eq!(unowned(&door), vec!["designing-data".to_string()]);
    assert!(rows_of_repo(door.clone(), "chorus", false).is_empty());
    assert_eq!(rows_of_repo(door, "chorus", true).len(), 1);
}
