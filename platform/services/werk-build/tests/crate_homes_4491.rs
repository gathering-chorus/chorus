// @domain: builds — placed by chorus-crawl's domain rules (#4538)
//! #4491 — werk-build finds a binary crate in any crate home, not only
//! platform/services: designing/services/<c> and <step>/products/<p>/api.

use std::fs;
use werk_build::{discover_build_units_in_tree, BuildUnit};

fn krate(root: &std::path::Path, dir: &str, name: &str, bin: bool) {
    let d = root.join(dir);
    fs::create_dir_all(d.join("src")).unwrap();
    fs::write(d.join("Cargo.toml"), format!("[package]\nname = \"{name}\"\n")).unwrap();
    fs::write(d.join("src").join(if bin { "main.rs" } else { "lib.rs" }), "").unwrap();
}

#[test]
fn crates_in_the_adr_041_tree_are_build_units() {
    let root = std::env::temp_dir().join(format!("wb-4491-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    krate(&root, "platform/services/werk-pull", "werk-pull", true);
    krate(&root, "designing/services/athena-gen", "athena-gen", true);
    krate(&root, "directing/products/pulse/api", "pulse-api", true);
    // NEGATIVE PROOF: a lib-only crate and a dir with no Cargo.toml are not units
    krate(&root, "designing/services/a-lib", "a-lib", false);
    fs::create_dir_all(root.join("designing/services/no-manifest/src")).unwrap();
    let units = discover_build_units_in_tree(&root);
    let rust: Vec<&str> = units.iter().filter_map(|u| match u { BuildUnit::RustCrate(n) => Some(n.as_str()), _ => None }).collect();
    assert_eq!(rust, vec!["athena-gen", "pulse-api", "werk-pull"], "{units:?}");
    let _ = fs::remove_dir_all(&root);
}
