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
    let dir = ["designing/", "roles/", "docs/", "knowledge/", "dashboards/", "messages/",
               "platform/scripts/", "platform/tests/", "skills/", ".claude/"]
        .iter()
        .any(|d| f.starts_with(d));
    ext || dir || f.contains("/public/")
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
    // Cargo path deps among platform/services.
    let services = root.join("platform/services");
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
