// @test-type: unit — a throwaway git origin with two land commits for one card; no store.
// @domain: pipelines
// @card: #4960
// @owner: silas
//! #4960: the card's first land stopped before its model deployed (deploy-prod failed,
//! so the success handler's athena-land never ran). Its fix-forward round landed a
//! second commit. A landed model read only the newest commit would leave the first
//! round's model behind for good. The landed card now reads from its FIRST landed
//! commit's parent. Exercises shared/card_scope.rs resolve_landed_from, base_rev,
//! first_landed_commit.
use athena_deploy::card_scope::{base_rev, first_landed_commit, resolve_landed_from};
use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git").current_dir(dir).args(["-c", "user.email=t@t", "-c", "user.name=t"]).args(args)
        .status().unwrap().success();
    assert!(ok, "git {args:?}");
}

fn repo(tag: &str, lands: &[(&str, &str)]) -> std::path::PathBuf {
    let t = std::env::temp_dir().join(format!("landed-4960-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&t);
    std::fs::create_dir_all(&t).unwrap();
    git(&t, &["init", "-q", "--bare", "origin.git"]);
    git(&t, &["clone", "-q", "origin.git", "home"]);
    let home = t.join("home");
    std::fs::create_dir_all(home.join("roles/silas/ontology")).unwrap();
    std::fs::write(home.join("README"), "base\n").unwrap();
    git(&home, &["add", "-A"]);
    git(&home, &["commit", "-q", "-m", "base"]);
    for (file, subject) in lands {
        std::fs::write(home.join(file), format!("# {subject}\n")).unwrap();
        git(&home, &["add", "-A"]);
        git(&home, &["commit", "-q", "-m", subject]);
    }
    git(&home, &["push", "-q", "origin", "HEAD:main"]);
    git(&home, &["fetch", "-q", "origin"]);
    home
}

fn rev(home: &Path, r: &str) -> String {
    String::from_utf8(Command::new("git").current_dir(home).args(["rev-parse", r]).output().unwrap().stdout).unwrap().trim().to_string()
}

#[test]
fn a_card_that_landed_twice_deploys_the_model_of_both_lands() {
    let home = repo("twice", &[("roles/silas/ontology/a.ttl", "#9 (silas) (#1)"), ("roles/silas/ontology/b.ttl", "#9 (silas) (#2)")]);
    let h = home.to_string_lossy().to_string();
    let card = resolve_landed_from("9", Some("silas".into()), &h).unwrap();
    assert_eq!(card.commit.as_deref(), Some(rev(&home, "HEAD").as_str()));
    assert!(card.sources.iter().any(|s| s.ends_with("a.ttl")), "the first land's model: {:?}", card.sources);
    assert!(card.sources.iter().any(|s| s.ends_with("b.ttl")), "the second land's model: {:?}", card.sources);
    assert_eq!(base_rev(&card), format!("{}^", rev(&home, "HEAD~1")));
    assert_eq!(first_landed_commit(&h, "9", "silas").unwrap().as_deref(), Some(rev(&home, "HEAD~1").as_str()));
}

#[test]
fn a_card_that_landed_once_is_read_exactly_as_before() {
    let home = repo("once", &[("roles/silas/ontology/a.ttl", "#9 (silas) (#1)")]);
    let card = resolve_landed_from("9", Some("silas".into()), &home.to_string_lossy()).unwrap();
    assert_eq!(card.since, None);
    assert_eq!(base_rev(&card), format!("{}^", rev(&home, "HEAD")));
}

#[test]
fn negative_proof_another_cards_land_between_is_not_this_cards_first() {
    // #90's land sits between; #9's first land is still #9's own, and #90's file is not #9's
    let home = repo("between", &[("roles/silas/ontology/a.ttl", "#9 (silas) (#1)"), ("roles/silas/ontology/x.ttl", "#90 (silas) (#3)"), ("roles/silas/ontology/b.ttl", "#9 (silas) (#2)")]);
    let h = home.to_string_lossy().to_string();
    assert_eq!(first_landed_commit(&h, "9", "silas").unwrap().as_deref(), Some(rev(&home, "HEAD~2").as_str()));
    let card = resolve_landed_from("9", Some("silas".into()), &h).unwrap();
    assert!(card.sources.iter().any(|s| s.ends_with("a.ttl")) && card.sources.iter().any(|s| s.ends_with("b.ttl")), "{:?}", card.sources);
    assert!(!card.sources.iter().any(|s| s.ends_with("x.ttl")), "#90's model is #90's: {:?}", card.sources);
}
