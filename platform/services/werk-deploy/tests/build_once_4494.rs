//! #4494 — build once: prod ships the binary the demo ran when the crate's
//! inputs are the same in the card's werk and on main; otherwise it builds
//! from main, so a change another card landed is never dropped (Silas,
//! 2026-10-11 09:19). Today deploy-prod rebuilds every time: 6.1 min median.
use werk_deploy::{build_inputs, may_promote};

fn t(p: &str, h: &str) -> (String, String) { (p.to_string(), h.to_string()) }

#[test]
fn a_crates_inputs_are_its_dir_the_shared_includes_and_its_path_deps() {
    let toml = "[dependencies]\nwerk-teardown = { path = \"../werk-teardown\" }\nserde = \"1\"\n";
    assert_eq!(build_inputs("platform/services/werk-accept", toml), vec![
        "platform/services/werk-accept", "platform/services/shared", "platform/services/werk-teardown"]);
    // a product API reaching back into platform/services
    let toml = "x = { path = \"../../../../platform/services/chorus-sdk-rs\" }\n";
    assert_eq!(build_inputs("directing/products/pulse/api", toml)[2], "platform/services/chorus-sdk-rs");
}

#[test]
fn same_trees_in_the_werk_and_on_main_promote() {
    let werk = vec![t("a", "111"), t("platform/services/shared", "222")];
    assert!(may_promote(&werk, &werk.clone()));
}

#[test]
fn negative_proof_main_moved_under_the_crate_means_rebuild() {
    let werk = vec![t("a", "111"), t("platform/services/shared", "222")];
    let main = vec![t("a", "111"), t("platform/services/shared", "999")];
    assert!(!may_promote(&werk, &main), "another card changed a shared include; the demo binary lacks it");
}

#[test]
fn an_unreadable_tree_never_promotes() {
    let werk = vec![t("a", "")];
    assert!(!may_promote(&werk, &werk.clone()));
    assert!(!may_promote(&[], &[]));
}
