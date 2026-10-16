//! #4499 — the code crawl's decisions, as data: which unit rows to write,
//! which units each running service is built from, and the writes in the
//! order the door needs them. `run_code` in main.rs only does the I/O.

use crate::code::{run_paths, unit_fields, Unit, UnitPlan};

pub type Fields = Vec<(String, String)>;

/// The CodeUnit rows the build files state: (name, fields without the name).
pub fn desired_rows(units: &[Unit], repo: &str) -> Vec<(String, Fields)> {
    units
        .iter()
        .map(|u| {
            let f = unit_fields(u, repo);
            (f[0].1.clone(), f[1..].to_vec())
        })
        .collect()
}

/// The CodeUnit rows the store holds, in the same shape as `desired_rows`.
pub fn current_rows(rows: Vec<Fields>) -> Vec<(String, Fields)> {
    rows.into_iter()
        .map(|fields| {
            let name = fields.iter().find(|(k, _)| k == "name").map(|(_, v)| v.clone()).unwrap_or_default();
            (name, crate::services::written_form(fields.into_iter().filter(|(k, _)| k != "name").collect()))
        })
        .collect()
}

/// #4405 — the stored rows this pass may change: the ones in its own
/// repository. Crawling a second repo must never delete the first one's units
/// (the plan deletes every current row it does not want). A row that names no
/// repository belongs to none: it is only claimed by an explicit `adopt`
/// (Jeff, 2026-10-12: "cleanly separate chorus from other apps").
pub fn rows_of_repo(current: Vec<(String, Fields)>, repo: &str, adopt: bool) -> Vec<(String, Fields)> {
    current
        .into_iter()
        .filter(|(_, f)| match repo_of_row(f) {
            Some(r) => r == repo,
            None => adopt,
        })
        .collect()
}

/// The repository a stored row names. The door returns an unset edge as an
/// empty string, not an absent field: both mean no repository.
pub fn repo_of_row(f: &Fields) -> Option<&str> {
    f.iter().find(|(k, v)| k == "inRepository" && !v.is_empty()).and_then(|(_, r)| r.rsplit("repository-").next())
}

/// The stored units that name no repository, so a pass can say so out loud.
pub fn unowned(current: &[(String, Fields)]) -> Vec<String> {
    current.iter().filter(|(_, f)| repo_of_row(f).is_none()).map(|(n, _)| n.clone()).collect()
}

/// What a repository says about itself in the `.repository` file at its root:
/// the repositories it depends on, and the one it is part of (single parent,
/// like partOf on products and domains).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RepoDecl {
    pub depends_on: Vec<String>,
    pub part_of: Option<String>,
}

pub fn parse_repo_decl(text: &str) -> Result<RepoDecl, String> {
    let mut d = RepoDecl::default();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut w = line.split_whitespace();
        let (key, name, rest) = (w.next().unwrap_or(""), w.next(), w.next());
        let name = match (name, rest) {
            (Some(n), None) => n.to_string(),
            _ => return Err(format!(".repository line {}: expected `<keyword> <repository>`, got `{line}`", i + 1)),
        };
        match key {
            "dependsOn" => d.depends_on.push(name),
            "partOf" if d.part_of.is_none() => d.part_of = Some(name),
            "partOf" => return Err(format!(".repository line {}: a repository is part of one parent", i + 1)),
            _ => return Err(format!(".repository line {}: unknown keyword `{key}` (dependsOn, partOf)", i + 1)),
        }
    }
    Ok(d)
}

/// The Repository row's fields, and the declared repositories the store does
/// not hold yet: an edge to one would dangle (the door refuses it), so it is
/// left out and named.
pub fn repository_fields(remote: Option<&str>, decl: &RepoDecl, stored: &[String]) -> (Fields, Vec<String>) {
    let mut f: Fields = remote.map(|r| vec![("repoRemote".to_string(), r.to_string())]).unwrap_or_default();
    let mut missing = Vec::new();
    let edges = decl.depends_on.iter().map(|n| ("dependsOn", n)).chain(decl.part_of.iter().map(|n| ("partOf", n)));
    for (k, n) in edges {
        if stored.contains(n) {
            f.push((k.to_string(), n.clone()));
        } else {
            missing.push(n.clone());
        }
    }
    (f, missing)
}

/// A new repository is created (a PUT of an absent row is a 404 at the door);
/// a stored one is replaced whole — the crawler is its only writer. A stored
/// repository with nothing to state is left as it is: the door refuses an
/// empty replace (shared-security, no remote and no edges, got a 422).
pub fn repository_write(coll: &str, repo: &str, fields: &[(String, String)], stored: bool) -> Option<Write> {
    if stored && fields.is_empty() {
        None
    } else if stored {
        Some(Write { method: "PUT", path: format!("{coll}/{repo}"), rows: vec![row(repo, fields)], what: format!("repository {repo}") })
    } else {
        Some(Write { method: "POST", path: format!("{coll}/batch"), rows: vec![row(repo, fields)], what: format!("create repository {repo}") })
    }
}

/// (service, path) for every ServiceInstance / ScheduledJob row that names a
/// service: the paths its source file, its binary or its launch script lead
/// to. `file_of` turns a sourcePath edge into a repo path and is asked once
/// per distinct edge. A row with no runsService links nothing.
#[allow(clippy::too_many_arguments)]
pub fn instance_links(
    rows: &[Fields],
    file_of: &mut dyn FnMut(&str) -> String,
    repo_root: &str,
    home: &str,
    units: &[Unit],
    bins: &[(String, String)],
    read_abs: &dyn Fn(&str) -> Option<String>,
) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = Vec::new();
    let mut out = Vec::new();
    for r in rows {
        let get = |k: &str| r.iter().find(|(f, _)| f == k).map(|(_, v)| v.clone()).unwrap_or_default();
        let (svc, src, bin) = (get("runsService"), get("sourcePath"), get("binaryPath"));
        if svc.is_empty() {
            continue;
        }
        let file = if src.is_empty() {
            String::new()
        } else if let Some((_, f)) = files.iter().find(|(n, _)| *n == src) {
            f.clone()
        } else {
            let f = file_of(&src);
            files.push((src.clone(), f.clone()));
            f
        };
        let svc = svc.strip_prefix("service-").unwrap_or(&svc).to_string();
        for p in run_paths(&file, &bin, repo_root, home, units, bins, read_abs) {
            out.push((svc.clone(), p));
        }
    }
    out
}

/// The door's name for a CodeFile row from the edge's IRI local name
/// (code-file-<name> → <name>).
pub fn code_file_door_name(edge: &str) -> &str {
    edge.strip_prefix("code-file-").unwrap_or(edge)
}

/// One write against the door. `rows` is the body: none for a DELETE, one row
/// for PUT/PATCH, many for a batch POST.
#[derive(Debug, PartialEq, Eq)]
pub struct Write {
    pub method: &'static str,
    pub path: String,
    pub rows: Vec<Fields>,
    pub what: String,
}

fn row(name: &str, fields: &[(String, String)]) -> Fields {
    let mut f = vec![("name".to_string(), name.to_string())];
    f.extend(fields.iter().cloned());
    f
}

/// The writes, in order: new units first without their edges (so every
/// edge's target exists when it is written), then every created or changed
/// unit whole, then deletes, then each service's builtFrom merged onto its
/// row (PATCH — the rest of a Service row belongs to other writers). No
/// Service collection served → no builtFrom writes.
pub fn code_writes(coll: &str, svc_coll: Option<&str>, plan: &UnitPlan, built: &[(String, Vec<String>)]) -> Vec<Write> {
    let mut w = Vec::new();
    let bare: Vec<Fields> = plan
        .create
        .iter()
        .map(|(n, f)| row(n, &f.iter().filter(|(k, _)| k != "dependsOn").cloned().collect::<Vec<_>>()))
        .collect();
    if !bare.is_empty() {
        w.push(Write { method: "POST", path: format!("{coll}/batch"), what: format!("create batch of {}", bare.len()), rows: bare });
    }
    // #4562 — a created unit with no edges is already whole after the batch;
    // only one that carries dependsOn is written again (measured 10-16: every
    // create was written twice, one row per request, ~15 writes a minute).
    let has_edges = |f: &[(String, String)]| f.iter().any(|(k, _)| k == "dependsOn");
    for (n, f) in plan.create.iter().filter(|(_, f)| has_edges(f)).chain(plan.update.iter()) {
        w.push(Write { method: "PUT", path: format!("{coll}/{n}"), rows: vec![row(n, f)], what: format!("write {n}") });
    }
    // #4562 — deletes in batches, as the case pass does (#4185)
    for chunk in plan.delete.chunks(crate::cases::DELETE_BATCH_ROWS) {
        w.push(Write {
            method: "POST",
            path: format!("{coll}/delete-batch"),
            rows: chunk.iter().map(|n| vec![("name".to_string(), n.clone())]).collect(),
            what: format!("delete batch of {}: {}", chunk.len(), chunk.join(", ")),
        });
    }
    if let Some(sc) = svc_coll {
        for (svc, us) in built {
            let patch: Fields = us.iter().map(|u| ("builtFrom".to_string(), u.clone())).collect();
            w.push(Write { method: "PATCH", path: format!("{sc}/{svc}"), rows: vec![patch], what: format!("builtFrom {svc}") });
        }
    }
    w
}


/// #4405 reopen 3 — a stored file row that does not name `repo` is restated,
/// even when its content has not moved, so every file comes to name the
/// repository it lives in (6,570 CodeFile rows named none, and another repo's
/// root unit claimed all of them in the blast radius).
pub fn file_needs_repo(stored: &Fields, repo: &str) -> bool {
    repo_of_row(stored) != Some(repo)
}

/// #4499 reopen 3 — the files each test file exercises, by the rule werk-test
/// selects a card's tests with (`exercisers_of`, #4419), so the graph and the
/// selection cannot disagree. Every test gets an entry, an empty one when it
/// names nothing. `files` are the crawled non-test files; `crate_of` names the
/// crate a path sits in.
pub fn exercise_edges(
    tests: &[String],
    files: &[String],
    text_of: &dyn Fn(&str) -> Option<String>,
    crate_of: &dyn Fn(&str) -> Option<String>,
) -> Vec<(String, Vec<String>)> {
    let rows: Vec<werk_test::TestRow> = tests
        .iter()
        .map(|t| werk_test::TestRow { file_path: t.clone(), covers: String::new(), pyramid_layer: String::new(), hermeticity: String::new(), test_concern: String::new(), needs: String::new() })
        .collect();
    let mut out: std::collections::BTreeMap<String, Vec<String>> = tests.iter().map(|t| (t.clone(), Vec::new())).collect();
    let texts: std::collections::HashMap<&str, String> = tests.iter().filter_map(|t| text_of(t).map(|x| (t.as_str(), x))).collect();
    // a test's text from memory; anything else (a step definition's own text) from the reader
    let read = |p: &str| texts.get(p).cloned().or_else(|| text_of(p));
    for f in files {
        // Only for speed (87s → seconds on 1,253 tests): every way the rule
        // can pick a test needs the test to name the file's stem or a data
        // key, to be a feature suite, or to sit in the file's crate — so the
        // rule is asked over those rows alone and answers exactly the same.
        // the stem as the rule takes it (a dotfile's stem is its whole name)
        let stem = std::path::Path::new(f).file_stem().and_then(|x| x.to_str()).unwrap_or("").to_string();
        let mut keys = werk_test::data_file_keys(f);
        if !stem.is_empty() {
            keys.push(stem);
        }
        let home = crate_of(f).map(|c| format!("{c}/"));
        let candidates: Vec<werk_test::TestRow> = rows.iter()
            .filter(|r| werk_test::is_feature_suite(&r.file_path)
                || home.as_deref().is_some_and(|h| r.file_path.starts_with(h))
                || texts.get(r.file_path.as_str()).is_some_and(|t| keys.iter().any(|k| t.contains(k.as_str()))))
            .cloned().collect();
        if candidates.is_empty() {
            continue;
        }
        for t in werk_test::exercisers_of(f, &candidates, &read, crate_of, None) {
            if let Some(v) = out.get_mut(&t) {
                v.push(f.clone());
            }
        }
    }
    out.into_iter().map(|(t, mut v)| { v.sort(); v.dedup(); (t, v) }).collect()
}

/// #4499 reopen 3 — the PATCH bodies that bring each test row's `exercises`
/// to what the rule says: only rows whose stored set differs, and an empty set
/// is written as null (a JSON merge patch clears the field). `stored` is the
/// row's current values as door names; `name_of` turns a path into its row name.
pub fn exercise_patches(
    want: &[(String, Vec<String>)],
    stored: &dyn Fn(&str) -> Vec<String>,
    name_of: &dyn Fn(&str) -> String,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (test, files) in want {
        // #4562 — compared as sets, the way the store holds them: the door
        // lists an empty set as one "" (45 rows re-patched every crawl,
        // measured 10-16), and two files with one row name are one member.
        let mut names: Vec<String> = files.iter().map(|f| name_of(f)).collect();
        names.sort();
        names.dedup();
        let mut held: Vec<String> = stored(test).into_iter().filter(|h| !h.is_empty()).collect();
        held.sort();
        held.dedup();
        if names == held {
            continue;
        }
        let body = if names.is_empty() {
            "{\"exercises\":null}".to_string()
        } else {
            format!("{{\"exercises\":[{}]}}", names.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(","))
        };
        out.push((name_of(test), body));
    }
    out
}

/// #4405 reopen 6 — whether a stored file row belongs to this pass. A pass over
/// another repository (`--target`) owns only that repository's rows; the home
/// pass owns its own and the legacy rows that name none. Nothing else is ever
/// planned for a delete: a personal-site pass must not delete chorus's 6,605.
/// #4405 reopen 7 — the CodeFile rows a pass may compare or delete: its own
/// repository's. The 10-14 nightly's full pass re-read every row unscoped and
/// called all 1,792 personal-site files stale. Every read goes through here.
pub fn rows_in_pass(rows: Vec<crate::InGraph>, repo: &str, home_pass: bool) -> Vec<crate::InGraph> {
    rows.into_iter().filter(|r| row_in_pass(&r.other, repo, home_pass)).collect()
}

pub fn row_in_pass(stored: &Fields, repo: &str, home_pass: bool) -> bool {
    match repo_of_row(stored) {
        Some(r) => r == repo,
        None => home_pass,
    }
}

#[cfg(test)]
mod exercise_patches_4562 {
    use super::exercise_patches;

    fn plan(want: Vec<(&str, Vec<&str>)>, held: Vec<&'static str>) -> Vec<(String, String)> {
        let want: Vec<(String, Vec<String>)> = want.into_iter()
            .map(|(t, f)| (t.to_string(), f.into_iter().map(String::from).collect())).collect();
        exercise_patches(&want, &move |_| held.iter().map(|s| s.to_string()).collect(), &|f| f.replace('/', "-"))
    }

    #[test]
    fn an_empty_set_the_door_lists_as_one_blank_is_unchanged() {
        assert!(plan(vec![("t.sh", vec![])], vec![""]).is_empty());
    }

    #[test]
    fn two_files_with_one_row_name_match_the_one_stored_member() {
        assert!(plan(vec![("t.sh", vec!["a/b", "a/b"])], vec!["a-b"]).is_empty());
    }

    /// NEGATIVE PROOF: a set that really changed is still patched.
    #[test]
    fn negative_proof_a_changed_set_is_still_patched() {
        let p = plan(vec![("t.sh", vec!["a/b", "c/d"])], vec!["a-b"]);
        assert_eq!(p.len(), 1);
        assert!(p[0].1.contains("c-d"), "{}", p[0].1);
        assert_eq!(plan(vec![("t.sh", vec![])], vec!["a-b"]).len(), 1, "a set emptied is patched to null");
    }
}
