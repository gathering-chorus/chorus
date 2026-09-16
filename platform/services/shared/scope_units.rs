// scope_units.rs — the ONE diff→units scoping core (#3783 build, #3821 tests).
// Source-included by werk-build AND werk-test (same pattern as failure_class.rs:
// not a crate, not a verb — one file both compile) so the two legs can never
// drift: what a diff can affect is a single answer, asked twice.
//
// Pure over its inputs: the caller supplies its unit set (build units and test
// units differ — a lib-only crate has tests but no binary), the DECLARED edges
// (TS `file:` deps + cargo path deps), and the diff. FULL is the loud escape,
// never the silent default.

/// A scopable unit: its stable name + werk-relative dir (no trailing slash).
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ScopeUnit {
    pub name: String,
    pub dir: String,
}

/// The scoping verdict: unit NAMES in (possibly empty) scope, or FULL with the
/// reason the caller must log.
#[derive(Debug, PartialEq, Eq)]
pub enum ScopeVerdict {
    Scoped(Vec<String>),
    Full(String),
}

/// Files that can never change build/test OUTPUT: prose, rendered assets,
/// role state, dashboards, static public/ trees.
pub fn scope_irrelevant(f: &str) -> bool {
    let ext = [".md", ".html", ".png", ".jpg", ".jpeg", ".gif", ".svg", ".txt", ".pdf"]
        .iter()
        .any(|e| f.ends_with(e));
    // #4169 — platform/launchd/ joins the list for the same reason platform/scripts/
    // and platform/tests/ are on it: a plist is a SCHEDULE, not an input to any
    // build or test output. It was the one unmapped path in #4166's five-file diff,
    // and it cost that card 314 units and 61 minutes.
    // #4173 — platform/tests/ came OFF this list (Silas, 2026-09-14: "I overshot
    // from not-a-build-input to not-worth-running"). A suite is not a build
    // input, but editing one must still RUN it; is_test_suite_path below scopes
    // a changed suite to itself instead of to nothing.
    // #4173 — .github/workflows/ is the pipeline's own definition. Changing it
    // changes how a run is ORCHESTRATED, not what any unit builds or tests, and
    // there is no unit it could scope to; the workflow is proven by the run it
    // drives plus the bats that read it.
    let dir = ["designing/", "roles/", "docs/", "knowledge/", "dashboards/", "messages/",
               "platform/scripts/", "platform/launchd/", "skills/", ".claude/",
               ".github/"]
        .iter()
        .any(|d| f.starts_with(d));
    // #4173 — git's own metadata is on the list for the same reason a plist is:
    // .gitignore decides what git TRACKS, never what a build or test produces.
    // It was the second unmapped path in this card's diff (the first, a runtime
    // watermark, belonged in .gitignore — which is how the two met).
    let vcs = matches!(
        f.rsplit('/').next().unwrap_or(f),
        ".gitignore" | ".gitattributes" | ".gitmodules"
    );
    ext || dir || vcs || f.contains("/public/")
}

/// A path that IS a test suite: the runner executes the file itself, so a change
/// to it scopes to itself and nothing else. `.bats` anywhere, and shell suites
/// under a tests directory — the same two shapes `test_unit_for_path` resolves.
pub fn is_test_suite_path(f: &str) -> bool {
    if f.ends_with(".bats") {
        return true;
    }
    let name = f.rsplit('/').next().unwrap_or(f);
    let shell = f.ends_with(".sh") && (name.starts_with("test-") || name.contains(".test."));
    shell && (f.starts_with("platform/tests/") || f.contains("/tests/"))
}

/// Diff → unit names + transitive DECLARED dependents. Any build/test-relevant
/// file outside every unit escapes to FULL with the file named — under-scoping
/// is unrepresentable by construction (#3092/#3126 defense). A provider that is
/// not itself a unit (lib-only crate) scopes through to its dependents.
pub fn scope_unit_names(
    changed: &[String],
    units: &[ScopeUnit],
    edges: &[(String, String)],
    force_full: bool,
) -> ScopeVerdict {
    use std::collections::BTreeSet;
    if force_full {
        return ScopeVerdict::Full("forced".to_string());
    }
    if changed.is_empty() {
        return ScopeVerdict::Full("empty-diff".to_string());
    }
    let mut names: BTreeSet<String> = BTreeSet::new();
    for f in changed {
        if scope_irrelevant(f) {
            continue;
        }
        if let Some(u) = units.iter().find(|u| f.starts_with(&format!("{}/", u.dir))) {
            names.insert(u.name.clone());
            continue;
        }
        // #4173 — platform/services/shared/ is not a crate: its files are
        // source-INCLUDED (`include!`) by the crates that use them, an edge the
        // cargo/TS scanners cannot see. Without it, editing this very file
        // refused the run as unmapped while the thing it changes is compiled
        // into two verbs. The provider name is the file's own path, so a change
        // to failure_class.rs does not drag in scope_units.rs's dependents.
        // A changed suite runs itself. Its "unit" is its own path; a caller
        // whose unit set has no suites simply filters it out, which is right —
        // a test-only diff builds nothing.
        if is_test_suite_path(f) {
            names.insert(f.clone());
            continue;
        }
        if f.starts_with("platform/services/shared/") {
            if edges.iter().any(|(p, _)| p == f) {
                names.insert(f.clone());
                continue;
            }
            return ScopeVerdict::Full(format!("unmapped:{}", f));
        }
        if let Some(rest) = f.strip_prefix("platform/services/") {
            if let Some(crate_name) = rest.split('/').next() {
                if edges.iter().any(|(p, _)| p == crate_name) {
                    names.insert(crate_name.to_string());
                    continue;
                }
            }
        }
        // #4000 — proving/flows is LANE-handled, not unit-built: the #3920 ui
        // lane already fires on these paths (ui_lane_fires), so a specs-only
        // diff scopes to the well-known "ui-flows" name instead of forcing
        // FULL. Deliberately NOT scope_irrelevant — these are real tests and
        // irrelevant would silently skip them. Other proving/ paths stay on
        // the loud FULL escape (the unmapped defense is untouched).
        if f.starts_with("proving/flows/") {
            names.insert("ui-flows".to_string());
            continue;
        }
        return ScopeVerdict::Full(format!("unmapped:{}", f));
    }
    loop {
        let add: Vec<String> = edges
            .iter()
            .filter(|(p, d)| names.contains(p) && !names.contains(d))
            .map(|(_, d)| d.clone())
            .collect();
        if add.is_empty() {
            break;
        }
        names.extend(add);
    }
    let scoped: Vec<String> = units
        .iter()
        .filter(|u| names.contains(&u.name))
        .map(|u| u.name.clone())
        .collect();
    ScopeVerdict::Scoped(scoped)
}

/// DECLARED dependency edges from the tree, provider→dependent: every TS
/// `file:` dep and every cargo path dep between platform/services crates.
/// fs-reading but crate-agnostic; both verbs call it against their root.
pub fn scope_declared_edges(root: &std::path::Path) -> Vec<(String, String)> {
    use std::fs;
    let mut edges: Vec<(String, String)> = Vec::new();
    // TS file: deps — walk package.json files (skip node_modules/.git/target).
    let mut stack = vec![root.to_path_buf()];
    let mut pkg_files: Vec<std::path::PathBuf> = Vec::new();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name();
            let name = name.to_string_lossy().to_string();
            if p.is_dir() {
                if name == "node_modules" || name == ".git" || name == "target" {
                    continue;
                }
                stack.push(p);
            } else if name == "package.json" {
                pkg_files.push(p);
            }
        }
    }
    let pkg_name_of = |content: &str| -> Option<String> {
        let i = content.find("\"name\"")?;
        let rest = &content[i + 6..];
        let c = rest.find(':')?;
        let rest = rest[c + 1..].trim_start();
        let rest = rest.strip_prefix('"')?;
        let e = rest.find('"')?;
        Some(rest[..e].to_string())
    };
    for pj in &pkg_files {
        let Ok(content) = fs::read_to_string(pj) else { continue };
        let Some(dep_name) = pkg_name_of(&content) else { continue };
        let pkg_dir = pj.parent().unwrap_or(root);
        let mut rest = content.as_str();
        while let Some(i) = rest.find("\"file:") {
            let tail = &rest[i + 6..];
            if let Some(e) = tail.find('"') {
                let target = &tail[..e];
                if let Ok(canon) = fs::canonicalize(pkg_dir.join(target)) {
                    if let Ok(lib_pj) = fs::read_to_string(canon.join("package.json")) {
                        if let Some(lib_name) = pkg_name_of(&lib_pj) {
                            edges.push((lib_name, dep_name.clone()));
                        }
                    }
                }
                rest = &tail[e..];
            } else {
                break;
            }
        }
    }
    // #4173 — `include!("../../shared/x.rs")` edges: a source-included file is a
    // real build input with no manifest entry anywhere, so it is invisible to
    // both scanners above. Provider is the shared file's werk-relative path.
    let services = root.join("platform/services");
    if let Ok(entries) = fs::read_dir(&services) {
        for e in entries.flatten() {
            let Some(dep_crate) = e.file_name().to_str().map(|s| s.to_string()) else { continue };
            let src = e.path().join("src");
            let Ok(files) = fs::read_dir(&src) else { continue };
            for f in files.flatten() {
                let Ok(text) = fs::read_to_string(f.path()) else { continue };
                for line in text.lines() {
                    // #4186 — `#[path = "../../shared/x.rs"] mod x;` is the same build
                    // input as `include!` and was invisible here: a card that used it
                    // failed its own test leg "unmapped:platform/services/shared/…".
                    let target = if let Some(i) = line.find("include!(\"") {
                        let rest = &line[i + "include!(\"".len()..];
                        let Some(q) = rest.find('"') else { continue };
                        &rest[..q]
                    } else if let Some(i) = line.find("#[path = \"") {
                        let rest = &line[i + "#[path = \"".len()..];
                        let Some(q) = rest.find('"') else { continue };
                        &rest[..q]
                    } else { continue };
                    let Some(name) = std::path::Path::new(target).file_name().and_then(|n| n.to_str())
                    else { continue };
                    if services.join("shared").join(name).is_file() {
                        edges.push((format!("platform/services/shared/{name}"), dep_crate.clone()));
                    }
                }
            }
        }
    }

    // Cargo path deps among platform/services.
    if let Ok(entries) = fs::read_dir(&services) {
        for e in entries.flatten() {
            let dir = e.path();
            let Ok(toml) = fs::read_to_string(dir.join("Cargo.toml")) else { continue };
            let Some(dep_crate) = e.file_name().to_str().map(|s| s.to_string()) else { continue };
            for line in toml.lines() {
                if let Some(i) = line.find("path") {
                    let rest = &line[i..];
                    if let Some(q1) = rest.find('"') {
                        if let Some(q2) = rest[q1 + 1..].find('"') {
                            let target = &rest[q1 + 1..q1 + 1 + q2];
                            if let Some(provider) =
                                std::path::Path::new(target).file_name().and_then(|n| n.to_str())
                            {
                                if services.join(provider).join("Cargo.toml").is_file() {
                                    edges.push((provider.to_string(), dep_crate.clone()));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    edges.sort();
    edges.dedup();
    edges
}

/// #4169 — Jeff, 2026-09-13: "we must never fall back to the whole tree that is
/// always wrong for a card we fail immediately and fix the data".
///
/// FULL has two kinds of reason and they must not share a fate. `forced` and
/// `empty-diff` are deliberate: someone asked for everything, or there is
/// nothing to scope from. `unmapped:<file>` is a DATA DEFECT — a path the model
/// does not know about — and widening hides it behind an hour of other people's
/// reds. Refuse instead, and name the file so it can be mapped.
pub fn full_reason_is_data_defect(reason: &str) -> bool {
    reason.starts_with("unmapped:")
}

/// The file named by an `unmapped:<file>` reason. The verdict has always known
/// it; before #4169 the caller logged the literal "unmapped-or-forced" and threw
/// it away, so a builder could not tell WHICH path widened their run.
pub fn unmapped_path(reason: &str) -> Option<&str> {
    reason.strip_prefix("unmapped:")
}

#[cfg(test)]
mod scope_refusal_4169 {
    use super::*;

    fn u(name: &str, dir: &str) -> ScopeUnit {
        ScopeUnit { name: name.to_string(), dir: dir.to_string() }
    }

    #[test]
    fn a_plist_is_not_a_build_or_test_input() {
        assert!(scope_irrelevant("platform/launchd/com.chorus.athena-validate.plist"));
    }

    #[test]
    fn the_four_1660_diff_files_all_scope_now() {
        // #4166's exact diff. Before this card it went FULL on the plist.
        for f in [
            "platform/api/public/borg/graph-validate.html",
            "platform/api/public/borg/graph-validate.txt",
            "platform/launchd/com.chorus.athena-validate.plist",
            "platform/scripts/athena-validate.sh",
        ] {
            assert!(scope_irrelevant(f), "{} should not widen a card", f);
        }
        // #4173 — the .bats file moved from "irrelevant" to "a suite that runs
        // itself" (Silas, 2026-09-14: the irrelevant list overshot to
        // not-worth-running). The claim this test makes is that none of the
        // five WIDEN the card, and that still holds: the suite scopes to
        // itself, which is a scoped run, not a FULL one.
        let bats = "platform/tests/4166-athena-validate-scheduled.bats";
        assert!(!scope_irrelevant(bats));
        assert!(is_test_suite_path(bats), "{} should scope to itself", bats);
        let units = [u(bats, bats)];
        let verdict = scope_unit_names(&[bats.to_string()], &units, &[], false);
        let ScopeVerdict::Scoped(names) = verdict else { panic!("a suite must not go FULL") };
        assert_eq!(names, vec![bats.to_string()]);
    }

    #[test]
    fn a_path_attribute_include_maps_the_shared_file_like_include_bang_4186() {
        let tmp = std::env::temp_dir().join(format!("scope-4186-{}", std::process::id()));
        let services = tmp.join("platform/services");
        std::fs::create_dir_all(services.join("shared")).unwrap();
        std::fs::create_dir_all(services.join("crate-a/src")).unwrap();
        std::fs::create_dir_all(services.join("crate-b/src")).unwrap();
        std::fs::write(services.join("shared/thing.rs"), "// shared").unwrap();
        std::fs::write(services.join("crate-a/src/lib.rs"), "#[path = \"../../shared/thing.rs\"]\nmod thing;\n").unwrap();
        std::fs::write(services.join("crate-b/src/lib.rs"), "mod t { include!(\"../../shared/thing.rs\"); }\n").unwrap();
        // NEGATIVE: a #[path] to a file that is NOT under shared/ must add no edge
        std::fs::write(services.join("crate-a/src/other.rs"), "#[path = \"local/helper.rs\"]\nmod h;\n").unwrap();
        let edges = scope_declared_edges(&tmp);
        let shared: Vec<_> = edges.iter().filter(|(p, _)| p == "platform/services/shared/thing.rs").map(|(_, c)| c.clone()).collect();
        assert!(shared.contains(&"crate-a".to_string()), "#[path] include maps: {:?}", edges);
        assert!(shared.contains(&"crate-b".to_string()), "include! still maps: {:?}", edges);
        assert!(!edges.iter().any(|(p, _)| p.contains("helper.rs")), "a non-shared #[path] adds nothing: {:?}", edges);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn negative_proof_an_unmapped_path_is_a_data_defect_and_names_itself() {
        let units = [u("werk-test", "platform/services/werk-test")];
        let verdict = scope_unit_names(
            &["some/unknown/place/thing.rs".to_string()], &units, &[], false);
        let ScopeVerdict::Full(reason) = verdict else { panic!("expected FULL") };
        assert!(full_reason_is_data_defect(&reason), "reason was {}", reason);
        assert_eq!(unmapped_path(&reason), Some("some/unknown/place/thing.rs"));
    }

    #[test]
    fn negative_proof_a_deliberate_full_is_NOT_a_data_defect() {
        // The check must separate the two states it exists to tell apart:
        // asked-for-everything must never be refused as a defect.
        let units = [u("werk-test", "platform/services/werk-test")];
        let forced = scope_unit_names(&["x.rs".to_string()], &units, &[], true);
        let ScopeVerdict::Full(r) = forced else { panic!("expected FULL") };
        assert_eq!(r, "forced");
        assert!(!full_reason_is_data_defect(&r));
        assert_eq!(unmapped_path(&r), None);

        let empty = scope_unit_names(&[], &units, &[], false);
        let ScopeVerdict::Full(r) = empty else { panic!("expected FULL") };
        assert_eq!(r, "empty-diff");
        assert!(!full_reason_is_data_defect(&r));
    }
}
