// @domain: tests
//! #4995 — werk-test scopes a domain's own API crate (`<step>/domains/<domain>/api`,
//! ADR-041): a change under shaping/domains/principles/api runs that crate's tests,
//! where before it was refused as "claimed by no unit". NEGATIVE PROOF: the domain's
//! model folder, outside api/, is not a crate.
use werk_test::crate_dir_of_path;

#[test]
fn a_domain_api_path_belongs_to_its_crate() {
    let yes = |_: &str| true;
    assert_eq!(crate_dir_of_path("shaping/domains/principles/api/src/main.rs", &yes).as_deref(), Some("shaping/domains/principles/api"));
    assert_eq!(crate_dir_of_path("shaping/domains/principles/api/Cargo.lock", &yes).as_deref(), Some("shaping/domains/principles/api"));
}

#[test]
fn negative_proof_a_domain_model_folder_is_no_crate() {
    let yes = |_: &str| true;
    assert_eq!(crate_dir_of_path("shaping/domains/principles/model.ttl", &yes), None);
    // and a domain api with no Cargo.toml is not one either
    assert_eq!(crate_dir_of_path("shaping/domains/principles/api/src/main.rs", &|_: &str| false), None);
}
