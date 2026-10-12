//! #4405 — the code herald pointed at more than one repository. Jeff,
//! 2026-10-12: "imagine a use case where we pointed crawler at my other gh
//! projects". Before this, a unit's row had no repository: crawling a second
//! repo planned a delete of every Chorus unit, and two repos' "src" collided.
use chorus_crawl::code::{plan_units, repo_id, unit_fields, unit_name, Unit};
use chorus_crawl::code_run::{desired_rows, rows_of_repo};
use std::path::Path;

fn u(path: &str) -> Unit {
    Unit { path: path.to_string(), kind: "package", deps: vec![], includes: vec![], name: String::new() }
}

#[test]
fn the_repo_comes_from_the_remote_else_the_directory() {
    assert_eq!(repo_id(Some("https://github.com/gathering-chorus/chorus.git\n"), Path::new("/x/y")), "chorus");
    assert_eq!(repo_id(Some("git@github.com:WJeffBridwell/wordpress-blog.git"), Path::new("/x/y")), "wordpress-blog");
    assert_eq!(repo_id(None, Path::new("/Users/j/CascadeProjects/jeff-bridwell-personal-site")), "jeff-bridwell-personal-site");
    assert_eq!(repo_id(Some(""), Path::new("/a/My_Repo")), "my-repo");
}

#[test]
fn a_units_row_names_its_repository_and_two_repos_never_collide() {
    assert_ne!(unit_name("chorus", "src"), unit_name("wordpress-blog", "src"));
    let f = unit_fields(&u("platform/api"), "chorus");
    assert!(f.contains(&("inRepository".to_string(), "chorus".to_string())), "{f:?}");
    assert_eq!(f[0].1, "chorus--platform-api");
}

#[test]
fn negative_proof_crawling_another_repo_never_deletes_the_home_repos_units() {
    // the store holds Chorus's units (some written before rows carried a repo)
    let mut current = desired_rows(&[u("platform/api"), u("directing/clearing")], "chorus");
    current.push(("platform-api".into(), vec![("unitPath".into(), "platform/api".into()), ("unitKind".into(), "package".into())]));
    let other = desired_rows(&[u("src")], "wordpress-blog");
    // the old shape: diff against everything → plans deleting all three Chorus rows
    let unscoped = plan_units(&other, &current);
    assert_eq!(unscoped.delete.len(), 3, "the hazard this card closes");
    // scoped to its own repo, the other pass deletes nothing
    let p = plan_units(&other, &rows_of_repo(current.clone(), "wordpress-blog", "chorus"));
    assert!(p.delete.is_empty(), "{p:?}");
    assert_eq!(p.create.len(), 1);
}

#[test]
fn the_home_repos_pass_retires_rows_written_before_rows_had_a_repo() {
    let mut current = desired_rows(&[u("platform/api")], "chorus");
    current.push(("platform-api".into(), vec![("unitPath".into(), "platform/api".into()), ("unitKind".into(), "package".into())]));
    let p = plan_units(&desired_rows(&[u("platform/api")], "chorus"), &rows_of_repo(current, "chorus", "chorus"));
    assert_eq!(p.delete, vec!["platform-api".to_string()]);
    assert_eq!(p.unchanged, 1);
}

#[test]
fn a_row_the_door_returns_with_the_kind_prefix_still_reads_as_its_repo() {
    let current = vec![("x".to_string(), vec![("inRepository".to_string(), "repository-chorus".to_string())])];
    assert_eq!(rows_of_repo(current.clone(), "chorus", "chorus").len(), 1);
    assert!(rows_of_repo(current, "wordpress-blog", "chorus").is_empty());
}

#[test]
fn a_repo_with_no_generated_api_has_no_model_units() {
    use chorus_crawl::code::units;
    let read = |f: &str| match f { "app/package.json" => Some("{\"name\":\"app\"}".to_string()), _ => None };
    let us = units(&[], &["app/package.json".to_string()], &read, &|_: &str| vec![]);
    assert_eq!(us.iter().map(|u| u.path.as_str()).collect::<Vec<_>>(), vec!["app"], "no phantom model units");
}
