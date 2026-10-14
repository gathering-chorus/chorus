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
    let w = repository_write("/code/repositories", "jeff-bridwell-personal-site", &f, false).unwrap();
    assert_eq!((w.method, w.path.as_str()), ("POST", "/code/repositories/batch"));
    assert_eq!(w.rows[0][0], ("name".to_string(), "jeff-bridwell-personal-site".to_string()));
    let w = repository_write("/code/repositories", "chorus", &f, true).unwrap();
    assert_eq!((w.method, w.path.as_str()), ("PUT", "/code/repositories/chorus"));
}

#[test]
fn negative_proof_a_stored_repository_with_nothing_to_state_is_not_written() {
    // shared-security: no remote, no edges — an empty replace is a 422 at the door
    assert!(repository_write("/code/repositories", "shared-security", &[], true).is_none());
    // a new one is still created, so its units have a row to name
    assert!(repository_write("/code/repositories", "shared-security", &[], false).is_some());
}

#[test]
fn a_file_row_that_does_not_name_its_repository_is_restated() {
    use chorus_crawl::code_run::file_needs_repo;
    let door_unset = vec![("inRepository".to_string(), String::new())];
    assert!(file_needs_repo(&door_unset, "chorus"), "the 6,570 rows as the door returned them");
    assert!(file_needs_repo(&vec![], "chorus"));
    assert!(!file_needs_repo(&vec![("inRepository".to_string(), "repository-chorus".to_string())], "chorus"));
    assert!(file_needs_repo(&vec![("inRepository".to_string(), "repository-chorus".to_string())], "wine-diary"));
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

/// The door's own slug, as measured 2026-10-12: a unit sent as
/// `shared-security--root` was stored as `shared-security-root`. Lowercase,
/// anything outside a-z0-9 becomes "-", runs of "-" collapse, ends trimmed.
fn door_slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        let c = if c.is_ascii_alphanumeric() { c } else { '-' };
        if !(c == '-' && out.ends_with('-')) {
            out.push(c);
        }
    }
    out.trim_matches('-').to_string()
}

#[test]
fn negative_proof_a_units_name_survives_the_door_unchanged() {
    use chorus_crawl::code::unit_name;
    // the shape that broke every real crawl: the name sent was not the name stored
    assert_ne!(door_slug("shared-security--root"), "shared-security--root", "the hazard this test exists for");
    for (repo, path) in [("shared-security", "."), ("chorus", "platform/services/werk-test"), ("jeff-bridwell-personal-site", "media-server"), ("my-repo", "src/My_Thing.v2"), ("wine-diary", "--odd//path-")] {
        let n = unit_name(repo, path);
        assert_eq!(door_slug(&n), n, "{repo} {path}");
    }
}

/// #4405 reopen 4 — a second pass over an unchanged repository plans nothing
/// (Jeff, 2026-10-12: "make sure our runner is properly aligned integrated
/// and visible as a sensor"). The door serves edges as IRI local names with
/// the target's kind prefix, and an unset edge as "": the shape measured on
/// /code/units at 21:13. Before this, every pass rewrote all 50 units.
fn door_row(path: &str, dep: Option<&str>) -> Vec<(String, String)> {
    vec![
        ("name".into(), chorus_crawl::code::unit_name("chorus", path)),
        ("status".into(), String::new()),
        ("dependsOn".into(), dep.map(|d| format!("code-unit-{}", chorus_crawl::code::unit_name("chorus", d))).unwrap_or_default()),
        ("hasDomain".into(), String::new()),
        ("inRepository".into(), "repository-chorus".into()),
        ("ownedBy".into(), "principal-kade".into()),
        ("unitKind".into(), "package".into()),
        ("unitPath".into(), path.into()),
    ]
}

fn dep_unit(path: &str, dep: &str) -> Unit {
    Unit { deps: vec![dep.to_string()], ..u(path) }
}

#[test]
fn a_second_pass_over_the_doors_own_rows_plans_nothing() {
    use chorus_crawl::code_run::current_rows;
    let units = [dep_unit("platform/api", "designing/data"), u("designing/data")];
    let door = vec![door_row("platform/api", Some("designing/data")), door_row("designing/data", None)];
    let p = plan_units(&desired_rows(&units, "chorus"), &current_rows(door));
    assert!(p.create.is_empty() && p.update.is_empty() && p.delete.is_empty(), "{p:?}");
    assert_eq!(p.unchanged, 2);
}

#[test]
fn negative_proof_one_changed_unit_plans_exactly_one_update() {
    use chorus_crawl::code_run::current_rows;
    // the same door rows, but the build files now say api depends on nothing
    let units = [u("platform/api"), u("designing/data")];
    let door = vec![door_row("platform/api", Some("designing/data")), door_row("designing/data", None)];
    let p = plan_units(&desired_rows(&units, "chorus"), &current_rows(door));
    assert_eq!(p.update.len(), 1, "{p:?}");
    assert!(p.create.is_empty() && p.delete.is_empty(), "{p:?}");
    assert_eq!(p.unchanged, 1);
}

// #4405 reopen 6 — Silas for Jeff (#4518): `sensor files --target` wrote nothing
// for personal-site (0 of 6,605 file rows) because the pass refused the flag.
// Each pass owns only its own repository's rows.
mod files_target_r6 {
    use chorus_crawl::code_run::row_in_pass;
    fn row(repo: &str) -> Vec<(String, String)> {
        vec![("filePath".into(), "a".into()), ("inRepository".into(), if repo.is_empty() { String::new() } else { format!("chorus:repository-{repo}") })]
    }

    #[test]
    fn a_target_pass_owns_only_its_repository() {
        assert!(row_in_pass(&row("jeff-bridwell-personal-site"), "jeff-bridwell-personal-site", false));
    }

    #[test]
    fn negative_proof_a_target_pass_never_owns_a_chorus_row() {
        assert!(!row_in_pass(&row("chorus"), "jeff-bridwell-personal-site", false));
        assert!(!row_in_pass(&row(""), "jeff-bridwell-personal-site", false), "a row naming no repository is never another repo's to delete");
    }

    #[test]
    fn negative_proof_the_home_pass_never_owns_another_repositorys_row() {
        assert!(!row_in_pass(&row("jeff-bridwell-personal-site"), "chorus", true));
        assert!(row_in_pass(&row("chorus"), "chorus", true));
        assert!(row_in_pass(&row(""), "chorus", true), "legacy rows stay the home pass's");
    }

    #[test]
    fn target_with_no_directory_is_refused_and_nothing_runs() {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_chorus-crawl")).args(["--target"]).env("CHORUS_ROLE", "kade").output().unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&out.stderr).contains("--target needs a directory"));
    }
}

// #4405 R6.4 — found on prod by R6.3: 600 of 1,792 personal-site rows refused.
mod row_names_r64 {
    use chorus_crawl::{file_row_name, stable_name};

    #[test]
    fn chorus_names_do_not_move() {
        assert_eq!(file_row_name("chorus", ".gitignore", true), stable_name(".gitignore"));
    }

    #[test]
    fn negative_proof_the_same_path_in_two_repos_is_two_rows() {
        assert_ne!(file_row_name("jeff-bridwell-personal-site", ".gitignore", false), file_row_name("chorus", ".gitignore", true));
    }

    #[test]
    fn negative_proof_a_long_path_still_gives_a_name_the_door_takes() {
        let long = format!("public/{}/index.html", "very-long-directory-name/".repeat(8));
        let n = file_row_name("jeff-bridwell-personal-site", &long, false);
        assert!(n.len() <= 128, "{} chars", n.len());
        assert!(n.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'), "{n}");
        // two long paths that differ only past the cut still differ
        assert_ne!(n, file_row_name("jeff-bridwell-personal-site", &format!("{long}x"), false));
    }
}

// #4405 R6.5 — every pass rewrote ~500 unchanged personal-site rows.
mod domain_sets_r65 {
    use chorus_crawl::domain_set;

    #[test]
    fn negative_proof_the_unplaced_marker_is_no_domain_on_either_side() {
        assert_eq!(domain_set([""].into_iter()), domain_set(std::iter::empty()));
    }

    #[test]
    fn a_two_domain_row_compares_equal_in_any_order() {
        assert_eq!(domain_set(["version-control", "identity", ""].into_iter()), domain_set(["identity", "version-control"].into_iter()));
    }

    #[test]
    fn negative_proof_a_real_domain_change_still_differs() {
        assert_ne!(domain_set(["identity"].into_iter()), domain_set(["identity", "security"].into_iter()));
    }

    #[test]
    fn the_door_page_reader_keeps_both_domains_of_a_row() {
        let page = r#"{"data":[{"name":"f","filePath":"a","hasDomain":["identity","version-control"]}]}"#;
        let rows = chorus_crawl::services::door_page_rows(page);
        assert_eq!(rows[0].iter().filter(|(k, _)| k == "hasDomain").count(), 2);
    }
}

// #4405 reopen 7 — the 10-14 nightly: chorus-crawl --validate red with 1,792
// stale rows, every one a personal-site file. The full pass's re-read compared
// chorus's tree with every CodeFile row, whatever repository it came from.
mod validate_scoped_r7 {
    use chorus_crawl::code_run::rows_in_pass;
    use chorus_crawl::{reconcile, InGraph, OnDisk};
    fn row(path: &str, repo: &str) -> InGraph {
        InGraph { path: path.into(), sha: "s".into(), other: vec![("inRepository".into(), format!("chorus:repository-{repo}"))] }
    }
    fn disk() -> Vec<OnDisk> { vec![OnDisk { path: "a.rs".into(), sha: "s".into(), classified: true }] }
    fn graph() -> Vec<InGraph> { vec![row("a.rs", "chorus"), row("app.js", "jeff-bridwell-personal-site")] }

    #[test]
    fn another_repositorys_row_is_never_stale() {
        let d = reconcile(&disk(), &rows_in_pass(graph(), "chorus", true));
        assert!(d.missing_from_tree.is_empty(), "foreign rows counted stale: {:?}", d.missing_from_tree);
    }

    /// NEGATIVE PROOF: the unscoped read (10-14's) calls the foreign row stale.
    #[test]
    fn negative_proof_the_unscoped_read_calls_it_stale() {
        let d = reconcile(&disk(), &graph());
        assert_eq!(d.missing_from_tree, vec!["app.js".to_string()]);
    }
}
