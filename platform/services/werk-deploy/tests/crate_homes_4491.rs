//! #4491 — werk-deploy deploys a changed crate from any crate home, by its
//! package name (a product API dir is `api`; its crate is `pulse-api`).

use std::fs;
use werk_deploy::{changed_crates_in, partition_lib_only, target_class_in, TargetClass};

fn krate(root: &std::path::Path, dir: &str, name: &str) {
    let d = root.join(dir);
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("Cargo.toml"), format!("[package]\nname = \"{name}\"\n")).unwrap();
    fs::write(d.join("src/main.rs"), "").unwrap();
}

#[test]
fn a_changed_crate_anywhere_in_the_tree_is_deployed_by_name() {
    let root = std::env::temp_dir().join(format!("wd-4491-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    krate(&root, "directing/products/pulse/api", "pulse-api");
    krate(&root, "designing/services/athena-gen", "athena-gen");
    let diff = "directing/products/pulse/api/src/main.rs\ndesigning/services/athena-gen/src/emit.rs\nplatform/services/werk-pull/src/lib.rs\nREADME.md\n";
    assert_eq!(changed_crates_in(&root, diff), vec!["werk-pull", "pulse-api", "athena-gen"]);
    // the name resolves to its dir: a binary crate with no plist is a CLI verb
    assert!(matches!(target_class_in("pulse-api", &root), Ok(TargetClass::CliVerb { .. })));
    let (deployable, lib_only) = partition_lib_only(&root, &["pulse-api".to_string()]);
    assert_eq!((deployable, lib_only), (vec!["pulse-api".to_string()], vec![]));
    // NEGATIVE PROOF: a path under a services dir with no Cargo.toml is not a crate
    assert_eq!(changed_crates_in(&root, "designing/services/notes/x.md\n"), Vec::<String>::new());
    let _ = fs::remove_dir_all(&root);
}
