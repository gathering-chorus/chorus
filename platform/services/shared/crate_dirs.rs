// #4491 — where the repo's Rust crates live, found by structure.
//
// A crate is a directory holding a Cargo.toml at one of three places:
//   platform/services/<crate>/        the legacy home
//   <step>/services/<crate>/          the ADR-041 tree (designing/services/athena-gen)
//   <step>/products/<product>/api/    a product's own API (directing/products/pulse/api)
// `platform/services/shared/` is never a crate: its files are include!()d.
// A crate's name is its Cargo package name (for every platform/services crate
// that equals the directory name). Included by werk-build, werk-test and
// werk-deploy, so the three verbs agree on what a crate is. std only.

/// The crate directory a repo path sits in, by the three shapes above, when
/// `has_manifest(dir)` says that directory holds a Cargo.toml. None otherwise.
pub fn crate_dir_of_path(path: &str, has_manifest: &dyn Fn(&str) -> bool) -> Option<String> {
    let parts: Vec<&str> = path.split('/').collect();
    let dir = match parts.as_slice() {
        [_, "services", c, _, ..] if !c.is_empty() => format!("{}/services/{}", parts[0], c),
        [_, "products", p, "api", _, ..] if !p.is_empty() => format!("{}/products/{}/api", parts[0], p),
        _ => return None,
    };
    if dir == "platform/services/shared" || !has_manifest(&dir) {
        return None;
    }
    Some(dir)
}

/// The package name in a Cargo.toml's `[package]` section.
pub fn cargo_package_name(manifest: &str) -> Option<String> {
    let mut in_package = false;
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
            continue;
        }
        if in_package {
            if let Some(rest) = t.strip_prefix("name") {
                let v = rest.trim_start().strip_prefix('=')?.trim().trim_matches('"');
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// Every crate under `root`, as (package name, repo-relative dir), sorted by dir.
pub fn crate_dirs(root: &std::path::Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let tops = match std::fs::read_dir(root) {
        Ok(t) => t,
        Err(_) => return out,
    };
    let subdirs = |p: &std::path::Path| -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(p)
            .map(|rd| rd.flatten().filter(|e| e.path().is_dir()).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
            .unwrap_or_default();
        v.sort();
        v
    };
    for top in tops.flatten() {
        let top_name = top.file_name().to_string_lossy().into_owned();
        if top_name.starts_with('.') || top_name == "node_modules" || top_name == "target" || !top.path().is_dir() {
            continue;
        }
        let mut cands: Vec<String> = subdirs(&top.path().join("services"))
            .into_iter().map(|c| format!("{}/services/{}", top_name, c)).collect();
        cands.extend(subdirs(&top.path().join("products")).into_iter().map(|p| format!("{}/products/{}/api", top_name, p)));
        for dir in cands {
            if dir == "platform/services/shared" {
                continue;
            }
            let manifest = root.join(&dir).join("Cargo.toml");
            if let Ok(text) = std::fs::read_to_string(&manifest) {
                let name = cargo_package_name(&text)
                    .unwrap_or_else(|| dir.rsplit('/').next().unwrap_or(&dir).to_string());
                out.push((name, dir));
            }
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

/// The repo-relative dir of the crate named `name` under `root`. A name the
/// scan does not find keeps the legacy `platform/services/<name>` answer, so a
/// caller's "no Cargo.toml there" error still names a real place.
pub fn crate_rel_dir(root: &std::path::Path, name: &str) -> String {
    crate_dirs(root)
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, d)| d)
        .unwrap_or_else(|| format!("platform/services/{}", name))
}

#[cfg(test)]
mod crate_dirs_4491 {
    use super::*;

    fn yes(_: &str) -> bool { true }

    #[test]
    fn the_three_homes_are_crates() {
        assert_eq!(crate_dir_of_path("platform/services/werk-test/src/main.rs", &yes).as_deref(), Some("platform/services/werk-test"));
        assert_eq!(crate_dir_of_path("designing/services/athena-gen/src/lib.rs", &yes).as_deref(), Some("designing/services/athena-gen"));
        assert_eq!(crate_dir_of_path("directing/products/pulse/api/src/main.rs", &yes).as_deref(), Some("directing/products/pulse/api"));
    }

    /// NEGATIVE PROOF: a directory with no Cargo.toml is not a crate, shared/
    /// never is, and a file directly under services/ or a product's non-api
    /// folder is not one either
    #[test]
    fn what_is_not_a_crate() {
        let none = |_: &str| false;
        assert_eq!(crate_dir_of_path("designing/services/athena-gen/src/lib.rs", &none), None);
        assert_eq!(crate_dir_of_path("platform/services/shared/scope_units.rs", &yes), None);
        assert_eq!(crate_dir_of_path("designing/services/README.md", &yes), None);
        assert_eq!(crate_dir_of_path("directing/products/pulse/ui/app.ts", &yes), None);
        assert_eq!(crate_dir_of_path("platform/api/src/server.ts", &yes), None);
    }

    #[test]
    fn the_package_name_is_the_crate_name() {
        let m = "[package]\nname = \"pulse-api\"\nversion = \"0.1.0\"\n\n[dependencies]\nname = \"not-this\"\n";
        assert_eq!(cargo_package_name(m).as_deref(), Some("pulse-api"));
        assert_eq!(cargo_package_name("[workspace]\nmembers = []\n"), None);
    }

    #[test]
    fn a_scan_finds_every_home_and_skips_shared() {
        let root = std::env::temp_dir().join(format!("crate-dirs-4491-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (dir, name) in [("platform/services/werk-test", "werk-test"), ("designing/services/athena-gen", "athena-gen"),
                            ("directing/products/pulse/api", "pulse-api"), ("platform/services/shared", "shared")] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
            std::fs::write(root.join(dir).join("Cargo.toml"), format!("[package]\nname = \"{}\"\n", name)).unwrap();
        }
        std::fs::create_dir_all(root.join("designing/services/no-manifest")).unwrap();
        let got = crate_dirs(&root);
        assert_eq!(got, vec![
            ("athena-gen".to_string(), "designing/services/athena-gen".to_string()),
            ("pulse-api".to_string(), "directing/products/pulse/api".to_string()),
            ("werk-test".to_string(), "platform/services/werk-test".to_string()),
        ]);
        assert_eq!(crate_rel_dir(&root, "pulse-api"), "directing/products/pulse/api");
        assert_eq!(crate_rel_dir(&root, "unknown"), "platform/services/unknown");
        let _ = std::fs::remove_dir_all(&root);
    }
}
